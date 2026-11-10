use activitypub_federation::{
  actix_web::{
    inbox::{ReceiveActivityHook, receive_activity_with_hook},
    response::create_http_response,
    signing_actor,
  },
  config::Data,
  traits::{Activity, Object},
};
use actix_web::{
  HttpRequest,
  HttpResponse,
  web::{self, Bytes},
};
use either::Either;
use lemmy_api_utils::{context::LemmyContext, plugins::plugin_hook_after};
use lemmy_apub_activities::activity_lists::SharedInboxActivities;
use lemmy_apub_objects::objects::{SiteOrMultiOrCommunityOrUser, UserOrCommunity};
use lemmy_db_schema::source::{
  activity::{ReceivedActivity, SentActivity},
  community::Community,
  instance::Instance,
  local_site::LocalSite,
};
use lemmy_db_schema_file::{InstanceId, enums::CommunityVisibility};
use lemmy_db_views_community_follower_approval::PendingFollowerView;
use lemmy_utils::{
  FEDERATION_CONTEXT,
  error::{LemmyErrorExt, LemmyErrorType, LemmyResult, UntranslatedError},
};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::timeout;
use tracing::debug;
use url::Url;

mod comment;
mod community;
mod person;
mod post;
pub mod routes;
pub mod site;

const INCOMING_ACTIVITY_TIMEOUT: Duration = Duration::from_secs(9);

pub async fn shared_inbox(
  request: HttpRequest,
  body: Bytes,
  data: Data<LemmyContext>,
) -> LemmyResult<HttpResponse> {
  let receive_fut =
    receive_activity_with_hook::<SharedInboxActivities, UserOrCommunity, LemmyContext>(
      request, body, Dummy, &data,
    );
  // Set a timeout shorter than `REQWEST_TIMEOUT` for processing incoming activities. This is to
  // avoid taking a long time to process an incoming activity when a required data fetch times out.
  // In this case our own instance would timeout and be marked as dead by the sender. Better to
  // consider the activity broken and move on.
  timeout(INCOMING_ACTIVITY_TIMEOUT, receive_fut)
    .await
    .with_lemmy_type(UntranslatedError::InboxTimeout.into())?
}

struct Dummy;

fn is_other_local_host(url: &Url, active_host: &str, local_hosts: &[String]) -> bool {
  url.host_str().is_some_and(|host| {
    !host.eq_ignore_ascii_case(active_host)
      && local_hosts
        .iter()
        .any(|local| local.eq_ignore_ascii_case(host))
  })
}

fn addresses_only_other_local_site(
  recipients: &[Url],
  active_host: &str,
  local_hosts: &[String],
) -> bool {
  let local_recipients = recipients
    .iter()
    .filter_map(|url| url.host_str())
    .filter(|host| {
      local_hosts
        .iter()
        .any(|local| local.eq_ignore_ascii_case(host))
    })
    .collect::<Vec<_>>();
  !local_recipients.is_empty()
    && !local_recipients
      .iter()
      .any(|host| host.eq_ignore_ascii_case(active_host))
}

impl ReceiveActivityHook<SharedInboxActivities, UserOrCommunity, LemmyContext> for Dummy {
  async fn hook(
    self,
    activity: &SharedInboxActivities,
    actor: &UserOrCommunity,
    context: &Data<LemmyContext>,
  ) -> LemmyResult<()> {
    let active_host = context.hostname();
    let local_hosts = LocalSite::list_hostnames(&mut context.pool()).await?;
    if is_other_local_host(activity.id(), active_host, &local_hosts)
      || is_other_local_host(activity.actor(), active_host, &local_hosts)
      || addresses_only_other_local_site(&activity.recipients(), active_host, &local_hosts)
    {
      return Err(LemmyErrorType::NotFound.into());
    }

    // Mark instance as alive
    // HTTP signature has already been verified at this point, so the
    // sending instance is authenticated and evidently online.
    // mark_alive also uses a cache so shouldn't hit the database too
    // often
    let instance_id = match actor {
      Either::Left(p) => p.instance_id,
      Either::Right(c) => c.instance_id,
    };
    Instance::mark_alive(&mut context.pool(), instance_id).await?;

    // Store received activities in the database. This ensures that the same activity doesn't get
    // received and processed more than once, which would be a waste of resources.
    debug!("Received activity {}", activity.id().to_string());
    ReceivedActivity::create(
      &mut context.pool(),
      context.site_view()?.local_site.id,
      &activity.id().clone().into(),
    )
    .await?;

    // This could also take the actor as param, but lifetimes and serde derives are tricky.
    // It is really a before hook, but doesnt allow modifying the data. It could use a
    // separate method so that error in plugin causes activity to be rejected.
    plugin_hook_after("activity_after_receive", activity);

    // This method could also be used to check if actor is banned, instead of checking in each
    // activity handler.
    Ok(())
  }
}

#[derive(Deserialize)]
struct ActivityQuery {
  type_: String,
  id: String,
}

/// Path parameter for ActivityPub actors (persons and communities), which may be qualified with a
/// domain (e.g. `name@domain`) to reference a remote actor.
#[derive(Deserialize, Clone)]
pub(crate) struct ActorPath {
  name: String,
}

impl ActorPath {
  /// Split the actor name into its local name and optional domain.
  pub(crate) fn split_name(&self) -> (&str, Option<&str>) {
    if let Some((n, d)) = self.name.split_once('@') {
      (n, Some(d))
    } else {
      (self.name.as_str(), None)
    }
  }
}

/// Return the ActivityPub json representation of a local activity over HTTP.
async fn get_activity(
  info: web::Path<ActivityQuery>,
  context: Data<LemmyContext>,
) -> LemmyResult<HttpResponse> {
  let hostname = context.protocol_and_hostname();
  let activity_id = Url::parse(&format!(
    "{}/activities/{}/{}",
    hostname, info.type_, info.id
  ))?
  .into();
  let activity = SentActivity::read_from_apub_id(&mut context.pool(), &activity_id).await?;

  let sensitive = activity.sensitive;
  if sensitive {
    Ok(HttpResponse::Forbidden().finish())
  } else {
    Ok(create_http_response(&activity.data, &FEDERATION_CONTEXT)?)
  }
}

/// Ensure that the community is public and not removed/deleted.
fn check_community_fetchable(community: &Community) -> LemmyResult<()> {
  if !community.visibility.can_federate() {
    return Err(LemmyErrorType::NotFound.into());
  }
  Ok(())
}

/// Check if posts or comments in the community are allowed to be fetched
async fn check_community_content_fetchable(
  community: &Community,
  request: &HttpRequest,
  context: &Data<LemmyContext>,
) -> LemmyResult<()> {
  use CommunityVisibility::*;
  match community.visibility {
    Public | Unlisted => Ok(()),
    Private => {
      let signing_actor =
        signing_actor::<SiteOrMultiOrCommunityOrUser>(request, None, context).await?;
      if community.local {
        Ok(
          PendingFollowerView::check_has_followers_from_instance(
            community.id,
            get_instance_id(&signing_actor),
            &mut context.pool(),
          )
          .await?,
        )
      } else if let Some(followers_url) = community.followers_url.clone() {
        let mut followers_url = followers_url.inner().clone();
        context.is_valid_ip(&followers_url).await?;
        followers_url
          .query_pairs_mut()
          .append_pair("is_follower", signing_actor.id().as_str());
        let req = context.client().get(followers_url.as_str());
        let req = context.sign_request(req, Bytes::new()).await?;
        context.client().execute(req).await?.error_for_status()?;
        Ok(())
      } else {
        Err(LemmyErrorType::NotFound.into())
      }
    }
    LocalOnlyPublic | LocalOnlyPrivate => Err(LemmyErrorType::NotFound.into()),
  }
}

pub(in crate::http) fn get_instance_id(s: &SiteOrMultiOrCommunityOrUser) -> InstanceId {
  use Either::*;
  match s {
    Left(Left(s)) => s.instance_id,
    Left(Right(m)) => m.instance_id,
    Right(Left(u)) => u.instance_id,
    Right(Right(c)) => c.instance_id,
  }
}

#[cfg(test)]
mod tests {
  use super::{addresses_only_other_local_site, is_other_local_host};
  use url::Url;

  fn hosts() -> Vec<String> {
    vec!["a.example".to_string(), "b.example".to_string()]
  }

  #[test]
  fn detects_other_local_host() {
    assert!(is_other_local_host(
      &Url::parse("https://b.example/actors/alice").unwrap(),
      "a.example",
      &hosts()
    ));
    assert!(!is_other_local_host(
      &Url::parse("https://a.example/actors/alice").unwrap(),
      "a.example",
      &hosts()
    ));
  }

  #[test]
  fn rejects_activity_only_addressed_to_other_site() {
    let recipients = vec![Url::parse("https://b.example/actors/alice").unwrap()];
    assert!(addresses_only_other_local_site(
      &recipients,
      "a.example",
      &hosts()
    ));

    let recipients = vec![
      Url::parse("https://b.example/actors/alice").unwrap(),
      Url::parse("https://a.example/actors/bob").unwrap(),
    ];
    assert!(!addresses_only_other_local_site(
      &recipients,
      "a.example",
      &hosts()
    ));
  }
}
