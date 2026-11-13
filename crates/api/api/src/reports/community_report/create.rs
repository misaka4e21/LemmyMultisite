use crate::check_report_reason;
use activitypub_federation::config::Data;
use actix_web::web::Json;
use diesel_async::AsyncConnection;
use either::Either;
use lemmy_api_utils::{
  context::LemmyContext,
  plugins::plugin_hook_after,
  send_activity::{ActivityChannel, SendActivityData},
  tenant::TenantContext,
  utils::{check_community_user_action, check_local_user_banned_or_deleted, slur_regex},
};
use lemmy_db_schema::{
  impls::presence::{SiteReportId, combined_report_id, ensure_report_site},
  source::{
    community::Community,
    community_report::{CommunityReport, CommunityReportForm},
    site::Site,
  },
  traits::Reportable,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_report_combined::{
  ReportCombinedViewInternal,
  api::{CommunityReportResponse, CreateCommunityReport},
};
use lemmy_diesel_utils::{connection::get_conn, traits::Crud};
use lemmy_email::admin::send_new_report_email_to_admins;
use lemmy_utils::error::{LemmyError, LemmyResult};

pub async fn create_community_report(
  Json(data): Json<CreateCommunityReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  tenant: TenantContext,
) -> LemmyResult<Json<CommunityReportResponse>> {
  check_local_user_banned_or_deleted(&local_user_view)?;
  let reason = data.reason.trim().to_string();
  let slur_regex = slur_regex(&context).await?;
  check_report_reason(&reason, &slur_regex)?;

  let person = &local_user_view.person;
  let community_id = data.community_id;
  let community = Community::read(&mut context.pool(), community_id).await?;
  check_community_user_action(&local_user_view, &community, &mut context.pool()).await?;
  let site = Site::read_from_instance_id(&mut context.pool(), community.instance_id).await?;

  let report_form = CommunityReportForm {
    creator_id: person.id,
    community_id,
    original_community_banner: community.banner,
    original_community_summary: community.summary,
    original_community_icon: community.icon,
    original_community_name: community.name,
    original_community_sidebar: community.sidebar,
    original_community_title: community.title,
    reason,
  };

  let pool = &mut context.pool();
  let conn = &mut get_conn(pool).await?;
  let site_id = tenant.local_site().id;
  let (report, combined_id) = conn
    .transaction::<_, LemmyError, _>(async |conn| {
      let pool = &mut conn.into();
      let report = CommunityReport::report(pool, &report_form).await?;
      let combined_id = combined_report_id(pool, SiteReportId::Community(report.id)).await?;
      ensure_report_site(pool, site_id, combined_id).await?;
      Ok((report, combined_id))
    })
    .await?;

  let community_report_view =
    ReportCombinedViewInternal::read_community_report(&mut context.pool(), report.id, person)
      .await?;
  plugin_hook_after("community_report_after_create", &community_report_view);

  // Email the admins
  let local_site = tenant.local_site().clone();
  if local_site.reports_email_admins {
    send_new_report_email_to_admins(
      &community_report_view.creator.name,
      // The argument here is normally the reported content's creator, but a community doesn't have
      // a single person to be considered the creator or the person responsible for the bad thing,
      // so the community name is used instead
      &community_report_view.community.name,
      &mut context.pool(),
      &local_site,
      context.settings(),
    )
    .await?;
  }

  ActivityChannel::submit_activity(
    SendActivityData::CreateReport {
      object_id: community.ap_id.inner().clone(),
      report_id: combined_id,
      actor: local_user_view.person,
      receiver: Either::Left(site),
      reason: data.reason.clone(),
    },
    &context,
  )?;

  Ok(Json(CommunityReportResponse {
    community_report_view,
  }))
}
