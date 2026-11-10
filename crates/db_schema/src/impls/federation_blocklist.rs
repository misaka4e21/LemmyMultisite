use crate::source::federation_blocklist::{FederationBlockList, FederationBlockListForm};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, delete, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{InstanceId, newtypes::LocalSiteId, schema::federation_blocklist};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl FederationBlockList {
  pub async fn block(pool: &mut DbPool<'_>, form: &FederationBlockListForm) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    insert_into(federation_blocklist::table)
      .values(form)
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }
  pub async fn unblock(
    pool: &mut DbPool<'_>,
    local_site_id_: LocalSiteId,
    instance_id_: InstanceId,
  ) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    delete(
      federation_blocklist::table.filter(
        federation_blocklist::local_site_id
          .eq(local_site_id_)
          .and(federation_blocklist::instance_id.eq(instance_id_)),
      ),
    )
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::Deleted)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    source::{
      instance::Instance,
      local_site::{LocalSite, LocalSiteInsertForm},
      person::{Person, PersonInsertForm},
      site::{Site, SiteUpsertForm},
    },
    test_data::TestData,
  };
  use chrono::Utc;
  use lemmy_diesel_utils::{connection::build_db_pool_for_tests, traits::Crud};
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn blocklist_is_site_scoped() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let remote = Instance::read_or_create(pool, &format!("blocked-{unique}.example")).await?;
    let second_instance =
      Instance::read_or_create(pool, &format!("second-blocklist-{unique}.example")).await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("second_site".into(), second_instance.id),
    )
    .await?;
    let second_system = Person::create(
      pool,
      &PersonInsertForm {
        local: Some(false),
        ..PersonInsertForm::test_form(second_instance.id, "second_blocklist_system")
      },
    )
    .await?;
    let second_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(
        second_site.id,
        second_system.id,
        second_instance.domain.clone(),
      ),
    )
    .await?;

    FederationBlockList::block(
      pool,
      &FederationBlockListForm::new(remote.id, data.local_site.id, None),
    )
    .await?;

    let first = Instance::read_federated_with_blocked_and_dead(pool, data.local_site.id)
      .await?
      .into_iter()
      .find(|(instance, _, _)| instance.id == remote.id)
      .ok_or(LemmyErrorType::NotFound)?;
    let second = Instance::read_federated_with_blocked_and_dead(pool, second_local_site.id)
      .await?
      .into_iter()
      .find(|(instance, _, _)| instance.id == remote.id)
      .ok_or(LemmyErrorType::NotFound)?;
    assert!(!first.1);
    assert!(second.1);

    Instance::delete(pool, second_instance.id).await?;
    data.delete(pool).await
  }
}
