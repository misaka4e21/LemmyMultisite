use chrono::{DateTime, Utc};
use lemmy_db_schema_file::schema::installation_admin;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(
  feature = "full",
  derive(diesel::Queryable, diesel::Selectable, diesel::Identifiable)
)]
#[cfg_attr(feature = "full", diesel(table_name = installation_admin))]
pub struct InstallationAdmin {
  pub id: i64,
  pub username: String,
  pub password_encrypted: String,
  pub created_at: DateTime<Utc>,
  pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "full", derive(diesel::Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = installation_admin))]
pub struct InstallationAdminInsertForm {
  pub username: String,
  pub password_encrypted: String,
}
