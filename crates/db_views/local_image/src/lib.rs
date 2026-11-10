use lemmy_db_schema::source::{images::LocalImage, person::Person, post::Post};
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

pub mod api;
#[cfg(feature = "full")]
pub mod impls;

#[skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(optional_fields, export))]
/// A local image view.
pub struct LocalImageView {
  pub local_image: LocalImage,
  pub person: Person,
  pub post: Option<Post>,
}
