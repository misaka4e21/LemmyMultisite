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
  utils::{
    check_comment_deleted_or_removed,
    check_community_user_action,
    check_local_user_banned_or_deleted,
    slur_regex,
  },
};
use lemmy_db_schema::{
  impls::presence::{SiteReportId, combined_report_id, ensure_report_site},
  source::comment_report::{CommentReport, CommentReportForm},
  traits::Reportable,
};
use lemmy_db_views_comment::CommentView;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_report_combined::{
  ReportCombinedViewInternal,
  api::{CommentReportResponse, CreateCommentReport},
};
use lemmy_diesel_utils::connection::get_conn;
use lemmy_email::admin::send_new_report_email_to_admins;
use lemmy_utils::error::{LemmyError, LemmyResult};

/// Creates a comment report and notifies the moderators of the community
pub async fn create_comment_report(
  Json(data): Json<CreateCommentReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  tenant: TenantContext,
) -> LemmyResult<Json<CommentReportResponse>> {
  check_local_user_banned_or_deleted(&local_user_view)?;
  let reason = data.reason.trim().to_string();
  let slur_regex = slur_regex(&context).await?;
  check_report_reason(&reason, &slur_regex)?;

  let person = &local_user_view.person;
  let local_instance_id = local_user_view.person.instance_id;
  let comment_id = data.comment_id;
  let comment_view = CommentView::read(
    &mut context.pool(),
    comment_id,
    Some(&local_user_view.local_user),
    local_instance_id,
  )
  .await?;

  check_community_user_action(
    &local_user_view,
    &comment_view.community,
    &mut context.pool(),
  )
  .await?;

  // Don't allow creating reports for removed / deleted comments
  check_comment_deleted_or_removed(&comment_view.comment)?;

  let report_form = CommentReportForm {
    creator_id: person.id,
    comment_id,
    original_comment_text: comment_view.comment.content,
    reason,
    violates_instance_rules: data.violates_instance_rules.unwrap_or_default(),
  };

  let pool = &mut context.pool();
  let conn = &mut get_conn(pool).await?;
  let site_id = tenant.local_site().id;
  let (report, combined_id) = conn
    .transaction::<_, LemmyError, _>(async |conn| {
      let pool = &mut conn.into();
      let report = CommentReport::report(pool, &report_form).await?;
      let combined_id = combined_report_id(pool, SiteReportId::Comment(report.id)).await?;
      ensure_report_site(pool, site_id, combined_id).await?;
      Ok((report, combined_id))
    })
    .await?;

  let comment_report_view =
    ReportCombinedViewInternal::read_comment_report(&mut context.pool(), report.id, person).await?;
  plugin_hook_after("comment_report_after_create", &comment_report_view);

  // Email the admins
  let local_site = tenant.local_site().clone();
  if local_site.reports_email_admins {
    send_new_report_email_to_admins(
      &comment_report_view.creator.name,
      &comment_report_view.comment_creator.name,
      &mut context.pool(),
      &local_site,
      context.settings(),
    )
    .await?;
  }

  if !report.violates_instance_rules {
    ActivityChannel::submit_activity(
      SendActivityData::CreateReport {
        object_id: comment_view.comment.ap_id.inner().clone(),
        report_id: combined_id,
        actor: local_user_view.person,
        receiver: Either::Right(comment_view.community),
        reason: data.reason.clone(),
      },
      &context,
    )?;
  }

  Ok(Json(CommentReportResponse {
    comment_report_view,
  }))
}
