use activitypub_federation::config::Data;
use actix_web::web::Json;
use either::Either;
use lemmy_api_utils::{
  context::LemmyContext,
  send_activity::{ActivityChannel, SendActivityData},
  utils::check_community_mod_action,
};
use lemmy_db_schema::impls::presence::{
  SiteReportId,
  combined_report_id,
  read_report_site_state,
  resolve_report_for_site,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_report_combined::{
  ReportCombinedViewInternal,
  api::{PostReportResponse, ResolvePostReport},
};
use lemmy_diesel_utils::utils::diesel_string_update;
use lemmy_utils::error::LemmyResult;

/// Resolves or unresolves a post report and notifies the moderators of the community
pub async fn resolve_post_report(
  Json(data): Json<ResolvePostReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<PostReportResponse>> {
  let report_id = data.report_id;
  let person = &local_user_view.person;
  let site_id = local_user_view.local_user.local_site_id;
  let combined_id = combined_report_id(&mut context.pool(), SiteReportId::Post(report_id)).await?;
  read_report_site_state(&mut context.pool(), site_id, combined_id).await?;
  let report =
    ReportCombinedViewInternal::read_post_report(&mut context.pool(), report_id, person).await?;

  let person = &local_user_view.person;
  check_community_mod_action(
    &local_user_view,
    &report.community,
    true,
    &mut context.pool(),
  )
  .await?;

  resolve_report_for_site(
    &mut context.pool(),
    site_id,
    combined_id,
    person.id,
    data.resolved,
    diesel_string_update(data.conclusion.as_deref()),
  )
  .await?;

  let mut post_report_view =
    ReportCombinedViewInternal::read_post_report(&mut context.pool(), report_id, person).await?;
  let state = read_report_site_state(&mut context.pool(), site_id, combined_id).await?;
  post_report_view.post_report.resolved = state.resolved;
  post_report_view.post_report.resolver_id = state.resolver_id;
  post_report_view.post_report.conclusion = state.conclusion;
  post_report_view.post_report.updated_at = state.updated_at;
  post_report_view.resolver = Some(person.clone());

  ActivityChannel::submit_activity(
    SendActivityData::SendResolveReport {
      object_id: post_report_view.post.ap_id.inner().clone(),
      actor: local_user_view.person,
      report_creator: report.creator,
      receiver: Either::Right(post_report_view.community.clone()),
    },
    &context,
  )?;

  Ok(Json(PostReportResponse { post_report_view }))
}
