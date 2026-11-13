use actix_web::web::{Data, Json};
use lemmy_api_utils::{
  claims::Claims,
  context::LemmyContext,
  plugins::LemmyPlugins,
  tenant::TenantContext,
};
use lemmy_db_schema::source::{
  actor_language::SiteLanguage,
  language::Language,
  local_site_url_blocklist::LocalSiteUrlBlocklist,
  oauth_provider::AdminOAuthProvider,
  registration_application::RegistrationApplication,
  tagline::Tagline,
};
use lemmy_db_schema_file::newtypes::LocalSiteId;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_person::PersonView;
use lemmy_db_views_site::api::GetSiteResponse;
use lemmy_utils::{VERSION, error::LemmyResult};
use moka::future::Cache;
use std::sync::LazyLock;

static SITE_RESPONSE_CACHE: LazyLock<Cache<LocalSiteId, GetSiteResponse>> =
  LazyLock::new(|| Cache::builder().max_capacity(100).build());

pub async fn invalidate_site_response(local_site_id: LocalSiteId) {
  SITE_RESPONSE_CACHE.invalidate(&local_site_id).await;
}

pub async fn get_site(
  local_user_view: Option<LocalUserView>,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<GetSiteResponse>> {
  // This data is independent from the user account so we can cache it across requests
  let site_id = tenant.local_site().id;
  let mut site_response = match SITE_RESPONSE_CACHE.get(&site_id).await {
    Some(response) => response,
    None => {
      let response = read_site(&context, &tenant).await?;
      SITE_RESPONSE_CACHE.insert(site_id, response.clone()).await;
      response
    }
  };

  // filter oauth_providers for public access
  if !local_user_view
    .as_ref()
    .map(|l| l.local_user.admin)
    .unwrap_or_default()
  {
    site_response.admin_oauth_providers = vec![];
  }

  // Return the feed-only token so the frontend can generate RSS feed urls
  if let Some(l) = &local_user_view {
    site_response.my_feed_token =
      Some(Claims::get_or_generate_feed_only(l.local_user.id, &context).await?);
  }

  Ok(Json(site_response))
}

async fn read_site(context: &LemmyContext, tenant: &TenantContext) -> LemmyResult<GetSiteResponse> {
  let site_view = tenant.site_view.clone();
  let admins = PersonView::list_admins(None, site_view.instance.id, &mut context.pool()).await?;
  let all_languages = Language::read_all(&mut context.pool()).await?;
  let discussion_languages = SiteLanguage::read_local_raw(&mut context.pool()).await?;
  let blocked_urls =
    LocalSiteUrlBlocklist::get_all(&mut context.pool(), tenant.local_site().id).await?;
  let tagline = Tagline::get_random(&mut context.pool(), tenant.local_site().id)
    .await
    .ok();
  let admin_oauth_providers =
    AdminOAuthProvider::get_all(&mut context.pool(), tenant.local_site().id).await?;
  let oauth_providers =
    AdminOAuthProvider::convert_providers_to_public(admin_oauth_providers.clone());
  let last_application_duration_seconds =
    RegistrationApplication::last_updated(&mut context.pool())
      .await
      .ok()
      .and_then(|u| u.updated_published_duration());

  Ok(GetSiteResponse {
    site_view,
    admins,
    version: VERSION.to_string(),
    all_languages,
    discussion_languages,
    blocked_urls,
    tagline,
    oauth_providers,
    admin_oauth_providers,
    active_plugins: LemmyPlugins::metadata(),
    my_feed_token: None,
    last_application_duration_seconds,
    captcha_enabled: LemmyPlugins::get_or_init().is_captcha_plugin_loaded(),
  })
}
