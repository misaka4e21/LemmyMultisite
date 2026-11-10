use actix_web::web::{Data, Json};
use lemmy_api_utils::{
  context::LemmyContext,
  tenant::TenantContext,
  utils::{check_email_verified, check_local_user_banned_or_deleted},
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::{ResetPassword, SuccessResponse};
use lemmy_email::account::send_password_reset_email;
use lemmy_utils::error::LemmyResult;
use tracing::error;

pub async fn reset_password(
  Json(data): Json<ResetPassword>,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<SuccessResponse>> {
  let email = data.email.to_lowercase();
  // For security, errors are not returned.
  // https://github.com/LemmyNet/lemmy/issues/5277
  let _ = try_reset_password(&email, &context, &tenant).await;
  Ok(Json(SuccessResponse::default()))
}

async fn try_reset_password(
  email: &str,
  context: &LemmyContext,
  tenant: &TenantContext,
) -> LemmyResult<()> {
  let local_user_view =
    LocalUserView::find_by_email(&mut context.pool(), tenant.local_site().id, email).await?;
  check_local_user_banned_or_deleted(&local_user_view)?;
  let site_view = tenant.site_view.clone();

  check_email_verified(&local_user_view, &site_view)?;
  if let Err(e) = send_password_reset_email(
    &local_user_view,
    &mut context.pool(),
    tenant.local_site(),
    context.settings(),
  )
  .await
  {
    error!("Failed to send password reset email: {}", e);
  }

  Ok(())
}
