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
  utils::{check_local_user_banned_or_deleted, slur_regex},
};
use lemmy_db_schema::{
  impls::presence::{SiteReportId, combined_report_id, ensure_report_site},
  source::{
    private_message::PrivateMessage,
    private_message_report::{PrivateMessageReport, PrivateMessageReportForm},
    site::Site,
  },
  traits::Reportable,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_report_combined::{
  ReportCombinedViewInternal,
  api::{CreatePrivateMessageReport, PrivateMessageReportResponse},
};
use lemmy_diesel_utils::{connection::get_conn, traits::Crud};
use lemmy_email::admin::send_new_report_email_to_admins;
use lemmy_utils::error::{LemmyError, LemmyErrorType, LemmyResult};

pub async fn create_pm_report(
  Json(data): Json<CreatePrivateMessageReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  tenant: TenantContext,
) -> LemmyResult<Json<PrivateMessageReportResponse>> {
  check_local_user_banned_or_deleted(&local_user_view)?;
  let reason = data.reason.trim().to_string();
  let slur_regex = slur_regex(&context).await?;
  check_report_reason(&reason, &slur_regex)?;

  let person = &local_user_view.person;
  let private_message_id = data.private_message_id;
  let private_message = PrivateMessage::read(&mut context.pool(), private_message_id).await?;

  // Make sure that only the recipient of the private message can create a report
  if person.id != private_message.recipient_id {
    return Err(LemmyErrorType::CouldntCreate.into());
  }

  let report_form = PrivateMessageReportForm {
    creator_id: person.id,
    private_message_id,
    original_pm_text: private_message.content,
    reason,
  };

  let pool = &mut context.pool();
  let conn = &mut get_conn(pool).await?;
  let site_id = tenant.local_site().id;
  let (report, combined_id) = conn
    .transaction::<_, LemmyError, _>(async |conn| {
      let pool = &mut conn.into();
      let report = PrivateMessageReport::report(pool, &report_form).await?;
      let combined_id = combined_report_id(pool, SiteReportId::PrivateMessage(report.id)).await?;
      ensure_report_site(pool, site_id, combined_id).await?;
      Ok((report, combined_id))
    })
    .await?;

  let private_message_report_view =
    ReportCombinedViewInternal::read_private_message_report(&mut context.pool(), report.id, person)
      .await?;
  plugin_hook_after(
    "private_message_report_after_create",
    &private_message_report_view,
  );

  // Email the admins
  let local_site = tenant.local_site().clone();
  if local_site.reports_email_admins {
    send_new_report_email_to_admins(
      &private_message_report_view.creator.name,
      &private_message_report_view.private_message_creator.name,
      &mut context.pool(),
      &local_site,
      context.settings(),
    )
    .await?;
  }

  let site = Site::read_from_instance_id(
    &mut context.pool(),
    private_message_report_view
      .private_message_creator
      .instance_id,
  )
  .await?;
  ActivityChannel::submit_activity(
    SendActivityData::CreateReport {
      object_id: private_message_report_view
        .private_message
        .ap_id
        .inner()
        .clone(),
      report_id: combined_id,
      actor: private_message_report_view.creator.clone(),
      receiver: Either::Left(site),
      reason: data.reason.clone(),
    },
    &context,
  )?;

  Ok(Json(PrivateMessageReportResponse {
    private_message_report_view,
  }))
}
