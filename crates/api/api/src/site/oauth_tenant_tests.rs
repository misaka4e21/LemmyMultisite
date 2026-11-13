use activitypub_federation::config::{Data, FederationConfig};
use actix_web::{App, HttpResponse, HttpServer, test::TestRequest, web};
use lemmy_api_crud::user::create::authenticate_with_oauth;
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext};
use lemmy_db_schema::{
  source::{
    instance::Instance,
    local_site::{LocalSite, LocalSiteInsertForm, LocalSiteUpdateForm},
    local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
    local_user::{LocalUser, LocalUserInsertForm},
    oauth_account::{OAuthAccount, OAuthAccountInsertForm},
    oauth_provider::{AdminOAuthProvider, OAuthProviderInsertForm},
    person::{Person, PersonInsertForm},
    site::{Site, SiteUpsertForm},
  },
  test_data::TestData,
};
use lemmy_db_views_site::{SiteView, api::AuthenticateWithOauth};
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use serial_test::serial;
use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};
use url::Url;

pub(super) async fn site_context(
  base: &Data<LemmyContext>,
  site: lemmy_db_schema_file::newtypes::LocalSiteId,
) -> LemmyResult<(Data<LemmyContext>, TenantContext)> {
  let view = SiteView::read_by_id(&mut base.pool(), site).await?;
  let tenant = TenantContext {
    hostname: view.local_site.hostname.clone(),
    scheme: view.site.ap_id.scheme().into(),
    site_view: view,
  };
  let context = base.app_data().with_tenant(Arc::new(tenant.clone()));
  let data = FederationConfig::builder()
    .domain(tenant.hostname.clone())
    .app_data(context)
    .debug(true)
    .http_fetch_limit(0)
    .build()
    .await?
    .to_request_data();
  Ok((data, tenant))
}

#[actix_web::test]
#[serial]
async fn oauth_provider_is_bound_to_request_site() -> LemmyResult<()> {
  let base = LemmyContext::init_test_context().await;
  let pool = &mut base.pool();
  let first = TestData::create(pool).await?;
  LocalSite::update(
    pool,
    first.local_site.id,
    &LocalSiteUpdateForm {
      email_verification_required: Some(false),
      ..Default::default()
    },
  )
  .await?;
  let user = LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: first.local_site.id,
      ..LocalUserInsertForm::test_form_admin(first.person.id)
    },
    vec![],
  )
  .await?;

  let second_instance = Instance::read_or_create(pool, "oauth-second.example").await?;
  let second_site = Site::create(
    pool,
    &SiteUpsertForm::new("second".into(), second_instance.id),
  )
  .await?;
  let system = Person::create(
    pool,
    &PersonInsertForm::test_form(second_instance.id, "system"),
  )
  .await?;
  let second = LocalSite::create(
    pool,
    &LocalSiteInsertForm::new(second_site.id, system.id, "oauth-second.example".into()),
  )
  .await?;
  LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(second.id)).await?;

  let hits = Arc::new(AtomicUsize::new(0));
  let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
  let address = listener.local_addr()?;
  let server_hits = hits.clone();
  let server = HttpServer::new(move || {
    let hits = server_hits.clone();
    App::new()
      .app_data(web::Data::new(hits))
      .route(
        "/token",
        web::post().to(|hits: web::Data<Arc<AtomicUsize>>| async move {
          hits.fetch_add(1, Ordering::SeqCst);
          HttpResponse::Ok()
            .json(serde_json::json!({"access_token":"test-token","token_type":"Bearer"}))
        }),
      )
      .route(
        "/userinfo",
        web::get().to(|hits: web::Data<Arc<AtomicUsize>>| async move {
          hits.fetch_add(1, Ordering::SeqCst);
          HttpResponse::Ok().json(serde_json::json!({"sub":"external-user"}))
        }),
      )
  })
  .workers(1)
  .listen(listener)?
  .run();
  let handle = server.handle();
  actix_web::rt::spawn(server);

  let issuer = Url::parse(&format!("http://{address}"))?;
  let provider = AdminOAuthProvider::create(
    pool,
    &OAuthProviderInsertForm {
      local_site_id: first.local_site.id,
      display_name: "test".into(),
      issuer: issuer.clone().into(),
      authorization_endpoint: issuer.join("/authorize")?.into(),
      token_endpoint: issuer.join("/token")?.into(),
      userinfo_endpoint: issuer.join("/userinfo")?.into(),
      id_claim: "sub".into(),
      client_id: "client".into(),
      client_secret: "secret".into(),
      scopes: "openid".into(),
      auto_verify_email: None,
      account_linking_enabled: None,
      use_pkce: Some(false),
      enabled: Some(true),
    },
  )
  .await?;
  OAuthAccount::create(
    pool,
    &OAuthAccountInsertForm::new(user.id, provider.id, "external-user".into()),
  )
  .await?;

  let (second_context, second_tenant) = site_context(&base, second.id).await?;
  let request = AuthenticateWithOauth {
    code: "code".into(),
    oauth_provider_id: provider.id,
    redirect_uri: Url::parse(&format!(
      "{}/oauth/callback",
      second_context.protocol_and_hostname()
    ))?,
    show_nsfw: None,
    username: None,
    answer: None,
    pkce_code_verifier: None,
    totp_2fa_token: None,
    stay_logged_in: None,
  };
  let result = authenticate_with_oauth(
    web::Json(request.clone()),
    TestRequest::default().to_http_request(),
    second_context,
    second_tenant,
  )
  .await;
  assert!(
    matches!(result, Err(ref error) if matches!(error.error_type, LemmyErrorType::OauthAuthorizationInvalid))
  );
  assert_eq!(hits.load(Ordering::SeqCst), 0);

  let (first_context, first_tenant) = site_context(&base, first.local_site.id).await?;
  let request = AuthenticateWithOauth {
    redirect_uri: Url::parse(&format!(
      "{}/oauth/callback",
      first_context.protocol_and_hostname()
    ))?,
    ..request
  };
  let response = authenticate_with_oauth(
    web::Json(request),
    TestRequest::default().to_http_request(),
    first_context,
    first_tenant,
  )
  .await?;
  assert!(response.jwt.is_some());
  assert_eq!(hits.load(Ordering::SeqCst), 2);

  handle.stop(true).await;
  Instance::delete(pool, second_instance.id).await?;
  first.delete(pool).await
}
