use crate::source::images::{ImageDetails, ImageDetailsInsertForm, RemoteImage};
use diesel::{ExpressionMethods, QueryDsl, dsl::exists, insert_into, select};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::schema::{image_details, remote_image};
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  dburl::DbUrl,
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};
use url::Url;

impl RemoteImage {
  pub async fn create(pool: &mut DbPool<'_>, links: Vec<Url>) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    let forms = links
      .into_iter()
      .map(|url| remote_image::dsl::link.eq::<DbUrl>(url.into()))
      .collect::<Vec<_>>();
    insert_into(remote_image::table)
      .values(forms)
      .on_conflict_do_nothing()
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  pub async fn validate(pool: &mut DbPool<'_>, link_: DbUrl) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;

    select(exists(
      remote_image::table.filter(remote_image::link.eq(link_)),
    ))
    .get_result::<bool>(conn)
    .await?
    .then_some(())
    .ok_or(LemmyErrorType::NotFound.into())
  }
}

impl ImageDetails {
  pub async fn create(pool: &mut DbPool<'_>, form: &ImageDetailsInsertForm) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;

    insert_into(image_details::table)
      .values(form)
      .on_conflict_do_nothing()
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }
}
