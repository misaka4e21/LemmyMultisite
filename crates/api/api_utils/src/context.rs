use crate::{request::client_builder, tenant::TenantContext};
use activitypub_federation::config::{Data, FederationConfig};
use lemmy_db_schema::source::secret::Secret;
use lemmy_db_views_site::SiteView;
use lemmy_diesel_utils::connection::{ActualDbPool, DbPool, build_db_pool_for_tests};
use lemmy_utils::{
  error::LemmyResult,
  rate_limit::RateLimit,
  settings::{SETTINGS, structs::Settings},
};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::sync::Arc;

#[derive(Clone)]
pub struct LemmyContext {
  pool: ActualDbPool,
  client: Arc<ClientWithMiddleware>,
  /// Pictrs requests must bypass proxy. Unfortunately no_proxy can only be set on ClientBuilder
  /// and not on RequestBuilder, so we need a separate client here.
  pictrs_client: Arc<ClientWithMiddleware>,
  secret: Arc<Secret>,
  rate_limit_cell: RateLimit,
  tenant: Option<Arc<TenantContext>>,
}

impl LemmyContext {
  pub fn create(
    pool: ActualDbPool,
    client: ClientWithMiddleware,
    pictrs_client: ClientWithMiddleware,
    secret: Secret,
    rate_limit_cell: RateLimit,
  ) -> LemmyContext {
    LemmyContext {
      pool,
      client: Arc::new(client),
      pictrs_client: Arc::new(pictrs_client),
      secret: Arc::new(secret),
      rate_limit_cell,
      tenant: None,
    }
  }
  pub fn pool(&self) -> DbPool<'_> {
    DbPool::Pool(&self.pool)
  }
  pub fn inner_pool(&self) -> &ActualDbPool {
    &self.pool
  }
  pub fn client(&self) -> &ClientWithMiddleware {
    &self.client
  }
  pub fn pictrs_client(&self) -> &ClientWithMiddleware {
    &self.pictrs_client
  }
  pub fn settings(&self) -> &'static Settings {
    &SETTINGS
  }
  pub fn secret(&self) -> &Secret {
    &self.secret
  }
  pub fn rate_limit_cell(&self) -> &RateLimit {
    &self.rate_limit_cell
  }

  pub fn with_tenant(&self, tenant: Arc<TenantContext>) -> Self {
    let mut context = self.clone();
    context.tenant = Some(tenant);
    context
  }

  pub fn tenant(&self) -> Option<&Arc<TenantContext>> {
    self.tenant.as_ref()
  }

  pub fn site_view(&self) -> LemmyResult<&SiteView> {
    self
      .tenant
      .as_ref()
      .map(|tenant| &tenant.site_view)
      .ok_or_else(|| lemmy_utils::error::LemmyErrorType::LocalSiteNotSetup.into())
  }

  pub fn hostname(&self) -> &str {
    self
      .tenant
      .as_ref()
      .map(|tenant| tenant.hostname.as_str())
      .or_else(|| SETTINGS.bootstrap_hostname())
      .unwrap_or("unset")
  }

  pub fn protocol_and_hostname(&self) -> String {
    match self.tenant.as_ref() {
      Some(tenant) => format!("{}://{}", tenant.scheme, tenant.hostname),
      None => SETTINGS.get_protocol_and_hostname(),
    }
  }

  /// Initialize a context for use in tests which blocks federation network calls.
  ///
  /// Do not use this in production code.
  #[expect(clippy::expect_used)]
  pub async fn init_test_federation_config() -> FederationConfig<LemmyContext> {
    // call this to run migrations
    let pool = build_db_pool_for_tests();

    let client = client_builder(&SETTINGS).build().expect("build client");

    let client = ClientBuilder::new(client).build();
    let secret = Secret {
      id: 0,
      jwt_secret: String::new().into(),
    };

    let rate_limit_cell = RateLimit::with_debug_config();

    let context = LemmyContext::create(
      pool,
      client.clone(),
      client,
      secret,
      rate_limit_cell.clone(),
    );

    FederationConfig::builder()
      .domain(context.hostname().to_string())
      .app_data(context)
      .debug(true)
      // Dont allow any network fetches
      .http_fetch_limit(0)
      .build()
      .await
      .expect("build federation config")
  }
  pub async fn init_test_context() -> Data<LemmyContext> {
    let config = Self::init_test_federation_config().await;
    config.to_request_data()
  }
}
