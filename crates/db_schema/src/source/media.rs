use chrono::{DateTime, Utc};
#[cfg(feature = "full")]
use lemmy_db_schema_file::schema::{media_object, media_reference};
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{LocalSiteId, PostId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(
  feature = "full",
  derive(
    diesel::Queryable,
    diesel::Selectable,
    diesel::Identifiable,
    i_love_jesus::CursorKeysModule
  )
)]
#[cfg_attr(feature = "full", diesel(table_name = media_object))]
#[cfg_attr(feature = "full", diesel(check_for_backend(diesel::pg::Pg)))]
#[cfg_attr(feature = "full", cursor_keys_module(name = media_object_keys))]
pub struct MediaObject {
  pub id: i64,
  pub content_hash: String,
  pub canonical_pictrs_alias: String,
  pub size_bytes: i64,
  pub width: i32,
  pub height: i32,
  pub content_type: String,
  pub blurhash: Option<String>,
  pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "full", derive(diesel::Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = media_object))]
pub struct MediaObjectInsertForm {
  pub content_hash: String,
  pub canonical_pictrs_alias: String,
  pub size_bytes: i64,
  pub width: i32,
  pub height: i32,
  pub content_type: String,
  pub blurhash: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(
  feature = "full",
  derive(diesel::Queryable, diesel::Selectable, diesel::Identifiable)
)]
#[cfg_attr(feature = "full", diesel(table_name = media_reference))]
#[cfg_attr(feature = "full", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct MediaReference {
  pub id: i64,
  pub local_site_id: LocalSiteId,
  pub media_id: i64,
  pub owner_person_id: Option<PersonId>,
  pub post_id: Option<PostId>,
  pub purpose: String,
  pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "full", derive(diesel::Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = media_reference))]
pub struct MediaReferenceInsertForm {
  pub local_site_id: LocalSiteId,
  pub media_id: i64,
  pub owner_person_id: Option<PersonId>,
  pub post_id: Option<PostId>,
  pub purpose: String,
}
