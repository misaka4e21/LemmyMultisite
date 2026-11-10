use crate::source::internal_activity_delivery::{
  InternalActivityDelivery,
  InternalActivityDeliveryForm,
};
use chrono::Utc;
use diesel::{
  BoolExpressionMethods,
  ExpressionMethods,
  QueryDsl,
  SelectableHelper,
  dsl::insert_into,
  update,
};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::schema::internal_activity_delivery;
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl InternalActivityDelivery {
  pub async fn create(
    pool: &mut DbPool<'_>,
    form: &InternalActivityDeliveryForm,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    insert_into(internal_activity_delivery::table)
      .values(form)
      .on_conflict_do_nothing()
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    Ok(())
  }

  pub async fn next_pending(pool: &mut DbPool<'_>, limit: i64) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    internal_activity_delivery::table
      .filter(
        internal_activity_delivery::state
          .eq("pending")
          .or(internal_activity_delivery::state.eq("failed")),
      )
      .filter(internal_activity_delivery::attempts.lt(5))
      .order_by(internal_activity_delivery::created_at)
      .limit(limit)
      .select(Self::as_select())
      .load(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn mark_done(pool: &mut DbPool<'_>, id: i64) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    update(internal_activity_delivery::table.find(id))
      .set((
        internal_activity_delivery::state.eq("done"),
        internal_activity_delivery::updated_at.eq(Utc::now()),
      ))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
    Ok(())
  }

  pub async fn mark_failed(pool: &mut DbPool<'_>, delivery: &Self) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    update(internal_activity_delivery::table.find(delivery.id))
      .set((
        internal_activity_delivery::state.eq("failed"),
        internal_activity_delivery::attempts.eq(delivery.attempts + 1),
        internal_activity_delivery::updated_at.eq(Utc::now()),
      ))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    source::activity::{SentActivity, SentActivityForm},
    test_data::TestData,
  };
  use lemmy_db_schema_file::enums::ActorType;
  use lemmy_diesel_utils::connection::build_db_pool_for_tests;
  use serde_json::json;
  use serial_test::serial;
  use url::Url;

  #[tokio::test]
  #[serial]
  async fn delivery_rows_are_idempotent() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    let sent = SentActivity::create(
      pool,
      SentActivityForm {
        ap_id: Url::parse("https://a.example/activities/follow/1")?.into(),
        data: json!({
          "id": "https://a.example/activities/follow/1",
          "type": "Follow",
          "actor": data.person.ap_id.clone(),
          "object": "https://b.example/u/bob"
        }),
        sensitive: false,
        send_inboxes: vec![],
        send_community_followers_of: None,
        send_all_instances: false,
        actor_type: ActorType::Person,
        actor_apub_id: data.person.ap_id.clone(),
        local_site_id: data.local_site.id,
      },
    )
    .await?;

    let form = InternalActivityDeliveryForm {
      origin_site_id: data.local_site.id,
      destination_site_id: data.local_site.id,
      activity_id: sent.id,
    };
    InternalActivityDelivery::create(pool, &form).await?;
    InternalActivityDelivery::create(pool, &form).await?;
    let pending = InternalActivityDelivery::next_pending(pool, 10).await?;
    assert_eq!(pending.len(), 1);

    InternalActivityDelivery::mark_done(pool, pending[0].id).await?;
    let pending = InternalActivityDelivery::next_pending(pool, 10).await?;
    assert!(pending.is_empty());

    data.delete(pool).await
  }
}
