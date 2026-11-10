use activitypub_federation::{config::FederationConfig, traits::Activity};
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext};
use lemmy_apub_activities::activity_lists::SharedInboxActivities;
use lemmy_apub_objects::utils::functions::{check_apub_id_valid, local_site_data_cached};
use lemmy_db_schema::source::{
  activity::{ReceivedActivity, SentActivity},
  internal_activity_delivery::InternalActivityDelivery,
};
use lemmy_db_views_site::SiteView;
use lemmy_utils::error::LemmyResult;
use std::{sync::Arc, time::Duration};
use tracing::{debug, warn};

const INTERNAL_DELIVERY_BATCH: i64 = 10;
const INTERNAL_DELIVERY_DELAY: Duration = Duration::from_secs(1);

pub async fn process_internal_deliveries(context: LemmyContext) -> LemmyResult<usize> {
  let pool = &mut context.pool();
  let deliveries = InternalActivityDelivery::next_pending(pool, INTERNAL_DELIVERY_BATCH).await?;
  let count = deliveries.len();

  for delivery in deliveries {
    let result: LemmyResult<()> = async {
      let sent = SentActivity::read(pool, delivery.activity_id).await?;
      let site_view = SiteView::read_by_id(pool, delivery.destination_site_id).await?;
      let hostname = site_view.local_site.hostname.clone();
      let tenant = Arc::new(TenantContext {
        hostname: hostname.clone(),
        scheme: site_view.site.ap_id.scheme().to_string(),
        site_view,
      });
      let destination_context = context.with_tenant(tenant);
      let data = FederationConfig::builder()
        .domain(hostname)
        .app_data(destination_context)
        .build()
        .await?
        .to_request_data();
      let activity: SharedInboxActivities = serde_json::from_value(sent.data.clone())?;

      let local_site_data = local_site_data_cached(pool, delivery.destination_site_id).await?;
      if let Some(actor) = &sent.actor_apub_id
        && check_apub_id_valid(actor.inner(), &local_site_data).is_err()
      {
        InternalActivityDelivery::mark_done(pool, delivery.id).await?;
        return Ok(());
      }

      // A duplicate received_activity row means another delivery already processed it.
      if ReceivedActivity::create(pool, delivery.destination_site_id, &sent.ap_id)
        .await
        .is_err()
      {
        InternalActivityDelivery::mark_done(pool, delivery.id).await?;
        return Ok(());
      }

      activity.receive(&data).await?;
      InternalActivityDelivery::mark_done(pool, delivery.id).await
    }
    .await;

    if let Err(error) = result {
      warn!(
        "Internal activity delivery {} -> {} failed: {}",
        delivery.origin_site_id.0, delivery.destination_site_id.0, error
      );
      InternalActivityDelivery::mark_failed(pool, &delivery).await?;
    }
  }
  Ok(count)
}

pub async fn run_internal_delivery_loop(context: LemmyContext) {
  loop {
    match process_internal_deliveries(context.clone()).await {
      Ok(count) if count > 0 => debug!("Processed {count} internal activity deliveries"),
      Ok(_) => {}
      Err(error) => warn!("Internal activity dispatcher failed: {error}"),
    }
    tokio::time::sleep(INTERNAL_DELIVERY_DELAY).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use lemmy_db_schema::source::{
    activity::{SentActivity, SentActivityForm},
    community::{Community, CommunityInsertForm},
    instance::Instance,
    internal_activity_delivery::InternalActivityDeliveryForm,
    local_site::{LocalSite, LocalSiteInsertForm},
    local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
    person::{Person, PersonInsertForm},
    site::{Site, SiteUpsertForm},
  };
  use lemmy_db_schema_file::enums::ActorType;
  use lemmy_db_views_community_follower::CommunityFollowerView;
  use lemmy_diesel_utils::{connection::build_db_pool_for_tests, traits::Crud};
  use serde_json::json;
  use serial_test::serial;
  use url::Url;

  #[tokio::test]
  #[serial]
  async fn internal_follow_creates_accept() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let a_domain = format!("a-{suffix}.example");
    let b_domain = format!("b-{suffix}.example");

    let a_instance = Instance::read_or_create(pool, &a_domain).await?;
    let b_instance = Instance::read_or_create(pool, &b_domain).await?;
    let a_site = Site::create(pool, &SiteUpsertForm::new("site a".into(), a_instance.id)).await?;
    let b_site = Site::create(pool, &SiteUpsertForm::new("site b".into(), b_instance.id)).await?;
    let a_system = Person::create(
      pool,
      &PersonInsertForm::test_form(a_instance.id, "a_system"),
    )
    .await?;
    let b_system = Person::create(
      pool,
      &PersonInsertForm::test_form(b_instance.id, "b_system"),
    )
    .await?;
    let a_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(a_site.id, a_system.id, a_domain.clone()),
    )
    .await?;
    let b_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(b_site.id, b_system.id, b_domain.clone()),
    )
    .await?;
    LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(a_local_site.id)).await?;
    LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(b_local_site.id)).await?;

    let alice = Person::create(
      pool,
      &PersonInsertForm {
        ap_id: Some(Url::parse(&format!("https://{a_domain}/u/alice"))?.into()),
        inbox_url: Some(Url::parse(&format!("https://{a_domain}/u/alice/inbox"))?.into()),
        local: Some(true),
        origin_local_site_id: Some(a_local_site.id),
        ..PersonInsertForm::new("alice".into(), "pubkey".into(), a_instance.id)
      },
    )
    .await?;
    let community = Community::create(
      pool,
      &CommunityInsertForm {
        ap_id: Some(Url::parse(&format!("https://{b_domain}/c/main"))?.into()),
        inbox_url: Some(Url::parse(&format!("https://{b_domain}/c/main/inbox"))?.into()),
        local: Some(true),
        origin_local_site_id: Some(b_local_site.id),
        ..CommunityInsertForm::new(b_instance.id, "main".into(), "pubkey".into())
      },
    )
    .await?;

    let follow_id = Url::parse(&format!("https://{a_domain}/activities/follow/1"))?;
    let follow = SentActivity::create(
      pool,
      SentActivityForm {
        ap_id: follow_id.clone().into(),
        data: json!({
          "id": follow_id,
          "type": "Follow",
          "actor": alice.ap_id.clone(),
          "object": community.ap_id.clone(),
          "to": [community.ap_id.clone()],
        }),
        sensitive: true,
        send_inboxes: vec![],
        send_community_followers_of: None,
        send_all_instances: false,
        actor_type: ActorType::Person,
        actor_apub_id: alice.ap_id.clone(),
        local_site_id: a_local_site.id,
      },
    )
    .await?;
    InternalActivityDelivery::create(
      pool,
      &InternalActivityDeliveryForm {
        origin_site_id: a_local_site.id,
        destination_site_id: b_local_site.id,
        activity_id: follow.id,
      },
    )
    .await?;

    let context = LemmyContext::init_test_context().await;
    assert_eq!(process_internal_deliveries((*context).clone()).await?, 1);
    CommunityFollowerView::is_follower(community.id, a_instance.id, pool).await?;

    let b_sent = SentActivity::list_for_site(pool, b_local_site.id).await?;
    let accept = b_sent
      .iter()
      .find(|s| s.data.get("type").and_then(|t| t.as_str()) == Some("Accept"))
      .expect("community should send an Accept activity");
    assert_eq!(accept.local_site_id, b_local_site.id);

    InternalActivityDelivery::create(
      pool,
      &InternalActivityDeliveryForm {
        origin_site_id: b_local_site.id,
        destination_site_id: a_local_site.id,
        activity_id: accept.id,
      },
    )
    .await?;
    assert_eq!(process_internal_deliveries((*context).clone()).await?, 1);
    CommunityFollowerView::is_follower(community.id, a_instance.id, pool).await?;

    Instance::delete(pool, a_instance.id).await?;
    Instance::delete(pool, b_instance.id).await?;
    Ok(())
  }
}
