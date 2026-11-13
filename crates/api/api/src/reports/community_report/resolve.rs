use activitypub_federation::config::Data;
use actix_web::web::Json;
use either::Either;
use lemmy_api_utils::{
  context::LemmyContext,
  send_activity::{ActivityChannel, SendActivityData},
  utils::is_admin,
};
use lemmy_db_schema::{
  impls::presence::{
    SiteReportId,
    combined_report_id,
    read_report_site_state,
    resolve_report_for_site,
  },
  source::site::Site,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_report_combined::{
  ReportCombinedViewInternal,
  api::{CommunityReportResponse, ResolveCommunityReport},
};
use lemmy_diesel_utils::utils::diesel_string_update;
use lemmy_utils::error::LemmyResult;

pub async fn resolve_community_report(
  Json(data): Json<ResolveCommunityReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<CommunityReportResponse>> {
  is_admin(&local_user_view)?;

  let report_id = data.report_id;
  let person = &local_user_view.person;

  let site_id = local_user_view.local_user.local_site_id;
  let combined_id =
    combined_report_id(&mut context.pool(), SiteReportId::Community(report_id)).await?;
  resolve_report_for_site(
    &mut context.pool(),
    site_id,
    combined_id,
    person.id,
    data.resolved,
    diesel_string_update(data.conclusion.as_deref()),
  )
  .await?;

  let mut community_report_view =
    ReportCombinedViewInternal::read_community_report(&mut context.pool(), report_id, person)
      .await?;
  let state = read_report_site_state(&mut context.pool(), site_id, combined_id).await?;
  community_report_view.community_report.resolved = state.resolved;
  community_report_view.community_report.resolver_id = state.resolver_id;
  community_report_view.community_report.conclusion = state.conclusion;
  community_report_view.community_report.updated_at = state.updated_at;
  community_report_view.resolver = Some(person.clone());
  let site = Site::read_from_instance_id(
    &mut context.pool(),
    community_report_view.community.instance_id,
  )
  .await?;

  ActivityChannel::submit_activity(
    SendActivityData::SendResolveReport {
      object_id: community_report_view.community.ap_id.inner().clone(),
      actor: local_user_view.person,
      report_creator: community_report_view.creator.clone(),
      receiver: Either::Left(site),
    },
    &context,
  )?;

  Ok(Json(CommunityReportResponse {
    community_report_view,
  }))
}
