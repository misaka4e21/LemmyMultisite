use activitypub_federation::http_signatures::generate_actor_keypair;
use chrono::Utc;
use diesel::{
  dsl::{exists, not, select},
  query_builder::AsQuery,
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_api_utils::utils::generate_inbox_url;
use lemmy_db_schema::{
  impls::presence::ensure_local_person_site,
  source::{
    instance::Instance,
    local_site::{LocalSite, LocalSiteInsertForm},
    local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
    local_user::{LocalUser, LocalUserInsertForm},
    person::{Person, PersonInsertForm},
    site::{Site, SiteUpsertForm},
  },
  traits::ApubActor,
};
use lemmy_db_schema_file::schema::local_site;
use lemmy_db_views_site::SiteView;
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  sensitive::SensitiveString,
  traits::Crud,
};
use lemmy_utils::{
  error::{LemmyError, LemmyErrorExt, LemmyErrorType, LemmyResult},
  settings::structs::Settings,
};
use rand::{RngExt, distr::Alphanumeric};
use tracing::info;
use url::Url;

pub async fn setup_local_site(pool: &mut DbPool<'_>, settings: &Settings) -> LemmyResult<SiteView> {
  let conn = &mut get_conn(pool).await?;
  // Check to see if local_site exists, without the cache wrapper
  if select(not(exists(local_site::table.as_query())))
    .get_result(conn)
    .await?
  {
    info!("No Local Site found, creating it.");
    return create_local_site(pool, settings).await;
  }

  let hostname = settings
    .bootstrap_hostname()
    .ok_or_else(|| LemmyErrorType::Unknown("must have hostname".into()))?;
  SiteView::read_by_hostname(pool, hostname).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use diesel::{ExpressionMethods, QueryDsl};
  use diesel_async::RunQueryDsl;
  use lemmy_db_schema::source::{
    instance::Instance,
    local_site::{LocalSite, LocalSiteInsertForm},
    person::{Person, PersonInsertForm},
    site::Site,
  };
  use lemmy_db_schema_file::schema::{person, person_site};
  use lemmy_db_views_person::PersonView;
  use lemmy_diesel_utils::{
    connection::{build_db_pool_for_tests, get_conn},
    traits::Crud,
  };
  use lemmy_utils::settings::structs::{Settings, SetupConfig};
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn bootstrap_admin_and_system_have_site_presence() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();

    // Two existing LocalSites prevent Person::create from choosing a default site.
    let mut dummy_instances = Vec::new();
    for index in 0..2 {
      let domain = format!("bootstrap-dummy-{unique}-{index}.example");
      let instance = Instance::read_or_create(pool, &domain).await?;
      let site = Site::create(
        pool,
        &SiteUpsertForm::new(format!("dummy-{index}"), instance.id),
      )
      .await?;
      let system = Person::create(
        pool,
        &PersonInsertForm::test_form(instance.id, &format!("dummy_system_{index}")),
      )
      .await?;
      LocalSite::create(pool, &LocalSiteInsertForm::new(site.id, system.id, domain)).await?;
      dummy_instances.push(instance);
    }

    let hostname = format!("bootstrap-{unique}.example");
    let mut settings = Settings::default();
    settings.setup = Some(SetupConfig {
      hostname: hostname.clone(),
      admin_username: Some("bootstrap_admin".to_string()),
      admin_password: Some("test-password".to_string()),
      admin_email: Some("admin@example.com".to_string()),
      site_name: Some("Bootstrap".to_string()),
      ..Default::default()
    });

    let view = create_local_site(pool, &settings).await?;
    let local_site_id = view.local_site.id;
    let instance_id = view.site.instance_id;

    let (admin, system) = {
      let conn = &mut get_conn(pool).await?;
      let admin: Person = person::table
        .filter(person::name.eq("bootstrap_admin"))
        .filter(person::instance_id.eq(instance_id))
        .first(conn)
        .await?;
      let system: Person = person::table
        .find(view.local_site.system_account)
        .first(conn)
        .await?;
      for id in [admin.id, system.id] {
        let presence: i64 = person_site::table
          .filter(person_site::person_id.eq(id))
          .count()
          .get_result(conn)
          .await?;
        assert_eq!(presence, 1);
      }
      (admin, system)
    };

    assert_eq!(admin.origin_local_site_id, Some(local_site_id));
    assert_eq!(system.origin_local_site_id, Some(local_site_id));

    let admins = PersonView::list_admins(None, instance_id, pool).await?;
    assert!(admins.iter().any(|view| view.person.id == admin.id));
    let read = PersonView::read(pool, admin.id, None, instance_id, false).await?;
    assert_eq!(read.person.id, admin.id);

    Instance::delete(pool, view.site.instance_id).await?;
    for instance in dummy_instances {
      Instance::delete(pool, instance.id).await?;
    }
    Ok(())
  }
}

async fn create_local_site(pool: &mut DbPool<'_>, settings: &Settings) -> LemmyResult<SiteView> {
  let conn = &mut get_conn(pool).await?;
  let domain = settings
    .get_hostname_without_port()
    .with_lemmy_type(LemmyErrorType::Unknown("must have domain".into()))?;
  let hostname = settings
    .bootstrap_hostname()
    .ok_or_else(|| LemmyErrorType::Unknown("must have hostname".into()))?;
  let mut admin_setup = None;

  conn
    .transaction::<(), LemmyError, _>(async |conn| {
      // Upsert this to the instance table
      let instance = Instance::read_or_create(&mut conn.into(), &domain).await?;

      let setup = settings.setup.as_ref();
      if let Some(setup) = setup
        && let Some(admin_username) = &setup.admin_username
        && let Some(admin_password) = &setup.admin_password
      {
        let person_keypair = generate_actor_keypair()?;
        let person_ap_id =
          Person::generate_local_actor_url(admin_username, &settings.get_protocol_and_hostname())?;

        // Register the user if there's a site setup
        let person_form = PersonInsertForm {
          ap_id: Some(person_ap_id.clone()),
          inbox_url: Some(generate_inbox_url(&settings.get_protocol_and_hostname())?),
          private_key: Some(person_keypair.private_key),
          ..PersonInsertForm::new(
            admin_username.clone(),
            person_keypair.public_key,
            instance.id,
          )
        };
        let person_inserted = Person::create(&mut conn.into(), &person_form).await?;

        admin_setup = Some((
          person_inserted,
          Some(admin_password.clone()),
          setup.admin_email.clone(),
        ));
      };

      // Add an entry for the site table
      let site_key_pair = generate_actor_keypair()?;
      let site_ap_id = Url::parse(&settings.get_protocol_and_hostname())?;

      let name = settings
        .setup
        .clone()
        .and_then(|s| s.site_name)
        .unwrap_or_else(|| "New Site".to_string());
      let site_form = SiteUpsertForm {
        ap_id: Some(site_ap_id.clone().into()),
        last_refreshed_at: Some(Utc::now()),
        inbox_url: Some(generate_inbox_url(&settings.get_protocol_and_hostname())?),
        private_key: Some(Some(site_key_pair.private_key)),
        public_key: Some(site_key_pair.public_key),
        ..SiteUpsertForm::new(name, instance.id)
      };
      let site = Site::create(&mut conn.into(), &site_form).await?;
      // create multi-comm follower account
      let r: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(14)
        .map(char::from)
        .collect();
      let name = format!("lemmy_{}", r);
      let form = PersonInsertForm {
        private_key: site.private_key.map(SensitiveString::into_inner),
        inbox_url: Some(site.inbox_url),
        bot_account: Some(true),
        ..PersonInsertForm::new(name, site.public_key, instance.id)
      };
      let system_account = Person::create(&mut conn.into(), &form).await?;

      // Finally create the local_site row
      let site_setup = admin_setup.is_some();
      let local_site_form = LocalSiteInsertForm {
        site_setup: Some(site_setup),
        ..LocalSiteInsertForm::new(site.id, system_account.id, hostname.to_string())
      };
      let local_site = LocalSite::create(&mut conn.into(), &local_site_form).await?;

      if let Some((admin_person, _, _)) = &admin_setup {
        ensure_local_person_site(&mut conn.into(), local_site.id, admin_person.id).await?;
      }
      ensure_local_person_site(&mut conn.into(), local_site.id, system_account.id).await?;

      if let Some((admin_person, admin_password, admin_email)) = admin_setup {
        let local_user_form = LocalUserInsertForm {
          email: admin_email,
          admin: Some(true),
          ..LocalUserInsertForm::new(admin_person.id, local_site.id, admin_password)
        };
        LocalUser::create(&mut conn.into(), &local_user_form, vec![]).await?;
      }

      // Create the rate limit table
      let local_site_rate_limit_form = LocalSiteRateLimitInsertForm::new(local_site.id);
      LocalSiteRateLimit::create(&mut conn.into(), &local_site_rate_limit_form).await?;
      Ok(())
    })
    .await?;

  SiteView::read_by_hostname(pool, hostname).await
}
