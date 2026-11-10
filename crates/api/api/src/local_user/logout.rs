use activitypub_federation::config::Data;
use actix_web::{HttpRequest, HttpResponse};
use lemmy_api_utils::{
  context::LemmyContext,
  utils::{read_auth_token, remove_auth_cookie},
};
use lemmy_db_schema::source::login_token::LoginToken;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::SuccessResponse;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

pub async fn logout(
  req: HttpRequest,
  // require login
  _local_user_view: LocalUserView,
  context: Data<LemmyContext>,
) -> LemmyResult<HttpResponse> {
  let jwt = read_auth_token(&req)?.ok_or(LemmyErrorType::NotLoggedIn)?;
  LoginToken::invalidate(&mut context.pool(), &jwt).await?;

  let mut res = HttpResponse::Ok().json(SuccessResponse::default());
  let secure = context
    .tenant()
    .is_some_and(|tenant| tenant.scheme == "https");
  res.add_removal_cookie(&remove_auth_cookie(secure))?;
  Ok(res)
}
