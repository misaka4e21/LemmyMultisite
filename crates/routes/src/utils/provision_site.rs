use activitypub_federation::http_signatures::generate_actor_keypair;
use chrono::Utc;
use diesel::{
  ExpressionMethods,
  QueryDsl,
  dsl::{exists, select},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_api_utils::{tenant::canonicalize_host, utils::generate_inbox_url};
use lemmy_db_schema::source::{
  instance::Instance,
  local_site::{LocalSite, LocalSiteInsertForm},
  local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
  local_user::{LocalUser, LocalUserInsertForm},
  person::{Person, PersonInsertForm},
  site::{Site, SiteUpsertForm},
};
use lemmy_db_schema_file::schema::removed_site;
use lemmy_db_views_site::SiteView;
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  sensitive::SensitiveString,
  traits::Crud,
};
use lemmy_utils::{
  error::{LemmyError, LemmyErrorType, LemmyResult},
  utils::validation::site_name_length_check,
};
use rand::{RngExt, distr::Alphanumeric};
use url::Url;

pub struct ProvisionSite {
  pub hostname: String,
  pub site_name: String,
  pub scheme: String,
  pub admin_username: String,
  pub admin_password: String,
  pub admin_email: Option<String>,
  pub smtp_connection: Option<String>,
  pub smtp_from_address: Option<String>,
}

/// Creates a complete site in one transaction: Instance, Site keys, system account, LocalSite,
/// rate limit, and the first site administrator.
pub async fn provision_site(pool: &mut DbPool<'_>, site: ProvisionSite) -> LemmyResult<SiteView> {
  let hostname = canonicalize_host(&site.hostname)?;
  site_name_length_check(&site.site_name)?;
  let base_url = format!("{}://{hostname}", site.scheme);
  let site_ap_id = Url::parse(&base_url)?;
  let inbox_url = generate_inbox_url(&base_url)?;

  let conn = &mut get_conn(pool).await?;
  let is_removed = select(exists(
    removed_site::table.filter(removed_site::hostname.eq(&hostname)),
  ))
  .get_result::<bool>(conn)
  .await?;
  if is_removed {
    return Err(LemmyErrorType::AlreadyExists.into());
  }
  let local_site_id = conn
    .transaction::<lemmy_db_schema_file::newtypes::LocalSiteId, LemmyError, _>(async |conn| {
      let instance = Instance::read_or_create(&mut conn.into(), &hostname).await?;
      if Site::read_from_instance_id(&mut conn.into(), instance.id)
        .await
        .is_ok()
      {
        return Err(LemmyErrorType::AlreadyExists.into());
      }

      let site_keypair = generate_actor_keypair()?;
      let site_form = SiteUpsertForm {
        ap_id: Some(site_ap_id.clone().into()),
        last_refreshed_at: Some(Utc::now()),
        inbox_url: Some(inbox_url.clone()),
        private_key: Some(Some(site_keypair.private_key)),
        public_key: Some(site_keypair.public_key),
        ..SiteUpsertForm::new(site.site_name.clone(), instance.id)
      };
      let site_actor = Site::create(&mut conn.into(), &site_form).await?;

      let system_name = format!(
        "lemmy_{}",
        rand::rng()
          .sample_iter(&Alphanumeric)
          .take(14)
          .map(char::from)
          .collect::<String>()
      );
      let system_form = PersonInsertForm {
        private_key: site_actor
          .private_key
          .clone()
          .map(SensitiveString::into_inner),
        inbox_url: Some(site_actor.inbox_url.clone()),
        bot_account: Some(true),
        ..PersonInsertForm::new(system_name, site_actor.public_key.clone(), instance.id)
      };
      let system_account = Person::create(&mut conn.into(), &system_form).await?;

      let local_site_form = LocalSiteInsertForm {
        site_setup: Some(true),
        smtp_connection: site.smtp_connection.clone(),
        smtp_from_address: site.smtp_from_address.clone(),
        ..LocalSiteInsertForm::new(site_actor.id, system_account.id, hostname.clone())
      };
      let local_site = LocalSite::create(&mut conn.into(), &local_site_form).await?;
      LocalSiteRateLimit::create(
        &mut conn.into(),
        &LocalSiteRateLimitInsertForm::new(local_site.id),
      )
      .await?;

      let admin_keypair = generate_actor_keypair()?;
      let admin_ap_id = Url::parse(&format!("{base_url}/u/{}", site.admin_username))?;
      let admin_form = PersonInsertForm {
        ap_id: Some(admin_ap_id.into()),
        inbox_url: Some(inbox_url),
        private_key: Some(admin_keypair.private_key),
        origin_local_site_id: Some(local_site.id),
        ..PersonInsertForm::new(
          site.admin_username.clone(),
          admin_keypair.public_key,
          instance.id,
        )
      };
      let admin = Person::create(&mut conn.into(), &admin_form).await?;
      let local_user_form = LocalUserInsertForm {
        email: site.admin_email.clone(),
        admin: Some(true),
        ..LocalUserInsertForm::new(admin.id, local_site.id, Some(site.admin_password.clone()))
      };
      LocalUser::create(&mut conn.into(), &local_user_form, vec![]).await?;
      Ok(local_site.id)
    })
    .await?;

  SiteView::read_by_id(pool, local_site_id).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::utils::provision_site::ProvisionSite;
  use chrono::Utc;
  use lemmy_db_schema::source::instance::Instance;
  use lemmy_diesel_utils::connection::build_db_pool_for_tests;
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn provision_site_is_transactional() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let hostname = format!("provision-{unique}.example");
    let input = ProvisionSite {
      hostname: hostname.clone(),
      site_name: "Provisioned site".to_string(),
      scheme: "https".to_string(),
      admin_username: "provision_admin".to_string(),
      admin_password: "test-password".to_string(),
      admin_email: None,
      smtp_connection: Some("smtps://site-a.example:465".to_string()),
      smtp_from_address: Some("noreply@site-a.example".to_string()),
    };

    let view = provision_site(pool, input).await?;
    assert_eq!(view.local_site.hostname, hostname);
    assert_eq!(
      view.local_site.smtp_connection.as_deref(),
      Some("smtps://site-a.example:465")
    );
    assert_eq!(
      view.local_site.smtp_from_address.as_deref(),
      Some("noreply@site-a.example")
    );
    assert_eq!(view.local_site_rate_limit.local_site_id, view.local_site.id);
    assert!(SiteView::read_by_hostname(pool, &hostname).await.is_ok());

    let duplicate = provision_site(
      pool,
      ProvisionSite {
        hostname: hostname.clone(),
        site_name: "Duplicate".to_string(),
        scheme: "https".to_string(),
        admin_username: "other_admin".to_string(),
        admin_password: "test-password".to_string(),
        admin_email: None,
        smtp_connection: None,
        smtp_from_address: None,
      },
    )
    .await;
    assert!(duplicate.is_err());

    Instance::delete(pool, view.site.instance_id).await?;
    Ok(())
  }
}
