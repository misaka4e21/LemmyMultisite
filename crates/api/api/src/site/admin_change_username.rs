use activitypub_federation::config::Data;
use actix_web::web::Json;
use chrono::Utc;
use lemmy_api_utils::{
  context::LemmyContext,
  tenant::TenantContext,
  utils::{is_admin, slur_regex},
};
use lemmy_db_schema::source::{
  local_user::LocalUser,
  person::{Person, PersonUpdateForm},
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::{AdminChangeUsername, SuccessResponse};
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::{
  error::LemmyResult,
  utils::{slurs::check_slurs, validation::is_valid_local_actor_name},
};

pub async fn admin_change_username(
  Json(data): Json<AdminChangeUsername>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  tenant: TenantContext,
) -> LemmyResult<Json<SuccessResponse>> {
  is_admin(&local_user_view)?;
  let local_site = tenant.local_site();
  let target = LocalUserView::read_person_for_site(
    &mut context.pool(),
    data.person_id,
    local_site.id,
  )
  .await?;
  LocalUser::is_higher_admin_check(
    &mut context.pool(),
    local_user_view.person.id,
    vec![data.person_id],
  )
  .await?;

  is_valid_local_actor_name(&data.username, local_site.federation_enabled)?;
  let slur_regex = slur_regex(&context).await?;
  check_slurs(&data.username, &slur_regex)?;
  if target.person.name != data.username {
    Person::check_username_available(
      &mut context.pool(),
      local_site.id,
      &data.username,
      target.person.id,
    )
    .await?;
    Person::update(
      &mut context.pool(),
      target.person.id,
      &PersonUpdateForm {
        name: Some(data.username),
        updated_at: Some(Some(Utc::now())),
        ..Default::default()
      },
    )
    .await?;
  }

  Ok(Json(SuccessResponse::default()))
}
