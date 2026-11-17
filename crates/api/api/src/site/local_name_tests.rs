use super::oauth_tenant_tests::site_context;
use crate::{
  local_user::{login::login, save_settings::save_user_settings},
  site::admin_change_username::admin_change_username,
};
use actix_web::{test::TestRequest, web::Json};
use lemmy_api_crud::{
  community::{create::create_community, update::edit_community},
  site::update::edit_site,
  user::create::register,
};
use lemmy_api_utils::context::LemmyContext;
use lemmy_db_schema::{
  source::{
    community::Community,
    instance::Instance,
    local_site::{LocalSite, LocalSiteUpdateForm},
    local_site_rate_limit::{LocalSiteRateLimit, LocalSiteRateLimitInsertForm},
    local_user::{LocalUser, LocalUserInsertForm},
    person::{Person, PersonInsertForm},
    site::{Site, SiteUpsertForm},
  },
  test_data::TestData,
};
use lemmy_db_views_community::api::{CreateCommunity, EditCommunity};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_registration_applications::api::Register;
use lemmy_db_views_site::{
  SiteView,
  api::{AdminChangeUsername, EditSite, Login, SaveUserSettings},
};
use lemmy_db_schema_file::enums::RegistrationMode;
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::LemmyResult;
use serial_test::serial;

#[actix_web::test]
#[serial]
async fn non_federated_users_and_moderators_can_use_chinese_actor_names() -> LemmyResult<()> {
  let base = LemmyContext::init_test_context().await;
  let pool = &mut base.pool();
  let data = TestData::create(pool).await?;
  LocalSite::update(
    pool,
    data.local_site.id,
    &LocalSiteUpdateForm {
      federation_enabled: Some(false),
      site_setup: Some(false),
      registration_mode: Some(RegistrationMode::Open),
      email_verification_required: Some(false),
      ..Default::default()
    },
  )
  .await?;

  let admin_person = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(data.local_site.id),
      ..PersonInsertForm::test_form(data.instance.id, "site_admin")
    },
  )
  .await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: data.local_site.id,
      ..LocalUserInsertForm::test_form_admin(admin_person.id)
    },
    vec![],
  )
  .await?;
  let admin = LocalUserView::read_person_for_site(pool, admin_person.id, data.local_site.id).await?;

  let user_person = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(data.local_site.id),
      ..PersonInsertForm::test_form(data.instance.id, "regular_user")
    },
  )
  .await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: data.local_site.id,
      ..LocalUserInsertForm::test_form(user_person.id)
    },
    vec![],
  )
  .await?;
  let user = LocalUserView::read_person_for_site(pool, user_person.id, data.local_site.id).await?;
  let (context, tenant) = site_context(&base, data.local_site.id).await?;

  let registration = register(
    Json(Register {
      username: "中文注册用户".into(),
      password: "valid_password".to_string().into(),
      password_verify: "valid_password".to_string().into(),
      ..Default::default()
    }),
    TestRequest::default().to_http_request(),
    context.clone(),
    tenant.clone(),
  )
  .await?;
  assert!(registration.jwt.is_some());

  for username in ["ひらがな登録", "カタカナ登録", "한국어등록"] {
    let registration = register(
      Json(Register {
        username: username.into(),
        password: "valid_password".to_string().into(),
        password_verify: "valid_password".to_string().into(),
        ..Default::default()
      }),
      TestRequest::default().to_http_request(),
      context.clone(),
      tenant.clone(),
    )
    .await?;
    assert!(registration.jwt.is_some());

    let authenticated = login(
      Json(Login {
        username_or_email: username.into(),
        password: "valid_password".to_string().into(),
        ..Default::default()
      }),
      TestRequest::default().to_http_request(),
      context.clone(),
      tenant.clone(),
    )
    .await?;
    assert!(authenticated.jwt.is_some());
  }

  save_user_settings(
    Json(SaveUserSettings {
      username: Some("ひらがな利用者".into()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await?;
  assert_eq!(Person::read(pool, user_person.id).await?.name, "ひらがな利用者");

  save_user_settings(
    Json(SaveUserSettings {
      username: Some("한국어사용자".into()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await?;
  assert_eq!(Person::read(pool, user_person.id).await?.name, "한국어사용자");

  admin_change_username(
    Json(AdminChangeUsername {
      person_id: user_person.id,
      username: "カタカナ管理改名".into(),
    }),
    context.clone(),
    admin.clone(),
    tenant.clone(),
  )
  .await?;
  assert_eq!(Person::read(pool, user_person.id).await?.name, "カタカナ管理改名");

  assert!(save_user_settings(
    Json(SaveUserSettings {
      username: Some(admin_person.name.clone()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await
  .is_err());
  assert!(save_user_settings(
    Json(SaveUserSettings {
      username: Some("ひらがな改名".into()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await
  .is_err());

  assert!(admin_change_username(
    Json(AdminChangeUsername {
      person_id: user_person.id,
      username: "越权改名".into(),
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await
  .is_err());

  let foreign_instance = Instance::read_or_create(pool, "foreign-name-test.example").await?;
  let foreign_site_record =
    Site::create(pool, &SiteUpsertForm::new("foreign site".into(), foreign_instance.id)).await?;
  let foreign_person = Person::create(
    pool,
    &PersonInsertForm {
      ..PersonInsertForm::test_form(foreign_instance.id, "foreign_system")
    },
  )
  .await?;
  let foreign_local_site = LocalSite::create(
    pool,
    &lemmy_db_schema::source::local_site::LocalSiteInsertForm::new(
      foreign_site_record.id,
      foreign_person.id,
      foreign_instance.domain.clone(),
    ),
  )
  .await?;
  LocalSiteRateLimit::create(
    pool,
    &LocalSiteRateLimitInsertForm::new(foreign_local_site.id),
  )
  .await?;
  let foreign_person = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(foreign_local_site.id),
      ..PersonInsertForm::test_form(foreign_instance.id, "foreign_user")
    },
  )
  .await?;
  assert!(admin_change_username(
    Json(AdminChangeUsername {
      person_id: foreign_person.id,
      username: "跨站改名".into(),
    }),
    context.clone(),
    admin.clone(),
    tenant.clone(),
  )
  .await
  .is_err());

  let response = create_community(
    Json(CreateCommunity {
      name: "中文社区".into(),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await?;
  let community = response.community_view.community.clone();
  let actor_id = community.ap_id.clone();
  edit_community(
    Json(EditCommunity {
      community_id: community.id,
      name: Some("改名社区".into()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await?;
  let renamed = Community::read(pool, community.id).await?;
  assert_eq!(renamed.name, "改名社区");
  assert_eq!(renamed.ap_id, actor_id);
  assert!(create_community(
    Json(CreateCommunity {
      name: "改名社区".into(),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await
  .is_err());

  let result = edit_site(
    Json(EditSite {
      federation_enabled: Some(true),
      ..Default::default()
    }),
    context.clone(),
    admin.clone(),
    tenant.clone(),
  )
  .await;
  let error = match result {
    Err(error) => error,
    Ok(_) => panic!("federation must remain disabled while incompatible names exist"),
  };
  let lemmy_utils::error::LemmyErrorType::Unknown(message) = error.error_type else {
    panic!("expected actionable federation validation error");
  };
  assert!(message.contains("username: カタカナ管理改名"));
  assert!(message.contains("community: 改名社区"));
  assert!(!SiteView::read_by_id(&mut base.pool(), data.local_site.id)
    .await?
    .local_site
    .federation_enabled);

  let registered_person = LocalUserView::read_from_name(pool, "中文注册用户")
    .await?
    .person;
  admin_change_username(
    Json(AdminChangeUsername {
      person_id: registered_person.id,
      username: "registered_user".into(),
    }),
    context.clone(),
    admin.clone(),
    tenant.clone(),
  )
  .await?;
  admin_change_username(
    Json(AdminChangeUsername {
      person_id: user_person.id,
      username: "compatible_user".into(),
    }),
    context.clone(),
    admin.clone(),
    tenant.clone(),
  )
  .await?;
  edit_community(
    Json(EditCommunity {
      community_id: community.id,
      name: Some("compatible_community".into()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await?;
  edit_site(
    Json(EditSite {
      federation_enabled: Some(true),
      ..Default::default()
    }),
    context.clone(),
    admin,
    tenant.clone(),
  )
  .await?;
  assert!(save_user_settings(
    Json(SaveUserSettings {
      username: Some("再次中文改名".into()),
      ..Default::default()
    }),
    context.clone(),
    user.clone(),
    tenant.clone(),
  )
  .await
  .is_err());
  assert!(edit_community(
    Json(EditCommunity {
      community_id: community.id,
      name: Some("再次中文社区名".into()),
      ..Default::default()
    }),
    context,
    user,
    tenant,
  )
  .await
  .is_err());

  Ok(())
}
