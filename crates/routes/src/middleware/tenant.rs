use actix_web::{
  Error,
  HttpMessage,
  body::MessageBody,
  dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
  error::ErrorNotFound,
  http::header::HOST,
};
use core::future::Ready;
use futures_util::future::LocalBoxFuture;
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext};
use std::rc::Rc;

#[derive(Clone)]
pub struct TenantMiddleware {
  context: LemmyContext,
}

impl TenantMiddleware {
  pub fn new(context: LemmyContext) -> Self {
    TenantMiddleware { context }
  }
}

impl<S, B> Transform<S, ServiceRequest> for TenantMiddleware
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  S::Future: 'static,
  B: MessageBody + 'static,
{
  type Response = ServiceResponse<B>;
  type Error = Error;
  type Transform = TenantService<S>;
  type InitError = ();
  type Future = Ready<Result<Self::Transform, Self::InitError>>;

  fn new_transform(&self, service: S) -> Self::Future {
    std::future::ready(Ok(TenantService {
      service: Rc::new(service),
      context: self.context.clone(),
    }))
  }
}

pub struct TenantService<S> {
  service: Rc<S>,
  context: LemmyContext,
}

impl<S, B> Service<ServiceRequest> for TenantService<S>
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
    let context = self.context.clone();

    Box::pin(async move {
      let host = req
        .request()
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ErrorNotFound("missing Host header"))?;

      let tenant = TenantContext::read_by_hostname(&mut context.pool(), host)
        .await
        .map_err(|_| ErrorNotFound("unknown site hostname"))?;
      req.extensions_mut().insert(tenant.local_site().id);
      req.extensions_mut().insert(tenant);

      svc.call(req).await
    })
  }
}
