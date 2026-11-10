use crate::protocol::collections::url_collection::UrlCollection;
use activitypub_federation::{config::Data, traits::Object};
use actix_web::HttpResponse;
use lemmy_api_utils::context::LemmyContext;
use lemmy_apub_objects::objects::instance::ApubSite;
use lemmy_utils::{FEDERATION_CONTEXT, error::LemmyResult};

pub(crate) async fn get_apub_site_http(context: Data<LemmyContext>) -> LemmyResult<HttpResponse> {
  let site: ApubSite = context.site_view()?.site.clone().into();

  site.http_response(&FEDERATION_CONTEXT, &context).await
}

pub(crate) async fn get_apub_site_outbox(context: Data<LemmyContext>) -> LemmyResult<HttpResponse> {
  let outbox_id = format!("{}/site_outbox", context.protocol_and_hostname());
  UrlCollection::new_empty_response(outbox_id)
}
