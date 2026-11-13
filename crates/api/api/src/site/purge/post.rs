use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_utils::{
  context::LemmyContext,
  send_activity::{ActivityChannel, SendActivityData},
  utils::is_admin,
};
use lemmy_db_schema::{
  impls::presence::delete_post_for_site,
  source::{
    local_user::LocalUser,
    modlog::{Modlog, ModlogInsertForm},
    post::Post,
  },
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_post::api::PurgePost;
use lemmy_db_views_site::api::SuccessResponse;
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

pub async fn purge_post(
  Json(data): Json<PurgePost>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<SuccessResponse>> {
  // Only let admin purge an item
  is_admin(&local_user_view)?;
  if context.site_view()?.local_site.id != local_user_view.local_user.local_site_id {
    return Err(LemmyErrorType::NotFound.into());
  }

  // Read the post to get the community_id
  let post = Post::read(&mut context.pool(), data.post_id).await?;
  if post.origin_local_site_id != Some(local_user_view.local_user.local_site_id) {
    return Err(LemmyErrorType::NotFound.into());
  }

  // Also check that you're a higher admin
  LocalUser::is_higher_admin_check(
    &mut context.pool(),
    local_user_view.person.id,
    vec![post.creator_id],
  )
  .await?;

  delete_post_for_site(
    &mut context.pool(),
    local_user_view.local_user.local_site_id,
    data.post_id,
  )
  .await?;

  // Mod tables
  let form =
    ModlogInsertForm::admin_purge_post(local_user_view.person.id, post.community_id, &data.reason);
  Modlog::create(&mut context.pool(), &[form]).await?;

  ActivityChannel::submit_activity(
    SendActivityData::RemovePost {
      post,
      moderator: local_user_view.person.clone(),
      reason: data.reason.clone(),
      removed: true,
      with_replies: false,
    },
    &context,
  )?;

  Ok(Json(SuccessResponse::default()))
}
