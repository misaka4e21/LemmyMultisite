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
  source::{person::Person, site::Site},
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_report_combined::{
  ReportCombinedViewInternal,
  api::{PrivateMessageReportResponse, ResolvePrivateMessageReport},
};
use lemmy_diesel_utils::{traits::Crud, utils::diesel_string_update};
use lemmy_utils::error::LemmyResult;

pub async fn resolve_pm_report(
  Json(data): Json<ResolvePrivateMessageReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<PrivateMessageReportResponse>> {
  is_admin(&local_user_view)?;

  let report_id = data.report_id;
  let person = &local_user_view.person;

  let site_id = local_user_view.local_user.local_site_id;
  let combined_id =
    combined_report_id(&mut context.pool(), SiteReportId::PrivateMessage(report_id)).await?;
  resolve_report_for_site(
    &mut context.pool(),
    site_id,
    combined_id,
    person.id,
    data.resolved,
    diesel_string_update(data.conclusion.as_deref()),
  )
  .await?;

  let mut private_message_report_view =
    ReportCombinedViewInternal::read_private_message_report(&mut context.pool(), report_id, person)
      .await?;
  let state = read_report_site_state(&mut context.pool(), site_id, combined_id).await?;
  private_message_report_view.private_message_report.resolved = state.resolved;
  private_message_report_view
    .private_message_report
    .resolver_id = state.resolver_id;
  private_message_report_view
    .private_message_report
    .conclusion = state.conclusion;
  private_message_report_view
    .private_message_report
    .updated_at = state.updated_at;
  private_message_report_view.resolver = Some(person.clone());

  let recipient = Person::read(
    &mut context.pool(),
    private_message_report_view.private_message.recipient_id,
  )
  .await?;
  let site = Site::read_from_instance_id(&mut context.pool(), recipient.instance_id).await?;
  ActivityChannel::submit_activity(
    SendActivityData::SendResolveReport {
      object_id: private_message_report_view
        .private_message
        .ap_id
        .inner()
        .clone(),
      actor: local_user_view.person,
      report_creator: private_message_report_view.creator.clone(),
      receiver: Either::Left(site),
    },
    &context,
  )?;

  Ok(Json(PrivateMessageReportResponse {
    private_message_report_view,
  }))
}
