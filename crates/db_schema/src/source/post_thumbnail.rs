use chrono::{DateTime, Utc};
use lemmy_db_schema_file::{newtypes::PostId, schema::post_thumbnail_source};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "full", derive(diesel::Queryable, diesel::Selectable))]
#[cfg_attr(feature = "full", diesel(table_name = post_thumbnail_source))]
#[cfg_attr(feature = "full", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct PostThumbnailSource {
  pub post_id: PostId,
  pub source_hash: String,
  pub pictrs_alias: Option<String>,
  pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "full", derive(diesel::Insertable))]
#[cfg_attr(feature = "full", diesel(table_name = post_thumbnail_source))]
pub struct PostThumbnailSourceForm {
  pub post_id: PostId,
  pub source_hash: String,
  pub pictrs_alias: Option<String>,
}
