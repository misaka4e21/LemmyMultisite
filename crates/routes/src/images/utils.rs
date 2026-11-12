use actix_web::{
  HttpRequest,
  http::{
    Method,
    header::{ACCEPT_ENCODING, HOST, HeaderName},
  },
  web::Data,
};
use diesel::NotFound;
use http::HeaderValue;
use lemmy_api_utils::{context::LemmyContext, request::delete_image_alias};
use lemmy_db_schema::source::media::MediaReference;
use lemmy_db_schema_file::{PersonId, newtypes::LocalSiteId};
use lemmy_diesel_utils::dburl::DbUrl;
use lemmy_utils::{REQWEST_TIMEOUT, error::LemmyResult};
use reqwest_middleware::RequestBuilder;
use url::Url;

pub(super) fn adapt_request(
  request: &HttpRequest,
  url: String,
  context: &LemmyContext,
) -> RequestBuilder {
  // remove accept-encoding header so that pictrs doesn't compress the response
  const INVALID_HEADERS: &[HeaderName] = &[ACCEPT_ENCODING, HOST];

  let client_request = context
    .pictrs_client()
    .request(convert_method(request.method()), url)
    .timeout(REQWEST_TIMEOUT);

  request
    .headers()
    .iter()
    .fold(client_request, |client_req, (key, value)| {
      if INVALID_HEADERS.contains(key) {
        client_req
      } else {
        // TODO: remove as_str and as_bytes conversions after actix-web upgrades to http 1.0
        client_req.header(key.as_str(), value.as_bytes())
      }
    })
}

// TODO: remove these conversions after actix-web upgrades to http 1.0
#[expect(clippy::expect_used)]
pub(super) fn convert_method(method: &Method) -> http::Method {
  http::Method::from_bytes(method.as_str().as_bytes()).expect("method can be converted")
}

pub(super) fn convert_header<'a>(
  name: &'a http::HeaderName,
  value: &'a HeaderValue,
) -> (&'a str, &'a [u8]) {
  (name.as_str(), value.as_bytes())
}

/// When adding a new avatar, banner or similar image, delete the old one.
pub(super) async fn delete_old_image(
  old_image: &Option<DbUrl>,
  new_image: Option<&Url>,
  local_site_id: LocalSiteId,
  person_id: PersonId,
  context: &Data<LemmyContext>,
) -> LemmyResult<()> {
  if let Some(old_image) = old_image {
    let old_alias = old_image.as_str().split('/').next_back().ok_or(NotFound)?;
    if let Some(new_image) = new_image {
      let new_alias = new_image
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .ok_or(NotFound)?;
      if old_alias == new_alias {
        return Ok(());
      }
    }

    let delete_object = MediaReference::delete_for_site_and_user(
      &mut context.pool(),
      old_alias,
      local_site_id,
      person_id,
    )
    .await?;
    if delete_object {
      delete_image_alias(old_alias, context).await?;
    }
  }
  Ok(())
}
