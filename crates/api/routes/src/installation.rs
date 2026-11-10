use actix_web::web::{Data, Json, Path};
use lemmy_api_utils::{
  context::LemmyContext,
  installation_admin::{InstallationAdminAuth, InstallationAdminClaims},
};
use lemmy_db_schema::source::installation_admin::InstallationAdmin;
use lemmy_db_schema_file::newtypes::LocalSiteId;
use lemmy_db_views_site::{SiteView, api::SuccessResponse};
use lemmy_routes::utils::{
  provision_site::{ProvisionSite, provision_site},
  site_lifecycle::{change_hostname, disable_site, remove_site},
};
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct InstallationAdminLogin {
  pub username: String,
  pub password: String,
}

#[derive(Serialize)]
pub struct InstallationAdminLoginResponse {
  pub jwt: String,
}

pub async fn installation_admin_login(
  Json(data): Json<InstallationAdminLogin>,
  context: Data<LemmyContext>,
) -> LemmyResult<Json<InstallationAdminLoginResponse>> {
  let admin = InstallationAdmin::read_by_username(&mut context.pool(), &data.username).await?;
  let Some(admin) = admin else {
    return Err(LemmyErrorType::IncorrectLogin.into());
  };
  let admin_id = admin.id;
  let password = data.password.clone();
  let valid = tokio::task::spawn_blocking(move || admin.verify_password(&password)).await?;
  if !valid {
    return Err(LemmyErrorType::IncorrectLogin.into());
  }
  let jwt = InstallationAdminClaims::generate(admin_id, &context)?;
  Ok(Json(InstallationAdminLoginResponse {
    jwt: jwt.into_inner(),
  }))
}

pub async fn installation_list_sites(
  context: Data<LemmyContext>,
  _admin: InstallationAdminAuth,
) -> LemmyResult<Json<Vec<SiteView>>> {
  Ok(Json(SiteView::read_all(&mut context.pool()).await?))
}

#[derive(Deserialize)]
pub struct InstallationCreateSite {
  pub hostname: String,
  pub site_name: String,
  pub admin_username: String,
  pub admin_password: String,
  pub admin_email: Option<String>,
  pub smtp_connection: Option<String>,
  pub smtp_from_address: Option<String>,
}

pub async fn installation_create_site(
  Json(data): Json<InstallationCreateSite>,
  context: Data<LemmyContext>,
  _admin: InstallationAdminAuth,
) -> LemmyResult<Json<SiteView>> {
  let scheme = if context.settings().tls_enabled {
    "https"
  } else {
    "http"
  };
  let view = provision_site(
    &mut context.pool(),
    ProvisionSite {
      hostname: data.hostname,
      site_name: data.site_name,
      scheme: scheme.to_string(),
      admin_username: data.admin_username,
      admin_password: data.admin_password,
      admin_email: data.admin_email,
      smtp_connection: data.smtp_connection,
      smtp_from_address: data.smtp_from_address,
    },
  )
  .await?;
  Ok(Json(view))
}

pub async fn installation_disable_site(
  id: Path<i32>,
  context: Data<LemmyContext>,
  _admin: InstallationAdminAuth,
) -> LemmyResult<Json<SuccessResponse>> {
  disable_site(&mut context.pool(), LocalSiteId(id.into_inner())).await?;
  Ok(Json(SuccessResponse::default()))
}

pub async fn installation_remove_site(
  id: Path<i32>,
  context: Data<LemmyContext>,
  _admin: InstallationAdminAuth,
) -> LemmyResult<Json<SuccessResponse>> {
  remove_site(&mut context.pool(), LocalSiteId(id.into_inner())).await?;
  Ok(Json(SuccessResponse::default()))
}

#[derive(Deserialize)]
pub struct InstallationChangeHostname {
  pub hostname: String,
}

pub async fn installation_change_hostname(
  id: Path<i32>,
  Json(data): Json<InstallationChangeHostname>,
  context: Data<LemmyContext>,
  _admin: InstallationAdminAuth,
) -> LemmyResult<Json<SiteView>> {
  let view = change_hostname(
    &mut context.pool(),
    LocalSiteId(id.into_inner()),
    &data.hostname,
  )
  .await?;
  Ok(Json(view))
}
