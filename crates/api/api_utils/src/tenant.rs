use actix_web::{
  Error,
  FromRequest,
  HttpMessage,
  HttpRequest,
  dev::Payload,
  error::ErrorInternalServerError,
};
use core::future::{Ready, ready};
use lemmy_db_views_site::SiteView;
use lemmy_diesel_utils::connection::DbPool;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use moka::future::Cache;
use std::sync::{Arc, LazyLock};

const TENANT_CACHE_CAPACITY: u64 = 1_000;

#[derive(Clone)]
pub struct TenantContext {
  pub site_view: SiteView,
  pub hostname: String,
  pub scheme: String,
}

impl TenantContext {
  pub fn from_request_extensions(request: &HttpRequest) -> Option<Arc<TenantContext>> {
    request.extensions().get::<Arc<TenantContext>>().cloned()
  }

  pub fn local_site(&self) -> &lemmy_db_schema::source::local_site::LocalSite {
    &self.site_view.local_site
  }

  pub fn site(&self) -> &lemmy_db_schema::source::site::Site {
    &self.site_view.site
  }

  pub async fn read_by_hostname(pool: &mut DbPool<'_>, host: &str) -> LemmyResult<Arc<Self>> {
    let hostname = canonicalize_host(host)?;
    if let Some(tenant) = tenant_cache().get(&hostname).await {
      return Ok(tenant);
    }

    let site_view = SiteView::read_by_hostname(pool, &hostname).await?;
    let scheme = site_view.site.ap_id.scheme().to_string();
    let tenant = Arc::new(TenantContext {
      site_view,
      hostname: hostname.clone(),
      scheme,
    });
    tenant_cache().insert(hostname, tenant.clone()).await;
    Ok(tenant)
  }

  pub async fn invalidate(hostname: &str) {
    if let Ok(hostname) = canonicalize_host(hostname) {
      tenant_cache().invalidate(&hostname).await;
    }
  }

  pub fn invalidate_all() {
    tenant_cache().invalidate_all();
  }
}

impl FromRequest for TenantContext {
  type Error = Error;
  type Future = Ready<Result<Self, Self::Error>>;

  fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
    match TenantContext::from_request_extensions(request) {
      Some(tenant) => ready(Ok((*tenant).clone())),
      None => ready(Err(ErrorInternalServerError("tenant context missing"))),
    }
  }
}

fn tenant_cache() -> &'static Cache<String, Arc<TenantContext>> {
  static CACHE: LazyLock<Cache<String, Arc<TenantContext>>> =
    LazyLock::new(|| Cache::builder().max_capacity(TENANT_CACHE_CAPACITY).build());
  &CACHE
}

pub fn canonicalize_host(host: &str) -> LemmyResult<String> {
  let host = host.trim().to_lowercase();
  let host = host.split(':').next().unwrap_or_default();
  let host = host.trim_end_matches('.');
  if host.is_empty() || host.contains('/') {
    return Err(LemmyErrorType::NotFound.into());
  }
  Ok(host.to_string())
}

#[cfg(test)]
mod tests {
  use super::canonicalize_host;

  #[test]
  fn test_canonicalize_host() {
    assert_eq!(canonicalize_host("A.Example.:8536").unwrap(), "a.example");
    assert_eq!(canonicalize_host("b.example").unwrap(), "b.example");
    assert!(canonicalize_host(" ").is_err());
  }
}
