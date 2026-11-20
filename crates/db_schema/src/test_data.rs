use crate::source::{
  instance::Instance,
  local_site::{LocalSite, LocalSiteInsertForm},
  local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
  person::{Person, PersonInsertForm},
  site::{Site, SiteUpsertForm},
};
use lemmy_diesel_utils::{connection::DbPool, traits::Crud};
use lemmy_utils::error::LemmyResult;
use chrono::Utc;

pub struct TestData {
  pub instance: Instance,
  pub site: Site,
  pub local_site: LocalSite,
  pub person: Person,
}

impl TestData {
  pub async fn create(pool: &mut DbPool<'_>) -> LemmyResult<Self> {
    let domain = format!("test-{}.example", Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let instance = Instance::read_or_create(pool, &domain).await?;

    let site_form = SiteUpsertForm {
      ap_id: Some(url::Url::parse(&format!("http://{domain}"))?.into()),
      ..SiteUpsertForm::new("test site".to_string(), instance.id)
    };
    let site = Site::create(pool, &site_form).await?;

    let system_acct =
      Person::create(pool, &PersonInsertForm::test_form(instance.id, "langs")).await?;
    let local_site_form = LocalSiteInsertForm {
      ..LocalSiteInsertForm::new(site.id, system_acct.id, instance.domain.clone())
    };
    let local_site = LocalSite::create(pool, &local_site_form).await?;
    LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(local_site.id)).await?;

    let person_form = PersonInsertForm {
      origin_local_site_id: Some(local_site.id),
      ..PersonInsertForm::test_form(instance.id, "holly")
    };

    let person = Person::create(pool, &person_form).await?;

    Ok(Self {
      instance,
      site,
      local_site,
      person,
    })
  }

  pub async fn delete(self, pool: &mut DbPool<'_>) -> LemmyResult<()> {
    Instance::delete(pool, self.instance.id).await?;
    Ok(())
  }
}
