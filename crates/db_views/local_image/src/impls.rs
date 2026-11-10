use crate::LocalImageView;
use diesel::{ExpressionMethods, JoinOnDsl, NullableExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl;
use i_love_jesus::SortDirection;
use lemmy_db_schema::{
  source::{
    images::LocalImage,
    media::{MediaObject, MediaReference, media_object_keys},
    person::Person,
    post::Post,
  },
  utils::limit_fetch,
};
use lemmy_db_schema_file::{
  PersonId,
  newtypes::LocalSiteId,
  schema::{media_object, media_reference, person, post},
};
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  pagination::{
    CursorData,
    PagedResponse,
    PaginationCursor,
    PaginationCursorConversion,
    paginate_response,
  },
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

#[derive(diesel::Queryable)]
struct MediaReferenceRow {
  reference: MediaReference,
  media: MediaObject,
  person: Person,
  post: Option<Post>,
}

impl From<MediaReferenceRow> for LocalImageView {
  fn from(row: MediaReferenceRow) -> Self {
    Self {
      local_image: LocalImage {
        pictrs_alias: row.media.canonical_pictrs_alias,
        published_at: row.reference.created_at,
        person_id: row.reference.owner_person_id,
        thumbnail_for_post_id: row.reference.post_id,
      },
      person: row.person,
      post: row.post,
    }
  }
}

macro_rules! media_query {
  () => {
    media_reference::table
      .inner_join(media_object::table.on(media_object::id.eq(media_reference::media_id)))
      .inner_join(person::table.on(person::id.nullable().eq(media_reference::owner_person_id)))
      .left_join(post::table.on(post::id.nullable().eq(media_reference::post_id)))
      .select((
        MediaReference::as_select(),
        MediaObject::as_select(),
        Person::as_select(),
        Option::<Post>::as_select(),
      ))
  };
}

impl PaginationCursorConversion for MediaReferenceRow {
  type PaginatedType = MediaObject;

  fn to_cursor(&self) -> CursorData {
    CursorData::new_plain(self.media.canonical_pictrs_alias.clone())
  }

  async fn from_cursor(
    cursor: CursorData,
    pool: &mut DbPool<'_>,
  ) -> LemmyResult<Self::PaginatedType> {
    let conn = &mut get_conn(pool).await?;
    media_object::table
      .filter(media_object::canonical_pictrs_alias.eq(cursor.plain()))
      .first(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }
}

impl LocalImageView {
  pub async fn get_all_paged_by_person_id(
    pool: &mut DbPool<'_>,
    person_id: PersonId,
    local_site_id: LocalSiteId,
    cursor_data: Option<PaginationCursor>,
    limit: Option<i64>,
  ) -> LemmyResult<PagedResponse<Self>> {
    let limit = limit_fetch(limit, None)?;

    let query = media_query!()
      .filter(media_reference::local_site_id.eq(local_site_id))
      .filter(media_reference::owner_person_id.eq(Some(person_id)))
      .limit(limit)
      .into_boxed();

    let paginated_query = <MediaReferenceRow as PaginationCursorConversion>::paginate(
      query,
      &cursor_data,
      SortDirection::Asc,
      pool,
    )
    .await?
    .then_order_by(media_object_keys::canonical_pictrs_alias);

    let conn = &mut get_conn(pool).await?;
    let rows = paginated_query
      .load::<MediaReferenceRow>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    let items = rows.into_iter().map(LocalImageView::from).collect();
    paginate_response(items, limit, cursor_data)
  }

  pub async fn get_all_by_person_id(
    pool: &mut DbPool<'_>,
    person_id: PersonId,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    let rows = media_query!()
      .filter(media_reference::owner_person_id.eq(Some(person_id)))
      .load::<MediaReferenceRow>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    Ok(rows.into_iter().map(LocalImageView::from).collect())
  }

  pub async fn get_all_paged(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
    cursor_data: Option<PaginationCursor>,
    limit: Option<i64>,
  ) -> LemmyResult<PagedResponse<Self>> {
    let limit = limit_fetch(limit, None)?;

    let query = media_query!()
      .filter(media_reference::local_site_id.eq(local_site_id))
      .limit(limit)
      .into_boxed();

    let paginated_query = <MediaReferenceRow as PaginationCursorConversion>::paginate(
      query,
      &cursor_data,
      SortDirection::Asc,
      pool,
    )
    .await?
    .then_order_by(media_object_keys::canonical_pictrs_alias);

    let conn = &mut get_conn(pool).await?;
    let rows = paginated_query
      .load::<MediaReferenceRow>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    let items = rows.into_iter().map(LocalImageView::from).collect();
    paginate_response(items, limit, cursor_data)
  }
}

impl PaginationCursorConversion for LocalImageView {
  type PaginatedType = MediaObject;

  fn to_cursor(&self) -> CursorData {
    CursorData::new_plain(self.local_image.pictrs_alias.clone())
  }

  async fn from_cursor(
    cursor: CursorData,
    pool: &mut DbPool<'_>,
  ) -> LemmyResult<Self::PaginatedType> {
    let conn = &mut get_conn(pool).await?;
    media_object::table
      .filter(media_object::canonical_pictrs_alias.eq(cursor.plain()))
      .first(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use lemmy_db_schema::{
    source::{
      instance::Instance,
      local_site::{LocalSite, LocalSiteInsertForm},
      media::{MediaObjectInsertForm, MediaReferenceInsertForm},
      person::PersonInsertForm,
      site::{Site, SiteUpsertForm},
    },
    test_data::TestData,
  };
  use lemmy_diesel_utils::{connection::build_db_pool_for_tests, traits::Crud};
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn lists_only_active_site_references() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();

    let second_instance = Instance::read_or_create(pool, "local-image-site.example").await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("local image site".into(), second_instance.id),
    )
    .await?;
    let second_system = Person::create(
      pool,
      &PersonInsertForm {
        local: Some(false),
        ..PersonInsertForm::test_form(second_instance.id, "local_image_system")
      },
    )
    .await?;
    let second_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(
        second_site.id,
        second_system.id,
        second_instance.domain.clone(),
      ),
    )
    .await?;

    let create_media = |alias: String, hash: String| MediaObjectInsertForm {
      content_hash: hash,
      canonical_pictrs_alias: alias,
      size_bytes: 1,
      width: 1,
      height: 1,
      content_type: "image/png".to_string(),
      blurhash: None,
    };
    let (first_media, _) = MediaObject::create_or_read(
      pool,
      &create_media(
        format!("first_alias_{unique}"),
        format!("first_hash_{unique}"),
      ),
    )
    .await?;
    let (second_media, _) = MediaObject::create_or_read(
      pool,
      &create_media(
        format!("second_alias_{unique}"),
        format!("second_hash_{unique}"),
      ),
    )
    .await?;

    for (local_site_id, media_id, owner) in [
      (data.local_site.id, first_media.id, data.person.id),
      (second_local_site.id, second_media.id, second_system.id),
    ] {
      MediaReference::ensure(
        pool,
        &MediaReferenceInsertForm {
          local_site_id,
          media_id,
          owner_person_id: Some(owner),
          post_id: None,
          purpose: "other".to_string(),
        },
      )
      .await?;
    }

    let first_list = LocalImageView::get_all_paged(pool, data.local_site.id, None, None).await?;
    assert_eq!(first_list.len(), 1);
    assert_eq!(
      first_list[0].local_image.pictrs_alias,
      first_media.canonical_pictrs_alias
    );
    let second_list = LocalImageView::get_all_paged(pool, second_local_site.id, None, None).await?;
    assert_eq!(second_list.len(), 1);
    assert_eq!(
      second_list[0].local_image.pictrs_alias,
      second_media.canonical_pictrs_alias
    );

    let person_list = LocalImageView::get_all_paged_by_person_id(
      pool,
      data.person.id,
      data.local_site.id,
      None,
      None,
    )
    .await?;
    assert_eq!(person_list.len(), 1);

    Instance::delete(pool, second_instance.id).await?;
    data.delete(pool).await
  }
}
