use crate::source::{
  media::MediaReference,
  protected_media_reference::{ProtectedMediaReference, ProtectedMediaReferenceInsertForm},
};
use diesel::{ExpressionMethods, QueryDsl, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  newtypes::LocalSiteId,
  schema::{media_reference, protected_media_reference},
};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

#[derive(diesel::QueryableByName)]
struct ExistsResult {
  #[diesel(sql_type = diesel::sql_types::Bool)]
  value: bool,
}

impl ProtectedMediaReference {
  /// Adds an idempotent access-control binding for one file or image use.
  pub async fn ensure(
    pool: &mut DbPool<'_>,
    form: &ProtectedMediaReferenceInsertForm,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let resource_exists = match (form.post_id, form.private_message_id) {
      (Some(post_id), None) => {
        diesel::sql_query(
          "SELECT EXISTS (SELECT 1 FROM post_site WHERE post_id = $1 AND local_site_id = $2) AS value",
        )
        .bind::<diesel::sql_types::Integer, _>(post_id.0)
        .bind::<diesel::sql_types::Integer, _>(form.local_site_id.0)
        .get_result::<ExistsResult>(conn)
        .await?
        .value
      }
      (None, Some(private_message_id)) => {
        diesel::sql_query(
          "SELECT EXISTS (SELECT 1 FROM private_message_site WHERE private_message_id = $1 AND local_site_id = $2) AS value",
        )
        .bind::<diesel::sql_types::Integer, _>(private_message_id.0)
        .bind::<diesel::sql_types::Integer, _>(form.local_site_id.0)
        .get_result::<ExistsResult>(conn)
        .await?
        .value
      }
      _ => false,
    };
    if !resource_exists {
      return Err(LemmyErrorType::NotFound.into());
    }

    let reference_exists = match (form.file_reference_id, form.media_reference_id) {
      (Some(file_reference_id), None) => {
        diesel::sql_query(
          "SELECT EXISTS (SELECT 1 FROM file_reference WHERE id = $1 AND local_site_id = $2) AS value",
        )
        .bind::<diesel::sql_types::Integer, _>(file_reference_id)
        .bind::<diesel::sql_types::Integer, _>(form.local_site_id.0)
        .get_result::<ExistsResult>(conn)
        .await?
        .value
      }
      (None, Some(media_reference_id)) => {
        diesel::sql_query(
          "SELECT EXISTS (SELECT 1 FROM media_reference WHERE id = $1 AND local_site_id = $2) AS value",
        )
        .bind::<diesel::sql_types::BigInt, _>(media_reference_id)
        .bind::<diesel::sql_types::Integer, _>(form.local_site_id.0)
        .get_result::<ExistsResult>(conn)
        .await?
        .value
      }
      _ => false,
    };
    if !reference_exists {
      return Err(LemmyErrorType::NotFound.into());
    }

    insert_into(protected_media_reference::table)
      .values(form)
      .on_conflict_do_nothing()
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    Ok(())
  }

  pub async fn list_for_file_reference(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
    file_reference_id: i32,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    protected_media_reference::table
      .filter(protected_media_reference::local_site_id.eq(local_site_id))
      .filter(protected_media_reference::file_reference_id.eq(Some(file_reference_id)))
      .load(conn)
      .await
      .map_err(Into::into)
  }

  pub async fn list_for_media_reference(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
    media_reference_id: i64,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    protected_media_reference::table
      .filter(protected_media_reference::local_site_id.eq(local_site_id))
      .filter(protected_media_reference::media_reference_id.eq(Some(media_reference_id)))
      .load(conn)
      .await
      .map_err(Into::into)
  }
}

impl MediaReference {
  pub async fn list_for_site_and_media(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
    media_id: i64,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    media_reference::table
      .filter(media_reference::local_site_id.eq(local_site_id))
      .filter(media_reference::media_id.eq(media_id))
      .load(conn)
      .await
      .map_err(Into::into)
  }
}
