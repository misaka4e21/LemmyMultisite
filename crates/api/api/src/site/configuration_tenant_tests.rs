use super::oauth_tenant_tests::site_context;
use crate::{
  local_user::{add_admin::add_admin, ban_person::ban_from_site},
  site::purge::{
    comment::purge_comment,
    community::purge_community,
    person::purge_person,
    post::purge_post,
  },
};
use actix_web::{
  App,
  HttpMessage,
  HttpResponse,
  dev::Service,
  test,
  web::{self, Json, Query},
};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use lemmy_api_crud::{
  custom_emoji::{
    create::create_custom_emoji,
    delete::delete_custom_emoji,
    list::list_custom_emojis,
    update::edit_custom_emoji,
  },
  oauth_provider::{
    create::create_oauth_provider,
    delete::delete_oauth_provider,
    update::edit_oauth_provider,
  },
  site::{
    create::create_site,
    read::{get_site, invalidate_site_response},
    update::edit_site,
  },
  tagline::{
    create::create_tagline,
    delete::delete_tagline,
    list::list_taglines,
    update::edit_tagline,
  },
  user::create::register,
};
use lemmy_api_utils::{
  context::LemmyContext,
  send_activity::ActivityChannel,
  utils::local_site_rate_limit_to_rate_limit_config,
};
use lemmy_db_schema::{
  impls::presence::{
    ensure_comment_site,
    ensure_community_site,
    ensure_person_site,
    ensure_post_site,
  },
  source::{
    comment::{Comment, CommentInsertForm},
    community::{Community, CommunityInsertForm},
    instance::{Instance, InstanceActions},
    local_site::{LocalSite, LocalSiteInsertForm, LocalSiteUpdateForm},
    local_site_rate_limit::{
      LocalSiteRateLimit,
      LocalSiteRateLimitInsertForm,
      LocalSiteRateLimitUpdateForm,
    },
    local_user::{LocalUser, LocalUserInsertForm},
    person::{Person, PersonInsertForm},
    post::{Post, PostInsertForm},
    site::{Site, SiteUpsertForm},
  },
  test_data::TestData,
};
use lemmy_db_schema_file::{
  enums::RegistrationMode,
  newtypes::LocalSiteId,
  schema::{modlog, person},
};
use lemmy_db_views_comment::api::PurgeComment;
use lemmy_db_views_community::api::PurgeCommunity;
use lemmy_db_views_custom_emoji::api::{
  CreateCustomEmoji,
  DeleteCustomEmoji,
  EditCustomEmoji,
  ListCustomEmojis,
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_person::api::{AddAdmin, BanPerson, PurgePerson};
use lemmy_db_views_post::api::PurgePost;
use lemmy_db_views_registration_applications::api::Register;
use lemmy_db_views_site::api::{
  CreateOAuthProvider,
  CreateSite,
  CreateTagline,
  DeleteOAuthProvider,
  DeleteTagline,
  EditOAuthProvider,
  EditSite,
  EditTagline,
  ListTaglines,
};
use lemmy_diesel_utils::{connection::get_conn, traits::Crud};
use lemmy_utils::error::{LemmyErrorType, LemmyResult};
use serial_test::serial;
use url::Url;

async fn runtime_post_limit(context: &LemmyContext, site_id: LocalSiteId) -> u32 {
  let app = test::init_service(
    App::new()
      .route("/", web::get().to(|| async { HttpResponse::Ok() }))
      .wrap(context.rate_limit_cell().post())
      .wrap_fn(move |request, service| {
        request.extensions_mut().insert(site_id);
        service.call(request)
      }),
  )
  .await;
  let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
  response
    .headers()
    .get("x-ratelimit-limit")
    .unwrap()
    .to_str()
    .unwrap()
    .parse()
    .unwrap()
}

#[tokio::test]
#[serial]
async fn site_settings_updates_are_scoped_to_the_current_tenant() -> LemmyResult<()> {
  let base = LemmyContext::init_test_context().await;
  let pool = &mut base.pool();
  let first = TestData::create(pool).await?;
  let first_user = LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: first.local_site.id,
      email_verified: Some(false),
      accepted_application: Some(false),
      ..LocalUserInsertForm::test_form_admin(first.person.id)
    },
    vec![],
  )
  .await?;
  let admin = LocalUserView::read_person(pool, first.person.id).await?;
  let instance = Instance::read_or_create(pool, "settings-isolation.example").await?;
  let site = Site::create(pool, &SiteUpsertForm::new("other".into(), instance.id)).await?;
  let person = Person::create(pool, &PersonInsertForm::test_form(instance.id, "other")).await?;
  let second = LocalSite::create(
    pool,
    &LocalSiteInsertForm::new(site.id, person.id, "settings-isolation.example".into()),
  )
  .await?;
  LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(second.id)).await?;
  let second_user = LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: second.id,
      email_verified: Some(false),
      accepted_application: Some(false),
      ..LocalUserInsertForm::test_form_admin(person.id)
    },
    vec![],
  )
  .await?;
  LocalSite::update(
    pool,
    first.local_site.id,
    &LocalSiteUpdateForm {
      site_setup: Some(false),
      email_verification_required: Some(false),
      registration_mode: Some(RegistrationMode::Open),
      ..Default::default()
    },
  )
  .await?;
  let (_, second_tenant) = site_context(&base, second.id).await?;
  let second_limits = second_tenant.site_view.local_site_rate_limit.clone();
  base.rate_limit_cell().set_config_for_site(
    second.id,
    local_site_rate_limit_to_rate_limit_config(&second_limits),
  );
  let (context, tenant) = site_context(&base, first.local_site.id).await?;
  let web_context = web::Data::new(context.app_data().clone());
  get_site(None, web_context.clone(), tenant.clone()).await?;
  get_site(None, web_context.clone(), second_tenant.clone()).await?;
  let response = create_site(
    Json(CreateSite {
      name: "First site".into(),
      rate_limit_post_max_requests: Some(12),
      ..Default::default()
    }),
    context.clone(),
    admin.clone(),
    tenant,
  )
  .await?;
  assert_eq!(
    response.site_view.local_site_rate_limit.post_max_requests,
    12
  );
  assert_eq!(runtime_post_limit(&context, first.local_site.id).await, 12);
  assert_eq!(
    runtime_post_limit(&context, second.id).await,
    second_limits.post_max_requests as u32
  );
  let (_, refreshed_tenant) = site_context(&base, first.local_site.id).await?;
  assert_eq!(
    get_site(None, web_context.clone(), refreshed_tenant)
      .await?
      .site_view
      .local_site_rate_limit
      .post_max_requests,
    12
  );
  assert_eq!(
    lemmy_db_views_site::SiteView::read_by_hostname(pool, "settings-isolation.example")
      .await?
      .local_site_rate_limit,
    second_limits
  );

  for form in [
    LocalSiteRateLimitUpdateForm {
      import_user_settings_max_requests: Some(7),
      ..Default::default()
    },
    LocalSiteRateLimitUpdateForm {
      import_user_settings_interval_seconds: Some(99),
      ..Default::default()
    },
  ] {
    LocalSiteRateLimit::update(pool, first.local_site.id, &form).await?;
  }
  let conn = &mut get_conn(pool).await?;
  let limits: LocalSiteRateLimit = lemmy_db_schema_file::schema::local_site_rate_limit::table
    .find(first.local_site.id)
    .first(conn)
    .await?;
  assert_eq!(limits.import_user_settings_max_requests, 7);
  assert_eq!(limits.import_user_settings_interval_seconds, 99);

  let (context, tenant) = site_context(&base, first.local_site.id).await?;
  let response = edit_site(
    Json(EditSite {
      rate_limit_post_max_requests: Some(13),
      email_verification_required: Some(true),
      registration_mode: Some(RegistrationMode::RequireApplication),
      application_question: Some("Why join?".into()),
      ..Default::default()
    }),
    context.clone(),
    admin.clone(),
    tenant,
  )
  .await?;
  assert_eq!(
    response.site_view.local_site_rate_limit.post_max_requests,
    13
  );
  assert_eq!(runtime_post_limit(&context, first.local_site.id).await, 13);
  assert_eq!(
    runtime_post_limit(&context, second.id).await,
    second_limits.post_max_requests as u32
  );
  let (_, refreshed_tenant) = site_context(&base, first.local_site.id).await?;
  assert_eq!(
    get_site(None, web_context.clone(), refreshed_tenant)
      .await?
      .site_view
      .local_site_rate_limit
      .post_max_requests,
    13
  );
  assert_eq!(
    get_site(None, web_context, second_tenant)
      .await?
      .site_view
      .local_site_rate_limit,
    second_limits
  );
  assert_eq!(
    lemmy_db_views_site::SiteView::read_by_hostname(pool, "settings-isolation.example")
      .await?
      .local_site_rate_limit,
    second_limits
  );
  let first_after = LocalUserView::read_person(pool, first.person.id)
    .await?
    .local_user;
  let second_after = LocalUserView::read_person(pool, person.id)
    .await?
    .local_user;
  assert_eq!(first_after.id, first_user.id);
  assert!(first_after.email_verified && first_after.accepted_application);
  assert_eq!(second_after.id, second_user.id);
  assert!(!second_after.email_verified && !second_after.accepted_application);

  let new_person = Person::create(
    pool,
    &PersonInsertForm::test_form(first.instance.id, "new_pending"),
  )
  .await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: first.local_site.id,
      email_verified: Some(false),
      accepted_application: Some(false),
      ..LocalUserInsertForm::test_form_admin(new_person.id)
    },
    vec![],
  )
  .await?;
  let (context, tenant) = site_context(&base, first.local_site.id).await?;
  edit_site(
    Json(EditSite::default()),
    context.clone(),
    admin.clone(),
    tenant.clone(),
  )
  .await?;
  let pending = LocalUserView::read_person(pool, new_person.id)
    .await?
    .local_user;
  assert!(!pending.email_verified && !pending.accepted_application);
  let missing_email = Register {
    username: "newuser".into(),
    password: "valid_password".to_string().into(),
    password_verify: "valid_password".to_string().into(),
    answer: Some("answer".into()),
    ..Default::default()
  };
  assert!(matches!(
    register(
      Json(missing_email),
      actix_web::test::TestRequest::default().to_http_request(),
      context.clone(),
      tenant.clone()
    )
    .await, Err(error) if matches!(error.error_type, LemmyErrorType::EmailRequired)
  ));
  let missing_answer = Register {
    username: "newuser".into(),
    password: "valid_password".to_string().into(),
    password_verify: "valid_password".to_string().into(),
    email: Some("new@example.com".to_string().into()),
    ..Default::default()
  };
  assert!(matches!(
    register(
      Json(missing_answer),
      actix_web::test::TestRequest::default().to_http_request(),
      context.clone(),
      tenant.clone()
    )
    .await, Err(error) if matches!(error.error_type, LemmyErrorType::RegistrationApplicationAnswerRequired)
  ));
  let registered = register(
    Json(Register {
      username: "newuser".into(),
      password: "valid_password".to_string().into(),
      password_verify: "valid_password".to_string().into(),
      email: Some("new@example.com".to_string().into()),
      answer: Some("answer".into()),
      ..Default::default()
    }),
    actix_web::test::TestRequest::default().to_http_request(),
    context.clone(),
    tenant.clone(),
  )
  .await?;
  assert!(registered.registration_created);
  assert!(registered.verify_email_sent);
  let registered_user = LocalUserView::read_from_name(pool, "newuser")
    .await?
    .local_user;
  assert!(!registered_user.email_verified && !registered_user.accepted_application);

  let conn = &mut get_conn(pool).await?;
  diesel::delete(
    lemmy_db_schema_file::schema::local_site_rate_limit::table.find(first.local_site.id),
  )
  .execute(conn)
  .await?;
  assert!(
    edit_site(
      Json(EditSite {
        rate_limit_post_max_requests: Some(14),
        ..Default::default()
      }),
      context,
      admin,
      tenant
    )
    .await
    .is_err()
  );
  assert_eq!(
    lemmy_db_views_site::SiteView::read_by_hostname(pool, "settings-isolation.example")
      .await?
      .local_site_rate_limit,
    second_limits
  );

  Instance::delete(pool, instance.id).await?;
  first.delete(pool).await
}

#[tokio::test]
#[serial]
async fn site_configuration_is_isolated_and_cache_is_invalidated() -> LemmyResult<()> {
  let base = LemmyContext::init_test_context().await;
  let pool = &mut base.pool();
  let first = TestData::create(pool).await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: first.local_site.id,
      ..LocalUserInsertForm::test_form_admin(first.person.id)
    },
    vec![],
  )
  .await?;
  let first_admin = LocalUserView::read_person(pool, first.person.id).await?;

  let second_instance = Instance::read_or_create(pool, "config-second.example").await?;
  let second_site = Site::create(
    pool,
    &SiteUpsertForm::new("second".into(), second_instance.id),
  )
  .await?;
  let second_person = Person::create(
    pool,
    &PersonInsertForm::test_form(second_instance.id, "admin"),
  )
  .await?;
  let second = LocalSite::create(
    pool,
    &LocalSiteInsertForm::new(
      second_site.id,
      second_person.id,
      "config-second.example".into(),
    ),
  )
  .await?;
  LocalSiteRateLimit::create(pool, &LocalSiteRateLimitInsertForm::new(second.id)).await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: second.id,
      ..LocalUserInsertForm::test_form_admin(second_person.id)
    },
    vec![],
  )
  .await?;
  let second_admin = LocalUserView::read_person(pool, second_person.id).await?;
  let (first_context, first_tenant) = site_context(&base, first.local_site.id).await?;
  let (second_context, second_tenant) = site_context(&base, second.id).await?;
  let first_web = web::Data::new(first_context.app_data().clone());
  let second_web = web::Data::new(second_context.app_data().clone());

  let unknown_target = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(second.id),
      ..PersonInsertForm::test_form(second_instance.id, "unknown_to_first")
    },
  )
  .await?;
  let other_author = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(second.id),
      ..PersonInsertForm::test_form(second_instance.id, "other_author")
    },
  )
  .await?;
  let other_community = Community::create(
    pool,
    &CommunityInsertForm {
      origin_local_site_id: Some(second.id),
      ..CommunityInsertForm::new(second_instance.id, "purge_target".into(), "key".into())
    },
  )
  .await?;
  let other_post = Post::create(
    pool,
    &PostInsertForm {
      origin_local_site_id: Some(second.id),
      ..PostInsertForm::new("purge target".into(), other_author.id, other_community.id)
    },
  )
  .await?;
  let other_comment = Comment::create(
    pool,
    &CommentInsertForm {
      origin_local_site_id: Some(second.id),
      ..CommentInsertForm::new(
        other_author.id,
        other_post.id,
        other_community.id,
        "purge target".into(),
      )
    },
    None,
  )
  .await?;
  ensure_person_site(pool, first.local_site.id, other_author.id).await?;
  ensure_community_site(pool, first.local_site.id, other_community.id).await?;
  ensure_post_site(pool, first.local_site.id, other_post.id).await?;
  ensure_comment_site(pool, first.local_site.id, other_comment.id).await?;
  while matches!(
    tokio::time::timeout(
      std::time::Duration::from_millis(10),
      ActivityChannel::retrieve_activity(),
    )
    .await,
    Ok(Some(_))
  ) {}
  assert!(
    purge_post(
      Json(PurgePost {
        post_id: other_post.id,
        reason: "test".into()
      }),
      first_context.clone(),
      second_admin.clone(),
    )
    .await
    .is_err()
  );
  let conn = &mut get_conn(pool).await?;
  let initial_modlog_count = modlog::table.count().get_result::<i64>(conn).await?;
  assert!(
    purge_person(
      Json(PurgePerson {
        person_id: other_author.id,
        reason: "test".into()
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  assert!(
    purge_community(
      Json(PurgeCommunity {
        community_id: other_community.id,
        reason: "test".into()
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  assert!(
    purge_post(
      Json(PurgePost {
        post_id: other_post.id,
        reason: "test".into()
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  assert!(
    purge_comment(
      Json(PurgeComment {
        comment_id: other_comment.id,
        reason: "test".into()
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  Person::read(pool, other_author.id).await?;
  Community::read(pool, other_community.id).await?;
  Post::read(pool, other_post.id).await?;
  Comment::read(pool, other_comment.id).await?;
  let conn = &mut get_conn(pool).await?;
  assert_eq!(
    modlog::table.count().get_result::<i64>(conn).await?,
    initial_modlog_count
  );
  assert!(
    tokio::time::timeout(
      std::time::Duration::from_millis(50),
      ActivityChannel::retrieve_activity(),
    )
    .await
    .is_err()
  );

  let own_author = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(first.local_site.id),
      ..PersonInsertForm::test_form(first.instance.id, "purge_own_author")
    },
  )
  .await?;
  let own_community = Community::create(
    pool,
    &CommunityInsertForm {
      origin_local_site_id: Some(first.local_site.id),
      ..CommunityInsertForm::new(
        first.instance.id,
        "purge_own_community".into(),
        "key".into(),
      )
    },
  )
  .await?;
  let own_post = Post::create(
    pool,
    &PostInsertForm {
      origin_local_site_id: Some(first.local_site.id),
      ..PostInsertForm::new("own post".into(), own_author.id, own_community.id)
    },
  )
  .await?;
  let own_comment = Comment::create(
    pool,
    &CommentInsertForm {
      origin_local_site_id: Some(first.local_site.id),
      ..CommentInsertForm::new(
        own_author.id,
        own_post.id,
        own_community.id,
        "own comment".into(),
      )
    },
    None,
  )
  .await?;
  purge_comment(
    Json(PurgeComment {
      comment_id: own_comment.id,
      reason: "test".into(),
    }),
    first_context.clone(),
    first_admin.clone(),
  )
  .await
  .expect("same-site comment purge failed");
  assert!(Comment::read(pool, own_comment.id).await.is_err());
  purge_post(
    Json(PurgePost {
      post_id: own_post.id,
      reason: "test".into(),
    }),
    first_context.clone(),
    first_admin.clone(),
  )
  .await
  .expect("same-site post purge failed");
  assert!(Post::read(pool, own_post.id).await.is_err());
  purge_community(
    Json(PurgeCommunity {
      community_id: own_community.id,
      reason: "test".into(),
    }),
    first_context.clone(),
    first_admin.clone(),
  )
  .await
  .expect("same-site community purge failed");
  assert!(Community::read(pool, own_community.id).await.is_err());
  purge_person(
    Json(PurgePerson {
      person_id: own_author.id,
      reason: "test".into(),
    }),
    first_context.clone(),
    first_admin.clone(),
  )
  .await
  .expect("same-site person purge failed");
  assert!(
    InstanceActions::check_ban(pool, own_author.id, first.instance.id)
      .await
      .is_err()
  );
  tokio::time::timeout(std::time::Duration::from_secs(10), async {
    loop {
      let conn = &mut get_conn(pool).await?;
      let deleted = person::table
        .find(own_author.id)
        .select(person::deleted)
        .first::<bool>(conn)
        .await?;
      if deleted {
        break Ok::<(), lemmy_utils::error::LemmyError>(());
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await
  .expect("purge task did not finish")?;
  for ban in [true, false] {
    assert!(
      ban_from_site(
        Json(BanPerson {
          person_id: unknown_target.id,
          ban,
          remove_or_restore_data: Some(true),
          reason: "test".into(),
          expires_at: None,
        }),
        first_context.clone(),
        first_admin.clone(),
      )
      .await
      .is_err()
    );
  }
  assert!(
    ban_from_site(
      Json(BanPerson {
        person_id: unknown_target.id,
        ban: true,
        remove_or_restore_data: Some(false),
        reason: "test".into(),
        expires_at: None,
      }),
      first_context.clone(),
      second_admin.clone(),
    )
    .await
    .is_err()
  );
  InstanceActions::check_ban(pool, unknown_target.id, first.instance.id).await?;
  InstanceActions::check_ban(pool, unknown_target.id, second_instance.id).await?;
  let conn = &mut get_conn(pool).await?;
  let actions = modlog::table
    .filter(modlog::target_person_id.eq(Some(unknown_target.id)))
    .count()
    .get_result::<i64>(conn)
    .await?;
  assert_eq!(actions, 0);

  for added in [true, false] {
    assert!(
      add_admin(
        Json(AddAdmin {
          person_id: second_person.id,
          added
        }),
        first_web.clone(),
        first_admin.clone()
      )
      .await
      .is_err()
    );
  }
  assert!(
    LocalUserView::read_person(pool, second_person.id)
      .await?
      .local_user
      .admin
  );
  let member = Person::create(
    pool,
    &PersonInsertForm {
      origin_local_site_id: Some(first.local_site.id),
      ..PersonInsertForm::test_form(first.instance.id, "new-admin")
    },
  )
  .await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm {
      local_site_id: first.local_site.id,
      ..LocalUserInsertForm::test_form(member.id)
    },
    vec![],
  )
  .await?;
  for ban in [true, false] {
    ban_from_site(
      Json(BanPerson {
        person_id: member.id,
        ban,
        remove_or_restore_data: Some(false),
        reason: "test".into(),
        expires_at: None,
      }),
      first_context.clone(),
      first_admin.clone(),
    )
    .await?;
    assert_eq!(
      InstanceActions::check_ban(pool, member.id, first.instance.id)
        .await
        .is_err(),
      ban
    );
  }
  add_admin(
    Json(AddAdmin {
      person_id: member.id,
      added: true,
    }),
    first_web.clone(),
    first_admin.clone(),
  )
  .await?;
  let lower_admin = LocalUserView::read_person(pool, member.id).await?;
  assert!(lower_admin.local_user.admin);
  assert!(
    add_admin(
      Json(AddAdmin {
        person_id: first.person.id,
        added: false
      }),
      first_web.clone(),
      lower_admin
    )
    .await
    .is_err()
  );
  add_admin(
    Json(AddAdmin {
      person_id: member.id,
      added: false,
    }),
    first_web.clone(),
    first_admin.clone(),
  )
  .await?;
  assert!(
    !LocalUserView::read_person(pool, member.id)
      .await?
      .local_user
      .admin
  );
  invalidate_site_response(first.local_site.id).await;
  invalidate_site_response(second.id).await;
  assert!(
    get_site(None, first_web.clone(), first_tenant.clone())
      .await?
      .tagline
      .is_none()
  );
  assert!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .tagline
      .is_none()
  );

  let provider = CreateOAuthProvider {
    display_name: "provider".into(),
    issuer: "https://identity.example".into(),
    authorization_endpoint: "https://identity.example/authorize".into(),
    token_endpoint: "https://identity.example/token".into(),
    userinfo_endpoint: "https://identity.example/userinfo".into(),
    id_claim: "sub".into(),
    client_id: "same-client".into(),
    client_secret: "secret".into(),
    scopes: "openid".into(),
    auto_verify_email: None,
    account_linking_enabled: None,
    use_pkce: None,
    enabled: Some(true),
  };
  let first_provider = create_oauth_provider(
    Json(provider.clone()),
    first_context.clone(),
    first_admin.clone(),
  )
  .await?;
  assert_eq!(
    get_site(None, first_web.clone(), first_tenant.clone())
      .await?
      .oauth_providers
      .len(),
    1
  );
  assert!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .oauth_providers
      .is_empty()
  );
  let second_provider =
    create_oauth_provider(Json(provider), second_context.clone(), second_admin.clone()).await?;
  let update = EditOAuthProvider {
    id: second_provider.id,
    display_name: Some("changed".into()),
    authorization_endpoint: None,
    token_endpoint: None,
    userinfo_endpoint: None,
    id_claim: None,
    client_secret: None,
    scopes: None,
    auto_verify_email: None,
    account_linking_enabled: None,
    use_pkce: None,
    enabled: None,
  };
  assert!(
    edit_oauth_provider(
      Json(update.clone()),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  assert!(
    delete_oauth_provider(
      Json(DeleteOAuthProvider {
        id: second_provider.id
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  assert_eq!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .oauth_providers[0]
      .id,
    second_provider.id
  );
  edit_oauth_provider(Json(update), second_context.clone(), second_admin.clone()).await?;
  assert_eq!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .oauth_providers[0]
      .display_name,
    "changed"
  );
  delete_oauth_provider(
    Json(DeleteOAuthProvider {
      id: second_provider.id,
    }),
    second_context.clone(),
    second_admin.clone(),
  )
  .await?;
  assert!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .oauth_providers
      .is_empty()
  );
  assert_eq!(
    get_site(None, first_web.clone(), first_tenant.clone())
      .await?
      .oauth_providers[0]
      .id,
    first_provider.id
  );

  let first_tagline = create_tagline(
    Json(CreateTagline {
      content: "first tagline".into(),
    }),
    first_context.clone(),
    first_admin.clone(),
    first_tenant.clone(),
  )
  .await?
  .tagline
  .clone();
  assert_eq!(
    get_site(None, first_web.clone(), first_tenant.clone())
      .await?
      .tagline
      .as_ref()
      .map(|v| v.id),
    Some(first_tagline.id)
  );
  assert!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .tagline
      .is_none()
  );
  let second_tagline = create_tagline(
    Json(CreateTagline {
      content: "second tagline".into(),
    }),
    second_context.clone(),
    second_admin.clone(),
    second_tenant.clone(),
  )
  .await?
  .tagline
  .clone();
  assert!(
    delete_tagline(
      Json(DeleteTagline {
        id: second_tagline.id
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  let listed_taglines = list_taglines(
    Query(ListTaglines::default()),
    first_web.clone(),
    first_tenant.clone(),
  )
  .await?;
  assert_eq!(listed_taglines.items.len(), 1);
  assert_eq!(listed_taglines.items[0].id, first_tagline.id);
  assert!(
    edit_tagline(
      Json(EditTagline {
        id: second_tagline.id,
        content: "wrong".into()
      }),
      first_context.clone(),
      first_admin.clone(),
      first_tenant.clone()
    )
    .await
    .is_err()
  );
  edit_tagline(
    Json(EditTagline {
      id: second_tagline.id,
      content: "updated".into(),
    }),
    second_context.clone(),
    second_admin.clone(),
    second_tenant.clone(),
  )
  .await?;
  assert_eq!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .tagline
      .as_ref()
      .map(|v| v.content.as_str()),
    Some("updated")
  );
  delete_tagline(
    Json(DeleteTagline {
      id: second_tagline.id,
    }),
    second_context.clone(),
    second_admin.clone(),
  )
  .await?;
  assert!(
    get_site(None, second_web.clone(), second_tenant.clone())
      .await?
      .tagline
      .is_none()
  );

  let emoji = CreateCustomEmoji {
    category: "test".into(),
    shortcode: "same".into(),
    image_url: Url::parse("https://example.com/emoji.png")?.into(),
    alt_text: "same".into(),
    keywords: vec!["keyword".into()],
  };
  let first_emoji = create_custom_emoji(
    Json(emoji.clone()),
    first_context.clone(),
    first_admin.clone(),
  )
  .await?
  .custom_emoji
  .custom_emoji
  .clone();
  let second_emoji = create_custom_emoji(Json(emoji), second_context.clone(), second_admin.clone())
    .await?
    .custom_emoji
    .custom_emoji
    .clone();
  assert!(
    delete_custom_emoji(
      Json(DeleteCustomEmoji {
        id: second_emoji.id
      }),
      first_context.clone(),
      first_admin.clone()
    )
    .await
    .is_err()
  );
  assert!(
    edit_custom_emoji(
      Json(EditCustomEmoji {
        id: second_emoji.id,
        category: None,
        shortcode: None,
        image_url: None,
        alt_text: Some("wrong".into()),
        keywords: Some(vec!["wrong".into()])
      }),
      first_context,
      first_admin
    )
    .await
    .is_err()
  );
  let listed =
    list_custom_emojis(Query(ListCustomEmojis::default()), first_web, first_tenant).await?;
  assert_eq!(listed.custom_emojis.len(), 1);
  assert_eq!(listed.custom_emojis[0].custom_emoji.id, first_emoji.id);
  let listed_second = list_custom_emojis(
    Query(ListCustomEmojis::default()),
    second_web.clone(),
    second_tenant.clone(),
  )
  .await?;
  assert_eq!(listed_second.custom_emojis.len(), 1);
  assert_eq!(
    listed_second.custom_emojis[0].keywords[0].keyword,
    "keyword"
  );
  edit_custom_emoji(
    Json(EditCustomEmoji {
      id: second_emoji.id,
      category: None,
      shortcode: None,
      image_url: None,
      alt_text: Some("updated".into()),
      keywords: Some(vec!["updated".into()]),
    }),
    second_context.clone(),
    second_admin.clone(),
  )
  .await?;
  let updated = list_custom_emojis(
    Query(ListCustomEmojis::default()),
    second_web.clone(),
    second_tenant.clone(),
  )
  .await?;
  assert_eq!(updated.custom_emojis[0].keywords[0].keyword, "updated");
  delete_custom_emoji(
    Json(DeleteCustomEmoji {
      id: second_emoji.id,
    }),
    second_context,
    second_admin,
  )
  .await?;
  assert!(
    list_custom_emojis(
      Query(ListCustomEmojis::default()),
      second_web,
      second_tenant
    )
    .await?
    .custom_emojis
    .is_empty()
  );

  Instance::delete(pool, second_instance.id).await?;
  first.delete(pool).await
}
