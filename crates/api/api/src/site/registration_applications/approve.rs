use activitypub_federation::config::Data;
use actix_web::web::Json;
use chrono::Utc;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_api_utils::{context::LemmyContext, utils::is_admin};
use lemmy_db_schema::source::{
  local_user::{LocalUser, LocalUserUpdateForm},
  registration_application::{RegistrationApplication, RegistrationApplicationUpdateForm},
};
use lemmy_db_schema_file::{
  newtypes::LocalUserId,
  schema::{local_user, registration_application},
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_registration_applications::{
  RegistrationApplicationView,
  api::{ApproveRegistrationApplication, RegistrationApplicationResponse},
};
use lemmy_diesel_utils::{connection::get_conn, utils::diesel_string_update};
use lemmy_email::account::{send_application_approved_email, send_application_denied_email};
use lemmy_utils::error::{LemmyError, LemmyErrorType, LemmyResult};

pub async fn approve_registration_application(
  data: Json<ApproveRegistrationApplication>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<RegistrationApplicationResponse>> {
  approve_registration_application_with_notifier(
    data,
    context,
    local_user_view,
    |user, data, context| {
      if data.approve {
        send_application_approved_email(user, &context.site_view()?.local_site, context.settings())
      } else {
        send_application_denied_email(
          user,
          data.deny_reason.clone(),
          &context.site_view()?.local_site,
          context.settings(),
        )
      }
    },
  )
  .await
}

pub(crate) async fn approve_registration_application_with_notifier<F>(
  Json(data): Json<ApproveRegistrationApplication>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
  notify: F,
) -> LemmyResult<Json<RegistrationApplicationResponse>>
where
  F: FnOnce(&LocalUserView, &ApproveRegistrationApplication, &LemmyContext) -> LemmyResult<()>,
{
  let app_id = data.id;

  // Only let admins do this
  is_admin(&local_user_view)?;
  let site_id = context.site_view()?.local_site.id;
  if local_user_view.local_user.local_site_id != site_id {
    return Err(LemmyErrorType::NotFound.into());
  }

  let pool = &mut context.pool();
  let conn = &mut get_conn(pool).await?;
  let tx_data = data.clone();
  let approved_user_id = conn
    .transaction::<LocalUserId, LemmyError, _>(async |conn| {
      let application = registration_application::table
        .inner_join(local_user::table)
        .filter(registration_application::id.eq(app_id))
        .filter(local_user::local_site_id.eq(site_id))
        .select(registration_application::all_columns)
        .for_update()
        .first::<RegistrationApplication>(conn)
        .await?;

      // Update the registration with reason, admin_id
      let deny_reason = diesel_string_update(tx_data.deny_reason.as_deref());
      let app_form = RegistrationApplicationUpdateForm {
        admin_id: Some(Some(local_user_view.person.id)),
        deny_reason,
        updated_at: Some(Some(Utc::now())),
      };

      diesel::update(
        registration_application::table
          .find(app_id)
          .filter(registration_application::local_user_id.eq(application.local_user_id)),
      )
      .set(&app_form)
      .execute(conn)
      .await?;

      // Update the local_user row
      let local_user_form = LocalUserUpdateForm {
        accepted_application: Some(tx_data.approve),
        ..Default::default()
      };

      let approved_user_id = application.local_user_id;
      diesel::update(
        local_user::table
          .find(approved_user_id)
          .filter(local_user::local_site_id.eq(site_id)),
      )
      .set(&local_user_form)
      .get_result::<LocalUser>(conn)
      .await?;

      Ok(approved_user_id)
    })
    .await?;

  let approved_local_user_view = LocalUserView::read(&mut context.pool(), approved_user_id).await?;
  if approved_local_user_view.local_user.email.is_some() {
    // Email sending may fail, but this won't revert the application approval
    notify(&approved_local_user_view, &data, &context)?;
  }

  // Read the view
  let registration_application = RegistrationApplicationView::read(
    &mut context.pool(),
    local_user_view.local_user.local_site_id,
    app_id,
  )
  .await?;

  Ok(Json(RegistrationApplicationResponse {
    registration_application,
  }))
}
