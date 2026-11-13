use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_utils::{
  context::LemmyContext,
  send_activity::{ActivityChannel, SendActivityData},
  utils::check_community_global_mod_action,
};
use lemmy_db_schema::source::community::{Community, CommunityActions};
use lemmy_db_schema_file::enums::CommunityFollowerState;
use lemmy_db_views_community::api::ApproveCommunityPendingFollower;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::SuccessResponse;
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

pub async fn post_pending_follows_approve(
  Json(data): Json<ApproveCommunityPendingFollower>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<SuccessResponse>> {
  let community = Community::read(&mut context.pool(), data.community_id).await?;
  check_community_global_mod_action(&local_user_view, &community, false, &mut context.pool())
    .await?;

  let community_actions =
    CommunityActions::read(&mut context.pool(), data.community_id, data.follower_id).await?;
  if community_actions.follow_state != Some(CommunityFollowerState::ApprovalRequired) {
    return Err(LemmyErrorType::CouldntUpdate.into());
  }
  let (state, activity_data) = if data.approve {
    (
      CommunityFollowerState::Accepted,
      SendActivityData::PrivateCommunityAcceptFollower {
        community_id: data.community_id,
        person_id: data.follower_id,
        follow_activity_id: community_actions.follow_activity_id,
      },
    )
  } else {
    (
      CommunityFollowerState::Denied,
      SendActivityData::PrivateCommunityRejectFollower {
        community_id: data.community_id,
        person_id: data.follower_id,
        follow_activity_id: community_actions.follow_activity_id,
      },
    )
  };
  CommunityActions::approve_private_community_follower(
    &mut context.pool(),
    data.community_id,
    data.follower_id,
    local_user_view.person.id,
    state,
  )
  .await?;
  ActivityChannel::submit_activity(activity_data, &context)?;

  Ok(Json(SuccessResponse::default()))
}
