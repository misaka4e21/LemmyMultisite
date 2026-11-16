use chrono::{DateTime, Utc};
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{FileId, LocalSiteId},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "full", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "full", diesel(table_name = lemmy_db_schema_file::schema::file_object))]
#[cfg_attr(feature = "full", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct FileObject {
  pub id: FileId,
  pub content_hash: String,
  pub storage_key: String,
  pub size_bytes: i64,
  pub content_type: String,
  pub published_at: DateTime<Utc>,
}

#[derive(Clone)]
#[cfg_attr(feature = "full", derive(Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = lemmy_db_schema_file::schema::file_object))]
pub struct FileObjectInsertForm {
  pub content_hash: String,
  pub storage_key: String,
  pub size_bytes: i64,
  pub content_type: String,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "full", derive(Queryable, Selectable, Identifiable))]
#[cfg_attr(feature = "full", diesel(table_name = lemmy_db_schema_file::schema::file_reference))]
#[cfg_attr(feature = "full", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct FileReference {
  pub id: i32,
  pub public_id: Uuid,
  pub local_site_id: LocalSiteId,
  pub file_id: FileId,
  pub owner_person_id: PersonId,
  pub original_filename: String,
  pub published_at: DateTime<Utc>,
}

#[derive(Clone)]
#[cfg_attr(feature = "full", derive(Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = lemmy_db_schema_file::schema::file_reference))]
pub struct FileReferenceInsertForm {
  pub public_id: Uuid,
  pub local_site_id: LocalSiteId,
  pub file_id: FileId,
  pub owner_person_id: PersonId,
  pub original_filename: String,
}
