use actix_web::web::{Data, Json, Path};
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext, utils::is_admin};
use lemmy_db_schema_file::PersonId;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_utils::error::LemmyResult;

pub async fn admin_get_user(
  person_id: Path<PersonId>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  tenant: TenantContext,
) -> LemmyResult<Json<LocalUserView>> {
  is_admin(&local_user_view)?;
  let person_id = person_id.into_inner();

  let user = LocalUserView::read_person_for_site(
    &mut context.pool(),
    person_id,
    tenant.local_site().id,
  )
  .await?;

  Ok(Json(user))
}
