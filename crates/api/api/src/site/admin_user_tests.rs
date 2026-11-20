#![expect(clippy::indexing_slicing)]

use super::{
  admin_get_user::admin_get_user, admin_list_users::admin_list_users,
  oauth_tenant_tests::site_context,
};
use actix_web::web::{Path, Query};
use lemmy_api_utils::context::LemmyContext;
use lemmy_db_schema::{
  source::{
    instance::{Instance, InstanceActions, InstanceBanForm},
    local_site::{LocalSite, LocalSiteInsertForm},
    local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
    local_user::{LocalUser, LocalUserInsertForm},
    person::{Person, PersonInsertForm},
    site::{Site, SiteUpsertForm},
  },
  traits::Bannable,
  test_data::TestData,
};
use lemmy_db_schema_file::{InstanceId, newtypes::LocalSiteId};
use lemmy_db_views_local_user::{LocalUserView, api::AdminListUsers, impls::LocalUserQuery};
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use serial_test::serial;

async fn create_site_user(
  pool: &mut lemmy_diesel_utils::connection::DbPool<'_>,
  instance_id: InstanceId,
  local_site_id: LocalSiteId,
  name: &str,
  email: &str,
  admin: bool,
) -> LemmyResult<LocalUserView> {
  let person = Person::create(
    pool,
    &PersonInsertForm {
      local: Some(true),
      origin_local_site_id: Some(local_site_id),
      ..PersonInsertForm::test_form(instance_id, name)
    },
  )
  .await?;
  let local_user_form = if admin {
    LocalUserInsertForm {
      local_site_id,
      email: Some(email.to_string()),
      ..LocalUserInsertForm::test_form_admin(person.id)
    }
  } else {
    LocalUserInsertForm {
      local_site_id,
      email: Some(email.to_string()),
      ..LocalUserInsertForm::test_form(person.id)
    }
  };
  LocalUser::create(pool, &local_user_form, vec![]).await?;
  LocalUserView::read_person_for_site(pool, person.id, local_site_id).await
}

async fn create_second_site(
  pool: &mut lemmy_diesel_utils::connection::DbPool<'_>,
) -> LemmyResult<(Instance, LocalSite)> {
  let instance = Instance::read_or_create(pool, "admin-user-second.example").await?;
  let site = Site::create(
    pool,
    &SiteUpsertForm::new("second site".into(), instance.id),
  )
  .await?;
  let system_person = Person::create(
    pool,
    &PersonInsertForm::test_form(instance.id, "admin_user_system"),
  )
  .await?;
  let local_site = LocalSite::create(
    pool,
    &LocalSiteInsertForm::new(site.id, system_person.id, instance.domain.clone()),
  )
  .await?;
  LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(local_site.id)).await?;
  Ok((instance, local_site))
}

#[actix_web::test]
#[serial]
async fn admin_user_search_and_detail_are_site_scoped() -> LemmyResult<()> {
  let base = LemmyContext::init_test_context().await;
  let pool = &mut base.pool();
  let first = TestData::create(pool).await?;
  let first_admin = create_site_user(
    pool,
    first.instance.id,
    first.local_site.id,
    "admin_user_first",
    "admin-first@example.com",
    true,
  )
  .await?;
  let matching_first = create_site_user(
    pool,
    first.instance.id,
    first.local_site.id,
    "needle_first",
    "needle-first@example.com",
    false,
  )
  .await?;
  let matching_second_first_site = create_site_user(
    pool,
    first.instance.id,
    first.local_site.id,
    "needle_second",
    "needle-second@example.com",
    false,
  )
  .await?;
  let non_admin = create_site_user(
    pool,
    first.instance.id,
    first.local_site.id,
    "ordinary_user",
    "ordinary@example.com",
    false,
  )
  .await?;
  let (second_instance, second_local_site) = create_second_site(pool).await?;
  let other_tenant_match = create_site_user(
    pool,
    second_instance.id,
    second_local_site.id,
    "needle_other_tenant",
    "needle-other@example.com",
    false,
  )
  .await?;

  let (context, tenant) = site_context(&base, first.local_site.id).await?;
  let web_context = actix_web::web::Data::new(context.app_data().clone());
  let admin_list = admin_list_users(
    Query(AdminListUsers {
      search_term: Some("NEEDLE".into()),
      limit: Some(1),
      ..Default::default()
    }),
    web_context.clone(),
    first_admin.clone(),
    tenant.clone(),
  )
  .await?
  .0;
  assert_eq!(admin_list.items.len(), 1);
  assert!(admin_list.next_page.is_some());
  assert_ne!(admin_list.items[0].person.id, other_tenant_match.person.id);

  let repeated_first_page = admin_list_users(
    Query(AdminListUsers {
      search_term: Some("NEEDLE".into()),
      limit: Some(1),
      ..Default::default()
    }),
    web_context.clone(),
    first_admin.clone(),
    tenant.clone(),
  )
  .await?
  .0;
  assert_eq!(
    repeated_first_page.items[0].person.id,
    admin_list.items[0].person.id
  );

  let second_page = admin_list_users(
    Query(AdminListUsers {
      search_term: Some("NEEDLE".into()),
      page_cursor: admin_list.next_page,
      limit: Some(1),
      ..Default::default()
    }),
    web_context.clone(),
    first_admin.clone(),
    tenant.clone(),
  )
  .await?
  .0;
  assert_eq!(second_page.items.len(), 1);
  assert_ne!(
    admin_list.items[0].person.id,
    second_page.items[0].person.id
  );
  assert!(
    [
      matching_first.person.id,
      matching_second_first_site.person.id
    ]
    .contains(&admin_list.items[0].person.id)
  );
  assert!(
    [
      matching_first.person.id,
      matching_second_first_site.person.id
    ]
    .contains(&second_page.items[0].person.id)
  );

  InstanceActions::ban(
    pool,
    &InstanceBanForm::new(matching_first.person.id, first.instance.id, None),
  )
  .await?;
  let banned_search = LocalUserQuery {
    local_site_id: first.local_site.id,
    search_term: Some("needle".into()),
    banned_only: Some(true),
    ..Default::default()
  }
  .list(pool)
  .await?;
  assert_eq!(banned_search.items.len(), 1);
  assert_eq!(banned_search.items[0].person.id, matching_first.person.id);

  let detail = admin_get_user(
    Path::from(first_admin.person.id),
    web_context.clone(),
    first_admin.clone(),
    tenant.clone(),
  )
  .await?
  .0;
  assert_eq!(detail.person.id, first_admin.person.id);
  assert!(detail.local_user.admin);

  let non_admin_result = admin_get_user(
    Path::from(matching_first.person.id),
    web_context.clone(),
    non_admin,
    tenant.clone(),
  )
  .await;
  assert!(non_admin_result.is_err_and(|error| error.error_type == LemmyErrorType::NotAnAdmin));

  let cross_site_result = admin_get_user(
    Path::from(other_tenant_match.person.id),
    web_context,
    first_admin,
    tenant,
  )
  .await;
  assert!(cross_site_result.is_err_and(|error| error.error_type == LemmyErrorType::NotFound));

  Instance::delete(pool, second_instance.id).await?;
  first.delete(pool).await
}
