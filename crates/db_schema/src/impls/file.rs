use crate::source::file::{
  FileObject, FileObjectInsertForm, FileReference, FileReferenceInsertForm,
};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{FileId, LocalSiteId},
  schema::{file_object, file_reference},
};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};
use uuid::Uuid;

impl FileObject {
  pub async fn read_by_hash(
    pool: &mut DbPool<'_>,
    content_hash: &str,
  ) -> LemmyResult<Option<Self>> {
    let conn = &mut get_conn(pool).await?;
    file_object::table
      .filter(file_object::content_hash.eq(content_hash))
      .first(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn create_or_read(
    pool: &mut DbPool<'_>,
    form: &FileObjectInsertForm,
  ) -> LemmyResult<(Self, bool)> {
    let conn = &mut get_conn(pool).await?;
    let inserted = insert_into(file_object::table)
      .values(form)
      .on_conflict(file_object::content_hash)
      .do_nothing()
      .get_result::<Self>(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    if let Some(object) = inserted {
      return Ok((object, true));
    }

    let object = file_object::table
      .filter(file_object::content_hash.eq(&form.content_hash))
      .first::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    Ok((object, false))
  }

  pub async fn read(id: FileId, pool: &mut DbPool<'_>) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    file_object::table
      .find(id)
      .first::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn reference_count(&self, pool: &mut DbPool<'_>) -> LemmyResult<i64> {
    let conn = &mut get_conn(pool).await?;
    file_reference::table
      .filter(file_reference::file_id.eq(self.id))
      .count()
      .get_result(conn)
      .await
      .map_err(Into::into)
  }

  pub async fn delete(self, pool: &mut DbPool<'_>) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    diesel::delete(file_object::table.find(self.id))
      .execute(conn)
      .await
      .map_err(Into::into)
  }
}

impl FileReference {
  pub async fn create(pool: &mut DbPool<'_>, form: &FileReferenceInsertForm) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    insert_into(file_reference::table)
      .values(form)
      .get_result(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  pub async fn read_by_public_id(
    pool: &mut DbPool<'_>,
    site_id: LocalSiteId,
    public_id: Uuid,
  ) -> LemmyResult<Option<Self>> {
    let conn = &mut get_conn(pool).await?;
    file_reference::table
      .filter(file_reference::local_site_id.eq(site_id))
      .filter(file_reference::public_id.eq(public_id))
      .first(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn list_by_owner(
    pool: &mut DbPool<'_>,
    site_id: LocalSiteId,
    owner_id: PersonId,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    file_reference::table
      .filter(file_reference::local_site_id.eq(site_id))
      .filter(file_reference::owner_person_id.eq(owner_id))
      .order(file_reference::published_at.desc())
      .load(conn)
      .await
      .map_err(Into::into)
  }

  pub async fn delete(self, pool: &mut DbPool<'_>) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    diesel::delete(file_reference::table.find(self.id))
      .execute(conn)
      .await
      .map_err(Into::into)
  }
}
