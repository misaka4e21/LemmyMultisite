use chrono::{DateTime, Utc};
use lemmy_db_schema_file::{
  newtypes::{ActivityId, LocalSiteId},
  schema::internal_activity_delivery,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "full", derive(diesel::Queryable, diesel::Selectable))]
#[cfg_attr(feature = "full", diesel(table_name = internal_activity_delivery))]
pub struct InternalActivityDelivery {
  pub id: i64,
  pub origin_site_id: LocalSiteId,
  pub destination_site_id: LocalSiteId,
  pub activity_id: ActivityId,
  pub state: String,
  pub attempts: i32,
  pub created_at: DateTime<Utc>,
  pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "full", derive(diesel::Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = internal_activity_delivery))]
pub struct InternalActivityDeliveryForm {
  pub origin_site_id: LocalSiteId,
  pub destination_site_id: LocalSiteId,
  pub activity_id: ActivityId,
}
