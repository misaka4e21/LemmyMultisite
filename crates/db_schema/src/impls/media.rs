use crate::source::media::{
  MediaObject,
  MediaObjectInsertForm,
  MediaReference,
  MediaReferenceInsertForm,
};
use diesel::{
  ExpressionMethods,
  JoinOnDsl,
  OptionalExtension,
  QueryDsl,
  dsl::{delete, exists, insert_into},
  select,
};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::schema::{media_object, media_reference};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl MediaObject {
  /// Inserts a new media object, or returns the existing object with the same content hash.
  ///
  /// The boolean is true when this call created the object. Callers use the canonical alias from
  /// the returned object, so duplicate uploads can be purged from pict-rs.
  pub async fn create_or_read(
    pool: &mut DbPool<'_>,
    form: &MediaObjectInsertForm,
  ) -> LemmyResult<(Self, bool)> {
    let conn = &mut get_conn(pool).await?;

    let existing = media_object::table
      .filter(media_object::content_hash.eq(&form.content_hash))
      .first::<Self>(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    if let Some(existing) = existing {
      return Ok((existing, false));
    }

    let inserted = insert_into(media_object::table)
      .values(form)
      .on_conflict(media_object::content_hash)
      .do_nothing()
      .get_result::<Self>(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    if let Some(inserted) = inserted {
      return Ok((inserted, true));
    }

    media_object::table
      .filter(media_object::content_hash.eq(&form.content_hash))
      .first::<Self>(conn)
      .await
      .map(|existing| (existing, false))
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  pub async fn read_by_alias(pool: &mut DbPool<'_>, alias: &str) -> LemmyResult<Option<Self>> {
    let conn = &mut get_conn(pool).await?;
    media_object::table
      .filter(media_object::canonical_pictrs_alias.eq(alias))
      .first(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  /// Deletes one media object by alias. References are removed by the foreign key cascade.
  pub async fn delete_by_alias(pool: &mut DbPool<'_>, alias: &str) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    delete(media_object::table.filter(media_object::canonical_pictrs_alias.eq(alias)))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  /// Deletes every media object for the given aliases. References are removed by the cascade.
  pub async fn delete_by_aliases(pool: &mut DbPool<'_>, aliases: &[String]) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    delete(media_object::table.filter(media_object::canonical_pictrs_alias.eq_any(aliases)))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }
}

impl MediaReference {
  /// Ensures that the active site has a reference to a media object.
  pub async fn ensure(pool: &mut DbPool<'_>, form: &MediaReferenceInsertForm) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let exists = select(exists(
      media_reference::table
        .filter(media_reference::local_site_id.eq(form.local_site_id))
        .filter(media_reference::media_id.eq(form.media_id))
        .filter(media_reference::purpose.eq(&form.purpose)),
    ))
    .get_result::<bool>(conn)
    .await
    .with_lemmy_type(LemmyErrorType::NotFound)?;
    if exists {
      return Ok(());
    }

    insert_into(media_reference::table)
      .values(form)
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    Ok(())
  }

  pub async fn validate_by_alias_and_user(
    pool: &mut DbPool<'_>,
    alias: &str,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    person_id: lemmy_db_schema_file::PersonId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    select(exists(
      media_reference::table
        .inner_join(media_object::table.on(media_object::id.eq(media_reference::media_id)))
        .filter(media_object::canonical_pictrs_alias.eq(alias))
        .filter(media_reference::local_site_id.eq(local_site_id))
        .filter(media_reference::owner_person_id.eq(Some(person_id))),
    ))
    .get_result::<bool>(conn)
    .await?
    .then_some(())
    .ok_or(LemmyErrorType::NotFound.into())
  }

  /// Deletes the active site's reference and returns true when no other site references the media.
  pub async fn delete_for_site_and_user(
    pool: &mut DbPool<'_>,
    alias: &str,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    person_id: lemmy_db_schema_file::PersonId,
  ) -> LemmyResult<bool> {
    let conn = &mut get_conn(pool).await?;
    let media = media_object::table
      .filter(media_object::canonical_pictrs_alias.eq(alias))
      .first::<MediaObject>(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    let Some(media) = media else {
      return Ok(false);
    };

    delete(
      media_reference::table
        .filter(media_reference::media_id.eq(media.id))
        .filter(media_reference::local_site_id.eq(local_site_id))
        .filter(media_reference::owner_person_id.eq(Some(person_id))),
    )
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::Deleted)?;

    let remaining = media_reference::table
      .filter(media_reference::media_id.eq(media.id))
      .count()
      .get_result::<i64>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    if remaining == 0 {
      delete(media_object::table.find(media.id))
        .execute(conn)
        .await
        .with_lemmy_type(LemmyErrorType::Deleted)?;
      Ok(true)
    } else {
      Ok(false)
    }
  }

  pub async fn exists_for_site(
    pool: &mut DbPool<'_>,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    media_id: i64,
  ) -> LemmyResult<bool> {
    let conn = &mut get_conn(pool).await?;
    select(exists(
      media_reference::table
        .filter(media_reference::local_site_id.eq(local_site_id))
        .filter(media_reference::media_id.eq(media_id)),
    ))
    .get_result::<bool>(conn)
    .await
    .with_lemmy_type(LemmyErrorType::NotFound)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    source::{
      instance::Instance,
      local_site::{LocalSite, LocalSiteInsertForm},
      person::{Person, PersonInsertForm},
      site::{Site, SiteUpsertForm},
    },
    test_data::TestData,
  };
  use chrono::Utc;
  use diesel::{ExpressionMethods, QueryDsl};
  use diesel_async::RunQueryDsl;
  use lemmy_db_schema_file::schema::{media_object, media_reference};
  use lemmy_diesel_utils::{
    connection::{build_db_pool_for_tests, get_conn},
    traits::Crud,
  };
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn identical_uploads_share_media_object() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;

    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let content_hash = format!("same-content-{unique}");
    let first_alias = format!("first_alias_{unique}");
    let form = MediaObjectInsertForm {
      content_hash: content_hash.clone(),
      canonical_pictrs_alias: first_alias.clone(),
      size_bytes: 42,
      width: 10,
      height: 20,
      content_type: "image/png".to_string(),
      blurhash: None,
    };
    let (first, created_first) = MediaObject::create_or_read(pool, &form).await?;
    assert!(created_first);

    let duplicate_form = MediaObjectInsertForm {
      canonical_pictrs_alias: format!("second_alias_{unique}"),
      ..form.clone()
    };
    let (second, created_second) = MediaObject::create_or_read(pool, &duplicate_form).await?;
    assert!(!created_second);
    assert_eq!(first.id, second.id);
    assert_eq!(second.canonical_pictrs_alias, first_alias);

    let second_domain = format!("second-media-{unique}.example");
    let second_instance = Instance::read_or_create(pool, &second_domain).await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("second media site".into(), second_instance.id),
    )
    .await?;
    let second_system = Person::create(
      pool,
      &PersonInsertForm {
        local: Some(false),
        ..PersonInsertForm::test_form(second_instance.id, "second_media_system")
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

    for (local_site_id, owner) in [
      (data.local_site.id, data.person.id),
      (second_local_site.id, second_system.id),
    ] {
      MediaReference::ensure(
        pool,
        &MediaReferenceInsertForm {
          local_site_id,
          media_id: first.id,
          owner_person_id: Some(owner),
          post_id: None,
          purpose: "other".to_string(),
        },
      )
      .await?;
    }

    let conn = &mut get_conn(pool).await?;
    let reference_count = media_reference::table
      .filter(media_reference::media_id.eq(first.id))
      .count()
      .get_result::<i64>(conn)
      .await?;
    let object_count = media_object::table
      .filter(media_object::content_hash.eq(&content_hash))
      .count()
      .get_result::<i64>(conn)
      .await?;
    assert_eq!(reference_count, 2);
    assert_eq!(object_count, 1);
    assert!(MediaReference::exists_for_site(pool, data.local_site.id, first.id).await?);
    assert!(MediaReference::exists_for_site(pool, second_local_site.id, first.id).await?);

    let remove_last = MediaReference::delete_for_site_and_user(
      pool,
      &first.canonical_pictrs_alias,
      data.local_site.id,
      data.person.id,
    )
    .await?;
    assert!(!remove_last);
    let conn = &mut get_conn(pool).await?;
    let remaining = media_reference::table
      .filter(media_reference::media_id.eq(first.id))
      .count()
      .get_result::<i64>(conn)
      .await?;
    assert_eq!(remaining, 1);
    assert!(!MediaReference::exists_for_site(pool, data.local_site.id, first.id).await?);
    assert!(MediaReference::exists_for_site(pool, second_local_site.id, first.id).await?);

    let remove_last = MediaReference::delete_for_site_and_user(
      pool,
      &first.canonical_pictrs_alias,
      second_local_site.id,
      second_system.id,
    )
    .await?;
    assert!(remove_last);
    let conn = &mut get_conn(pool).await?;
    let remaining = media_object::table
      .filter(media_object::content_hash.eq(&content_hash))
      .count()
      .get_result::<i64>(conn)
      .await?;
    assert_eq!(remaining, 0);

    Instance::delete(pool, second_instance.id).await?;
    data.delete(pool).await
  }
}
