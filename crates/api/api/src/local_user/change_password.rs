use actix_web::{
  HttpRequest,
  web::{Data, Json},
};
use lemmy_api_utils::{
  claims::Claims,
  context::LemmyContext,
  utils::{check_local_user_banned_or_deleted, password_length_check},
};
use lemmy_db_schema::{
  impls::password::verify_password,
  source::{local_user::LocalUser, login_token::LoginToken},
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::{ChangePassword, LoginResponse};
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use tokio::task::spawn_blocking;

pub async fn change_password(
  Json(data): Json<ChangePassword>,
  req: HttpRequest,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<LoginResponse>> {
  check_local_user_banned_or_deleted(&local_user_view)?;
  password_length_check(&data.new_password)?;

  // Make sure passwords match
  if data.new_password != data.new_password_verify {
    return Err(LemmyErrorType::PasswordsDoNotMatch.into());
  }

  // Check the old password
  let valid: bool = if let Some(password_encrypted) = &local_user_view.local_user.password_encrypted
  {
    let old_password = data.old_password.clone();
    let password_encrypted = password_encrypted.clone().into_inner();
    spawn_blocking(move || verify_password(&old_password, &password_encrypted)).await?
  } else {
    data.old_password.is_empty()
  };

  if !valid {
    return Err(LemmyErrorType::IncorrectLogin.into());
  }

  let local_user_id = local_user_view.local_user.id;
  let new_password = data.new_password.clone();
  let updated_local_user =
    LocalUser::update_password(&mut context.pool(), local_user_id, &new_password).await?;

  LoginToken::invalidate_all(&mut context.pool(), local_user_view.local_user.id).await?;

  // Return the jwt
  Ok(Json(LoginResponse {
    jwt: Some(Claims::generate(updated_local_user.id, data.stay_logged_in, req, &context).await?),
    verify_email_sent: false,
    registration_created: false,
  }))
}
