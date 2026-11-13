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
  api::{CommentReportResponse, ResolveCommentReport},
};
use lemmy_diesel_utils::utils::diesel_string_update;
use lemmy_utils::error::LemmyResult;

/// Resolves or unresolves a comment report and notifies the moderators of the community
pub async fn resolve_comment_report(
  Json(data): Json<ResolveCommentReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<CommentReportResponse>> {
  let report_id = data.report_id;
  let person = &local_user_view.person;
  let site_id = local_user_view.local_user.local_site_id;
  let combined_id =
    combined_report_id(&mut context.pool(), SiteReportId::Comment(report_id)).await?;
  read_report_site_state(&mut context.pool(), site_id, combined_id).await?;
  let report =
    ReportCombinedViewInternal::read_comment_report(&mut context.pool(), report_id, person).await?;

  let person_id = local_user_view.person.id;
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
    person_id,
    data.resolved,
    diesel_string_update(data.conclusion.as_deref()),
  )
  .await?;

  let report_id = data.report_id;
  let mut comment_report_view =
    ReportCombinedViewInternal::read_comment_report(&mut context.pool(), report_id, person).await?;
  let state = read_report_site_state(&mut context.pool(), site_id, combined_id).await?;
  comment_report_view.comment_report.resolved = state.resolved;
  comment_report_view.comment_report.resolver_id = state.resolver_id;
  comment_report_view.comment_report.conclusion = state.conclusion;
  comment_report_view.comment_report.updated_at = state.updated_at;
  comment_report_view.resolver = Some(person.clone());

  ActivityChannel::submit_activity(
    SendActivityData::SendResolveReport {
      object_id: comment_report_view.comment.ap_id.inner().clone(),
      actor: local_user_view.person,
      report_creator: report.creator,
      receiver: Either::Right(comment_report_view.community.clone()),
    },
    &context,
  )?;

  Ok(Json(CommentReportResponse {
    comment_report_view,
  }))
}
