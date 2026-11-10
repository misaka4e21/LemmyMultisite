use activitypub_federation::config::Data;
use actix_web::web::{Json, Query};
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext, utils::is_admin};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_registration_applications::{
  RegistrationApplicationView,
  api::ListRegistrationApplications,
  impls::RegistrationApplicationQuery,
};
use lemmy_diesel_utils::pagination::PagedResponse;
use lemmy_utils::error::LemmyResult;

/// Lists registration applications, filterable by undenied only.
pub async fn list_registration_applications(
  Query(data): Query<ListRegistrationApplications>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  tenant: TenantContext,
) -> LemmyResult<Json<PagedResponse<RegistrationApplicationView>>> {
  let local_site = tenant.local_site().clone();

  // Make sure user is an admin
  is_admin(&local_user_view)?;

  let registration_applications = RegistrationApplicationQuery {
    local_site_id: local_site.id,
    unread_only: data.unread_only,
    verified_email_only: Some(local_site.email_verification_required),
    page_cursor: data.page_cursor,
    limit: data.limit,
  }
  .list(&mut context.pool())
  .await?;

  Ok(Json(registration_applications))
}
