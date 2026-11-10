use chrono::Utc;
use diesel::{
  ExpressionMethods,
  QueryDsl,
  dsl::{delete, insert_into, update},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_api_utils::{tenant::TenantContext, utils::generate_inbox_url};
use lemmy_db_schema_file::{
  newtypes::LocalSiteId,
  schema::{instance, local_site, removed_site, site},
};
use lemmy_db_views_site::SiteView;
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  dburl::DbUrl,
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};
use url::Url;

fn normalize_hostname(hostname: &str) -> LemmyResult<String> {
  let hostname = hostname.trim().to_lowercase();
  let hostname = hostname.split(':').next().unwrap_or_default();
  let hostname = hostname.trim_end_matches('.');
  if hostname.is_empty() || hostname.contains('/') {
    return Err(LemmyErrorType::Unknown("invalid hostname".into()).into());
  }
  Ok(hostname.to_string())
}

pub async fn disable_site(pool: &mut DbPool<'_>, local_site_id: LocalSiteId) -> LemmyResult<()> {
  let view = SiteView::read_by_id(pool, local_site_id).await?;
  let conn = &mut get_conn(pool).await?;
  update(local_site::table.find(local_site_id))
    .set((
      local_site::disabled.eq(true),
      local_site::updated_at.eq(Utc::now()),
    ))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  SiteView::invalidate(local_site_id).await;
  TenantContext::invalidate(&view.local_site.hostname).await;
  Ok(())
}

pub async fn remove_site(pool: &mut DbPool<'_>, local_site_id: LocalSiteId) -> LemmyResult<()> {
  let view = SiteView::read_by_id(pool, local_site_id).await?;
  disable_site(pool, local_site_id).await?;
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<(), lemmy_utils::error::LemmyError, _>(async |conn| {
      insert_into(removed_site::table)
        .values((
          removed_site::hostname.eq(&view.local_site.hostname),
          removed_site::removed_at.eq(Utc::now()),
        ))
        .on_conflict_do_nothing()
        .execute(conn)
        .await?;

      delete(local_site::table.find(local_site_id))
        .execute(conn)
        .await?;
      delete(site::table.find(view.site.id)).execute(conn).await?;
      delete(instance::table.find(view.site.instance_id))
        .execute(conn)
        .await?;
      Ok(())
    })
    .await?;
  SiteView::invalidate(local_site_id).await;
  TenantContext::invalidate(&view.local_site.hostname).await;
  Ok(())
}

pub async fn change_hostname(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  new_hostname: &str,
) -> LemmyResult<SiteView> {
  let view = SiteView::read_by_id(pool, local_site_id).await?;
  let connector = &mut get_conn(pool).await?;
  let (disabled, local_users, local_posts, local_comments, local_communities, old_hostname) =
    local_site::table
      .find(local_site_id)
      .select((
        local_site::disabled,
        local_site::local_users,
        local_site::local_posts,
        local_site::local_comments,
        local_site::local_communities,
        local_site::hostname,
      ))
      .first::<(bool, i32, i32, i32, i32, String)>(connector)
      .await?;
  if disabled {
    return Err(LemmyErrorType::CouldntUpdate.into());
  }
  if local_users > 0 || local_posts > 0 || local_comments > 0 || local_communities > 0 {
    return Err(LemmyErrorType::CouldntUpdate.into());
  }

  let new_hostname = normalize_hostname(new_hostname)?;
  if new_hostname == old_hostname {
    return Ok(view);
  }
  if SiteView::read_by_hostname(pool, &new_hostname)
    .await
    .is_ok()
  {
    return Err(LemmyErrorType::AlreadyExists.into());
  }

  let scheme = view.site.ap_id.scheme().to_string();
  let base_url = format!("{scheme}://{new_hostname}");
  let ap_id: Url = Url::parse(&base_url)?;
  let ap_id: DbUrl = ap_id.into();
  let inbox_url = generate_inbox_url(&base_url)?;

  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<(), lemmy_utils::error::LemmyError, _>(async |conn| {
      update(local_site::table.find(local_site_id))
        .set((
          local_site::hostname.eq(&new_hostname),
          local_site::updated_at.eq(Utc::now()),
        ))
        .execute(conn)
        .await?;
      update(site::table.find(view.site.id))
        .set((site::ap_id.eq(ap_id), site::inbox_url.eq(inbox_url)))
        .execute(conn)
        .await?;
      update(instance::table.find(view.site.instance_id))
        .set(instance::domain.eq(&new_hostname))
        .execute(conn)
        .await?;
      Ok(())
    })
    .await?;

  TenantContext::invalidate(&old_hostname).await;
  TenantContext::invalidate(&new_hostname).await;
  SiteView::invalidate(local_site_id).await;
  SiteView::read_by_id(pool, local_site_id).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::utils::provision_site::{ProvisionSite, provision_site};
  use diesel::update;
  use lemmy_db_schema::{source::instance::Instance, test_data::TestData};
  use lemmy_diesel_utils::connection::build_db_pool_for_tests;
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn disable_and_remove_site() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let first_host = format!("lifecycle-a-{unique}.example");
    let second_host = format!("lifecycle-b-{unique}.example");
    let first = provision_site(
      pool,
      ProvisionSite {
        hostname: first_host.clone(),
        site_name: "Lifecycle A".to_string(),
        scheme: "https".to_string(),
        admin_username: "lifecycle_a".to_string(),
        admin_password: "password".to_string(),
        admin_email: None,
        smtp_connection: None,
        smtp_from_address: None,
      },
    )
    .await?;
    let second = provision_site(
      pool,
      ProvisionSite {
        hostname: second_host.clone(),
        site_name: "Lifecycle B".to_string(),
        scheme: "https".to_string(),
        admin_username: "lifecycle_b".to_string(),
        admin_password: "password".to_string(),
        admin_email: None,
        smtp_connection: None,
        smtp_from_address: None,
      },
    )
    .await?;

    disable_site(pool, first.local_site.id).await?;
    assert!(SiteView::read_by_hostname(pool, &first_host).await.is_err());
    assert!(SiteView::read_by_hostname(pool, &second_host).await.is_ok());

    remove_site(pool, first.local_site.id).await?;
    assert!(
      SiteView::read_by_id(pool, first.local_site.id)
        .await
        .is_err()
    );
    assert!(SiteView::read_by_hostname(pool, &second_host).await.is_ok());

    let duplicate = provision_site(
      pool,
      ProvisionSite {
        hostname: first_host,
        site_name: "Reused".to_string(),
        scheme: "https".to_string(),
        admin_username: "reused".to_string(),
        admin_password: "password".to_string(),
        admin_email: None,
        smtp_connection: None,
        smtp_from_address: None,
      },
    )
    .await;
    assert!(duplicate.is_err());

    Instance::delete(pool, second.site.instance_id).await?;
    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn hostname_change_is_blocked_after_users() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let old_host = data.local_site.hostname.clone();
    let new_host = format!("renamed-{unique}.example");

    let conn = &mut get_conn(pool).await?;
    update(local_site::table.find(data.local_site.id))
      .set((
        local_site::local_users.eq(0),
        local_site::local_posts.eq(0),
        local_site::local_comments.eq(0),
        local_site::local_communities.eq(0),
      ))
      .execute(conn)
      .await?;

    change_hostname(pool, data.local_site.id, &new_host).await?;
    assert!(SiteView::read_by_hostname(pool, &new_host).await.is_ok());
    assert!(SiteView::read_by_hostname(pool, &old_host).await.is_err());

    let conn = &mut get_conn(pool).await?;
    update(local_site::table.find(data.local_site.id))
      .set(local_site::local_users.eq(1))
      .execute(conn)
      .await?;
    assert!(
      change_hostname(
        pool,
        data.local_site.id,
        &format!("blocked-{unique}.example")
      )
      .await
      .is_err()
    );

    data.delete(pool).await
  }
}
