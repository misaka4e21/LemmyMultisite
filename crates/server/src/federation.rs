use activitypub_federation::config::FederationConfig;
use actix_web::{
  Error,
  HttpMessage,
  body::MessageBody,
  dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
  error::ErrorInternalServerError,
};
use core::future::Ready;
use futures_util::future::LocalBoxFuture;
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext};
use lemmy_apub::{FEDERATION_HTTP_FETCH_LIMIT, VerifyUrlData};
use lemmy_apub_objects::objects::instance::ApubSite;
use lemmy_db_schema_file::newtypes::LocalSiteId;
use lemmy_db_views_site::SiteView;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};

pub type FederationConfigs = Arc<HashMap<LocalSiteId, FederationConfig<LemmyContext>>>;

pub async fn build_federation_config(
  base_context: &LemmyContext,
  site_view: &SiteView,
  request_timeout: Option<Duration>,
) -> LemmyResult<FederationConfig<LemmyContext>> {
  let tenant = Arc::new(TenantContext {
    site_view: site_view.clone(),
    hostname: site_view.local_site.hostname.clone(),
    scheme: site_view.site.ap_id.scheme().to_string(),
  });
  let context = base_context.with_tenant(tenant.clone());

  let mut builder = FederationConfig::builder();
  builder
    .domain(tenant.hostname.clone())
    .app_data(context)
    .client(base_context.client().clone())
    .http_fetch_limit(FEDERATION_HTTP_FETCH_LIMIT)
    .debug(cfg!(debug_assertions))
    .http_signature_compat(true)
    .url_verifier(Box::new(VerifyUrlData(
      base_context.inner_pool().clone(),
      tenant.local_site().id,
    )));

  if let Some(timeout) = request_timeout {
    builder.request_timeout(timeout);
  }

  if tenant.site_view.local_site.federation_signed_fetch {
    let site: ApubSite = tenant.site_view.site.clone().into();
    builder.signed_fetch_actor(&site);
  }

  builder.build().await.map_err(Into::into)
}

pub async fn build_federation_configs(
  base_context: &LemmyContext,
  request_timeout: Option<Duration>,
) -> LemmyResult<FederationConfigs> {
  let sites = SiteView::read_all(&mut base_context.pool()).await?;
  if sites.is_empty() {
    return Err(LemmyErrorType::LocalSiteNotSetup.into());
  }

  let mut configs = HashMap::new();
  for site_view in &sites {
    let config = build_federation_config(base_context, site_view, request_timeout).await?;
    configs.insert(site_view.local_site.id, config);
  }
  Ok(Arc::new(configs))
}

#[derive(Clone)]
pub struct TenantFederationMiddleware {
  configs: FederationConfigs,
  base_context: LemmyContext,
}

impl TenantFederationMiddleware {
  pub fn new(configs: FederationConfigs, base_context: LemmyContext) -> Self {
    TenantFederationMiddleware {
      configs,
      base_context,
    }
  }
}

impl<S, B> Transform<S, ServiceRequest> for TenantFederationMiddleware
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  S::Future: 'static,
  B: MessageBody + 'static,
{
  type Response = ServiceResponse<B>;
  type Error = Error;
  type Transform = TenantFederationService<S>;
  type InitError = ();
  type Future = Ready<Result<Self::Transform, Self::InitError>>;

  fn new_transform(&self, service: S) -> Self::Future {
    std::future::ready(Ok(TenantFederationService {
      service: Rc::new(service),
      configs: self.configs.clone(),
      base_context: self.base_context.clone(),
    }))
  }
}

pub struct TenantFederationService<S> {
  service: Rc<S>,
  configs: FederationConfigs,
  base_context: LemmyContext,
}

impl<S, B> Service<ServiceRequest> for TenantFederationService<S>
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  S::Future: 'static,
  B: 'static,
{
  type Response = ServiceResponse<B>;
  type Error = Error;
  type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

  forward_ready!(service);

  fn call(&self, req: ServiceRequest) -> Self::Future {
    let svc = self.service.clone();
    let configs = self.configs.clone();
    let base_context = self.base_context.clone();

    Box::pin(async move {
      let tenant = TenantContext::from_request_extensions(req.request())
        .ok_or_else(|| ErrorInternalServerError("tenant context missing"))?;
      let config = match configs.get(&tenant.local_site().id).cloned() {
        Some(config) => config,
        None => build_federation_config(&base_context, &tenant.site_view, None)
          .await
          .map_err(ErrorInternalServerError)?,
      };
      req.extensions_mut().insert(config);
      svc.call(req).await
    })
  }
}
