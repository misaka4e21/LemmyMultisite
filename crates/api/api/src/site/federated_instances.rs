use actix_web::web::{Data, Json, Query};
use lemmy_api_utils::{
  context::LemmyContext,
  tenant::TenantContext,
  utils::check_private_instance,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::{FederatedInstanceView, SiteView, api::GetFederatedInstances};
use lemmy_diesel_utils::pagination::PagedResponse;
use lemmy_utils::error::LemmyResult;

pub async fn get_federated_instances(
  Query(data): Query<GetFederatedInstances>,
  context: Data<LemmyContext>,
  local_user_view: Option<LocalUserView>,
  tenant: TenantContext,
) -> LemmyResult<Json<PagedResponse<FederatedInstanceView>>> {
  let SiteView { local_site, .. } = tenant.site_view.clone();
  check_private_instance(&local_user_view, &local_site)?;

  let federated_instances =
    FederatedInstanceView::list(&mut context.pool(), data, local_site.id).await?;

  // Return the jwt
  Ok(Json(federated_instances))
}
