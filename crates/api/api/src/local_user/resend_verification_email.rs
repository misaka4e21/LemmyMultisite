use actix_web::web::{Data, Json};
use lemmy_api_utils::{
  context::LemmyContext,
  tenant::TenantContext,
  utils::check_local_user_banned_or_deleted,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::{ResendVerificationEmail, SuccessResponse};
use lemmy_email::account::send_verification_email_if_required;
use lemmy_utils::error::LemmyResult;

pub async fn resend_verification_email(
  Json(data): Json<ResendVerificationEmail>,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<SuccessResponse>> {
  let email = data.email.to_string();

  // For security, errors are not returned.
  // https://github.com/LemmyNet/lemmy/issues/5277
  let _ = try_resend_verification_email(&email, &context, &tenant).await;

  Ok(Json(SuccessResponse::default()))
}

async fn try_resend_verification_email(
  email: &str,
  context: &LemmyContext,
  tenant: &TenantContext,
) -> LemmyResult<()> {
  // Fetch that email
  let local_user_view =
    LocalUserView::find_by_email(&mut context.pool(), tenant.local_site().id, email).await?;
  check_local_user_banned_or_deleted(&local_user_view)?;

  let site_view = tenant.site_view.clone();

  send_verification_email_if_required(
    &site_view.local_site,
    &local_user_view,
    &mut context.pool(),
    context.settings(),
  )
  .await?;

  Ok(())
}
