use chrono::{DateTime, Utc};
use lemmy_db_schema_file::{
  newtypes::{LocalSiteId, PostId, PrivateMessageId},
  schema::protected_media_reference,
};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "full", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "full", diesel(table_name = protected_media_reference))]
#[cfg_attr(feature = "full", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct ProtectedMediaReference {
  pub id: i64,
  pub local_site_id: LocalSiteId,
  pub file_reference_id: Option<i32>,
  pub media_reference_id: Option<i64>,
  pub post_id: Option<PostId>,
  pub private_message_id: Option<PrivateMessageId>,
  pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
#[cfg_attr(feature = "full", derive(Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = protected_media_reference))]
pub struct ProtectedMediaReferenceInsertForm {
  pub local_site_id: LocalSiteId,
  pub file_reference_id: Option<i32>,
  pub media_reference_id: Option<i64>,
  pub post_id: Option<PostId>,
  pub private_message_id: Option<PrivateMessageId>,
}
