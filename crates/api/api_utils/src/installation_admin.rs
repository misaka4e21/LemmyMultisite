use crate::context::LemmyContext;
use actix_web::{
  FromRequest,
  HttpRequest,
  dev::Payload,
  error::ErrorUnauthorized,
  http::header::Header,
};
use actix_web_httpauth::headers::authorization::{Authorization, Bearer};
use chrono::{Duration, Utc};
use core::future::{Ready, ready};
use jsonwebtoken::{DecodingKey, EncodingKey, Header as JwtHeader, Validation, decode, encode};
use lemmy_diesel_utils::sensitive::SensitiveString;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use serde::{Deserialize, Serialize};

const INSTALLATION_ADMIN_SCOPE: &str = "installation_admin";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallationAdminClaims {
  pub sub: String,
  pub iss: String,
  pub iat: i64,
  pub exp: i64,
  pub scope: String,
}

impl InstallationAdminClaims {
  pub fn generate(id: i64, context: &LemmyContext) -> LemmyResult<SensitiveString> {
    let now = Utc::now();
    let claims = Self {
      sub: id.to_string(),
      iss: context.hostname().to_string(),
      iat: now.timestamp(),
      exp: (now + Duration::days(1)).timestamp(),
      scope: INSTALLATION_ADMIN_SCOPE.to_string(),
    };
    let key = EncodingKey::from_secret(context.secret().jwt_secret.as_ref());
    Ok(encode(&JwtHeader::default(), &claims, &key)?.into())
  }

  pub fn validate(jwt: &str, context: &LemmyContext) -> LemmyResult<i64> {
    let key = DecodingKey::from_secret(context.secret().jwt_secret.as_ref());
    let claims = decode::<Self>(jwt, &key, &Validation::default())
      .map_err(|_| LemmyErrorType::NotLoggedIn)?
      .claims;
    if claims.scope != INSTALLATION_ADMIN_SCOPE {
      return Err(LemmyErrorType::NotLoggedIn.into());
    }
    Ok(claims.sub.parse()?)
  }
}

#[derive(Debug, Clone, Copy)]
pub struct InstallationAdminAuth {
  pub id: i64,
}

impl InstallationAdminAuth {
  pub fn from_request(req: &HttpRequest, context: &LemmyContext) -> LemmyResult<Option<Self>> {
    let Ok(header) = Authorization::<Bearer>::parse(req) else {
      return Ok(None);
    };
    let id = InstallationAdminClaims::validate(header.as_ref().token(), context)?;
    Ok(Some(Self { id }))
  }
}

impl FromRequest for InstallationAdminAuth {
  type Error = actix_web::Error;
  type Future = Ready<Result<Self, Self::Error>>;

  fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
    let Some(context) = req.app_data::<actix_web::web::Data<LemmyContext>>() else {
      return ready(Err(ErrorUnauthorized("context missing")));
    };
    let result = Self::from_request(req, context.as_ref())
      .map_err(|_| ErrorUnauthorized("installation admin login required"))
      .and_then(|value| {
        value.ok_or_else(|| ErrorUnauthorized("installation admin login required"))
      });
    ready(result)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn installation_admin_claims_round_trip() -> LemmyResult<()> {
    let context = LemmyContext::init_test_context().await;
    let token = InstallationAdminClaims::generate(42, &context)?;
    assert_eq!(InstallationAdminClaims::validate(&token, &context)?, 42);
    assert!(
      crate::claims::Claims::validate(&token, &context, false, None)
        .await
        .is_err()
    );
    Ok(())
  }
}
