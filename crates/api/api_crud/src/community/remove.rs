use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_utils::{
  build_response::build_community_response,
  context::LemmyContext,
  notify::notify_mod_action,
  send_activity::{ActivityChannel, SendActivityData},
  utils::{check_community_mod_action, is_admin},
};
use lemmy_db_schema::{
  impls::presence::{SiteReportObject, resolve_reports_for_site, set_community_site_removed},
  source::{
    community::{Community, CommunityUpdateForm},
    modlog::{Modlog, ModlogInsertForm},
  },
};
use lemmy_db_views_community::api::{CommunityResponse, RemoveCommunity};
use lemmy_db_views_community_moderator::CommunityModeratorView;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::LemmyResult;

pub async fn remove_community(
  Json(data): Json<RemoveCommunity>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<CommunityResponse>> {
  let community = Community::read(&mut context.pool(), data.community_id).await?;
  check_community_mod_action(&local_user_view, &community, true, &mut context.pool()).await?;

  // Verify its an admin (only an admin can remove a community)
  is_admin(&local_user_view)?;

  // Do the remove
  let community_id = data.community_id;
  let removed = data.removed;
  let active_site_id = local_user_view.local_user.local_site_id;
  let is_origin = community.origin_local_site_id == Some(active_site_id);
  let community = if is_origin {
    Community::update(
      &mut context.pool(),
      community_id,
      &CommunityUpdateForm {
        removed: Some(removed),
        ..Default::default()
      },
    )
    .await?
  } else {
    set_community_site_removed(&mut context.pool(), active_site_id, community_id, removed).await?;
    community
  };

  resolve_reports_for_site(
    &mut context.pool(),
    active_site_id,
    SiteReportObject::Community(community_id),
    None,
    local_user_view.person.id,
  )
  .await?;

  // Mod
  let community_owner =
    CommunityModeratorView::top_mod_for_community(&mut context.pool(), data.community_id).await?;
  let form = ModlogInsertForm::admin_remove_community(
    &local_user_view.person,
    data.community_id,
    community_owner,
    removed,
    &data.reason,
  );
  let action = Modlog::create(&mut context.pool(), &[form]).await?;
  notify_mod_action(action.clone(), context.app_data());

  if is_origin {
    ActivityChannel::submit_activity(
      SendActivityData::RemoveCommunity {
        moderator: local_user_view.person.clone(),
        community,
        reason: data.reason.clone(),
        removed: data.removed,
      },
      &context,
    )?;
  }

  build_community_response(&context, local_user_view, community_id).await
}
