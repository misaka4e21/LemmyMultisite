use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_utils::{context::LemmyContext, utils::is_admin};
use lemmy_db_schema::source::custom_emoji::CustomEmoji;
use lemmy_db_views_custom_emoji::api::DeleteCustomEmoji;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::SuccessResponse;
use lemmy_utils::error::LemmyResult;

pub async fn delete_custom_emoji(
  Json(data): Json<DeleteCustomEmoji>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<SuccessResponse>> {
  // Make sure user is an admin
  is_admin(&local_user_view)?;

  CustomEmoji::delete_for_site(
    &mut context.pool(),
    data.id,
    local_user_view.local_user.local_site_id,
  )
  .await?;

  Ok(Json(SuccessResponse::default()))
}
