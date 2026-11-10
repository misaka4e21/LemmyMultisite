use crate::source::post_thumbnail::{PostThumbnailSource, PostThumbnailSourceForm};
use diesel::{ExpressionMethods, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::schema::post_thumbnail_source;
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl PostThumbnailSource {
  pub async fn upsert(pool: &mut DbPool<'_>, form: &PostThumbnailSourceForm) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    insert_into(post_thumbnail_source::table)
      .values(form)
      .on_conflict(post_thumbnail_source::post_id)
      .do_update()
      .set((
        post_thumbnail_source::source_hash.eq(&form.source_hash),
        post_thumbnail_source::pictrs_alias.eq(&form.pictrs_alias),
      ))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
    Ok(())
  }
}
