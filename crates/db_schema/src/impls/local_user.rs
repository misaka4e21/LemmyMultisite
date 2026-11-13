use crate::source::{
  actor_language::LocalUserLanguage,
  local_user::{LocalUser, LocalUserInsertForm, LocalUserUpdateForm},
  site::Site,
};
use bcrypt::{DEFAULT_COST, hash};
use diesel::{
  CombineDsl,
  ExpressionMethods,
  JoinOnDsl,
  NullableExpressionMethods,
  OptionalExtension,
  QueryDsl,
  dsl::{IntervalDsl, insert_into, not},
  result::Error,
};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{CommunityId, LanguageId, LocalSiteId, LocalUserId},
  schema::{community, community_actions, local_user, person, registration_application},
};
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  dburl::DbUrl,
  utils::{
    functions::{coalesce, lower},
    now,
  },
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl LocalUser {
  pub async fn create(
    pool: &mut DbPool<'_>,
    form: &LocalUserInsertForm,
    languages: Vec<LanguageId>,
  ) -> LemmyResult<LocalUser> {
    let conn = &mut get_conn(pool).await?;
    let mut form_with_encrypted_password = form.clone();

    if let Some(password_encrypted) = &form.password_encrypted {
      let password_hash = hash(password_encrypted, DEFAULT_COST)?;
      form_with_encrypted_password.password_encrypted = Some(password_hash);
    }

    let local_user_ = insert_into(local_user::table)
      .values(form_with_encrypted_password)
      .get_result::<Self>(conn)
      .await?;

    LocalUserLanguage::update(pool, languages, local_user_.id).await?;

    Ok(local_user_)
  }

  pub async fn update(
    pool: &mut DbPool<'_>,
    local_user_id: LocalUserId,
    form: &LocalUserUpdateForm,
  ) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    let res = diesel::update(local_user::table.find(local_user_id))
      .set(form)
      .execute(conn)
      .await;
    // Diesel will throw an error if the query is all Nones (not updating anything), ignore this.
    match res {
      Err(Error::QueryBuilderError(_)) => Ok(0),
      other => other,
    }
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn update_for_site(
    pool: &mut DbPool<'_>,
    local_user_id: LocalUserId,
    local_site_id: LocalSiteId,
    form: &LocalUserUpdateForm,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(
      local_user::table
        .filter(local_user::id.eq(local_user_id))
        .filter(local_user::local_site_id.eq(local_site_id)),
    )
    .set(form)
    .get_result(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn delete(pool: &mut DbPool<'_>, id: LocalUserId) -> LemmyResult<usize> {
    let conn = &mut *get_conn(pool).await?;
    diesel::delete(local_user::table.find(id))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  pub async fn update_password(
    pool: &mut DbPool<'_>,
    local_user_id: LocalUserId,
    new_password: &str,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    let password_hash = hash(new_password, DEFAULT_COST)?;

    diesel::update(local_user::table.find(local_user_id))
      .set((local_user::password_encrypted.eq(password_hash),))
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn set_all_users_email_verified(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(local_user::table.filter(local_user::local_site_id.eq(local_site_id)))
      .set(local_user::email_verified.eq(true))
      .get_results::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn set_all_users_registration_applications_accepted(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(local_user::table.filter(local_user::local_site_id.eq(local_site_id)))
      .set(local_user::accepted_application.eq(true))
      .get_results::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn delete_old_denied_local_users(pool: &mut DbPool<'_>) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;

    // Make sure:
    // - An admin has interacted with the application
    // - The app is older than a week
    // - The accepted_application is false
    let old_denied_registrations = registration_application::table
      .filter(registration_application::admin_id.is_not_null())
      .filter(registration_application::published_at.lt(now() - 1.week()))
      .select(registration_application::local_user_id);

    // Delete based on join logic is here:
    // https://stackoverflow.com/questions/60836040/how-do-i-perform-a-delete-with-sub-query-in-diesel-against-a-postgres-database
    let local_users = local_user::table
      .filter(local_user::id.eq_any(old_denied_registrations))
      .filter(not(local_user::accepted_application))
      .select(local_user::person_id);

    // Delete the person rows, which should automatically clear the local_user ones
    let persons = person::table.filter(person::id.eq_any(local_users));

    diesel::delete(persons)
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  pub async fn check_is_email_taken(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
    email: &str,
  ) -> LemmyResult<()> {
    use diesel::dsl::{exists, select};
    let conn = &mut get_conn(pool).await?;
    select(not(exists(
      local_user::table
        .filter(local_user::local_site_id.eq(local_site_id))
        .filter(lower(coalesce(local_user::email, "")).eq(email.to_lowercase())),
    )))
    .get_result::<bool>(conn)
    .await?
    .then_some(())
    .ok_or(LemmyErrorType::EmailAlreadyTaken.into())
  }

  // TODO: maybe move this and pass in LocalUserView
  pub async fn export_backup(
    pool: &mut DbPool<'_>,
    person_id_: PersonId,
  ) -> LemmyResult<UserBackupLists> {
    use lemmy_db_schema_file::schema::{
      comment,
      comment_actions,
      community,
      community_actions,
      instance,
      instance_actions,
      person_actions,
      post,
      post_actions,
    };
    let conn = &mut get_conn(pool).await?;

    let followed_communities = community_actions::table
      .filter(community_actions::followed_at.is_not_null())
      .filter(community_actions::person_id.eq(person_id_))
      .inner_join(community::table)
      .select(community::ap_id)
      .get_results(conn)
      .await?;

    let saved_posts = post_actions::table
      .filter(post_actions::saved_at.is_not_null())
      .filter(post_actions::person_id.eq(person_id_))
      .inner_join(post::table)
      .select(post::ap_id)
      .get_results(conn)
      .await?;

    let saved_comments = comment_actions::table
      .filter(comment_actions::saved_at.is_not_null())
      .filter(comment_actions::person_id.eq(person_id_))
      .inner_join(comment::table)
      .select(comment::ap_id)
      .get_results(conn)
      .await?;

    let blocked_communities = community_actions::table
      .filter(community_actions::blocked_at.is_not_null())
      .filter(community_actions::person_id.eq(person_id_))
      .inner_join(community::table)
      .select(community::ap_id)
      .get_results(conn)
      .await?;

    let blocked_users = person_actions::table
      .filter(person_actions::blocked_at.is_not_null())
      .filter(person_actions::person_id.eq(person_id_))
      .inner_join(person::table.on(person_actions::target_id.eq(person::id)))
      .select(person::ap_id)
      .get_results(conn)
      .await?;

    let blocked_instances_communities = instance_actions::table
      .filter(instance_actions::blocked_communities_at.is_not_null())
      .filter(instance_actions::person_id.eq(person_id_))
      .inner_join(instance::table)
      .select(instance::domain)
      .get_results(conn)
      .await?;

    let blocked_instances_persons = instance_actions::table
      .filter(instance_actions::blocked_persons_at.is_not_null())
      .filter(instance_actions::person_id.eq(person_id_))
      .inner_join(instance::table)
      .select(instance::domain)
      .get_results(conn)
      .await?;

    // TODO: use join for parallel queries?

    Ok(UserBackupLists {
      followed_communities,
      saved_posts,
      saved_comments,
      blocked_communities,
      blocked_users,
      blocked_instances_communities,
      blocked_instances_persons,
    })
  }

  /// Checks to make sure the acting admin is higher than the target admin
  pub async fn is_higher_admin_check(
    pool: &mut DbPool<'_>,
    admin_person_id: PersonId,
    target_person_ids: Vec<PersonId>,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;

    let admin_site = local_user::table
      .filter(local_user::person_id.eq(admin_person_id))
      .filter(local_user::admin.eq(true))
      .select(local_user::local_site_id)
      .first::<LocalSiteId>(conn)
      .await?;

    // Build the list of persons
    let mut persons = target_person_ids;
    persons.push(admin_person_id);
    persons.dedup();

    let res = local_user::table
      .filter(local_user::admin.eq(true))
      .filter(local_user::local_site_id.eq(admin_site))
      .filter(local_user::person_id.eq_any(persons))
      .order_by(local_user::id)
      // This does a limit 1 select first
      .first::<LocalUser>(conn)
      .await?;

    // If the first result sorted by published is the acting admin
    if res.person_id == admin_person_id {
      Ok(())
    } else {
      Err(LemmyErrorType::NotHigherAdmin.into())
    }
  }

  /// Checks to make sure the acting moderator is higher than the target moderator
  pub async fn is_higher_mod_or_admin_check(
    pool: &mut DbPool<'_>,
    for_community_id: CommunityId,
    admin_person_id: PersonId,
    target_person_ids: Vec<PersonId>,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;

    // Local communities use their owning site's administrator hierarchy.
    // Remote communities use the acting local user's site, when one exists.
    let community_site = community::table
      .find(for_community_id)
      .select(community::origin_local_site_id)
      .first::<Option<LocalSiteId>>(conn)
      .await?;
    let actor_site = local_user::table
      .filter(local_user::person_id.eq(admin_person_id))
      .select(local_user::local_site_id)
      .first::<LocalSiteId>(conn)
      .await
      .optional()?;
    let hierarchy_site = community_site.or(actor_site);

    // Build the list of persons
    let mut persons = target_person_ids;
    persons.push(admin_person_id);
    persons.dedup();

    let admins = local_user::table
      .filter(local_user::admin.eq(true))
      .filter(local_user::local_site_id.nullable().eq(hierarchy_site))
      .filter(local_user::person_id.eq_any(&persons))
      .order_by(local_user::id)
      .select(local_user::person_id);

    let mods = community_actions::table
      .filter(community_actions::became_moderator_at.is_not_null())
      .filter(community_actions::community_id.eq(for_community_id))
      .filter(community_actions::person_id.eq_any(&persons))
      .order_by(community_actions::became_moderator_at)
      .select(community_actions::person_id);

    let res = admins.union_all(mods).get_results::<PersonId>(conn).await?;
    let first_person = res.as_slice().first().ok_or(LemmyErrorType::NotHigherMod)?;

    // If the first result sorted by published is the acting mod
    if *first_person == admin_person_id {
      Ok(())
    } else {
      Err(LemmyErrorType::NotHigherMod.into())
    }
  }
}

/// Adds some helper functions for an optional LocalUser
pub trait LocalUserOptionHelper {
  fn person_id(&self) -> Option<PersonId>;
  fn local_user_id(&self) -> Option<LocalUserId>;
  fn show_bot_accounts(&self) -> bool;
  fn show_read_posts(&self) -> bool;
  fn is_admin(&self) -> bool;
  fn show_nsfw(&self, site: &Site) -> bool;
  fn hide_posts_with_media(&self) -> bool;
}

impl LocalUserOptionHelper for Option<&LocalUser> {
  fn person_id(&self) -> Option<PersonId> {
    self.map(|l| l.person_id)
  }

  fn local_user_id(&self) -> Option<LocalUserId> {
    self.map(|l| l.id)
  }

  fn show_bot_accounts(&self) -> bool {
    self.map(|l| l.show_bot_accounts).unwrap_or(true)
  }

  fn show_read_posts(&self) -> bool {
    self.map(|l| l.show_read_posts).unwrap_or(true)
  }

  fn is_admin(&self) -> bool {
    self.map(|l| l.admin).unwrap_or(false)
  }

  fn show_nsfw(&self, site: &Site) -> bool {
    self
      .map(|l| l.show_nsfw)
      .unwrap_or(site.content_warning.is_some())
  }

  fn hide_posts_with_media(&self) -> bool {
    self.map(|l| l.hide_posts_with_media).unwrap_or(false)
  }
}

impl LocalUserInsertForm {
  pub fn test_form(person_id: PersonId) -> Self {
    Self::new(person_id, LocalSiteId(1), Some(String::new()))
  }

  pub fn test_form_admin(person_id: PersonId) -> Self {
    LocalUserInsertForm {
      admin: Some(true),
      ..Self::test_form(person_id)
    }
  }
}

pub struct UserBackupLists {
  pub followed_communities: Vec<DbUrl>,
  pub saved_posts: Vec<DbUrl>,
  pub saved_comments: Vec<DbUrl>,
  pub blocked_communities: Vec<DbUrl>,
  pub blocked_users: Vec<DbUrl>,
  pub blocked_instances_communities: Vec<String>,
  pub blocked_instances_persons: Vec<String>,
}

#[cfg(test)]
mod tests {
  use crate::source::{
    instance::Instance,
    local_user::{LocalUser, LocalUserInsertForm, LocalUserUpdateForm},
    person::{Person, PersonInsertForm},
  };
  use lemmy_diesel_utils::{connection::build_db_pool_for_tests, traits::Crud};
  use lemmy_utils::error::LemmyResult;
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn test_admin_higher_check() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let inserted_instance = Instance::read_or_create(pool, "my_domain.tld").await?;

    let fiona_person = PersonInsertForm::test_form(inserted_instance.id, "fiona");
    let inserted_fiona_person = Person::create(pool, &fiona_person).await?;

    let fiona_local_user_form = LocalUserInsertForm::test_form_admin(inserted_fiona_person.id);
    let _inserted_fiona_local_user =
      LocalUser::create(pool, &fiona_local_user_form, vec![]).await?;

    let delores_person = PersonInsertForm::test_form(inserted_instance.id, "delores");
    let inserted_delores_person = Person::create(pool, &delores_person).await?;
    let delores_local_user_form = LocalUserInsertForm::test_form_admin(inserted_delores_person.id);
    let _inserted_delores_local_user =
      LocalUser::create(pool, &delores_local_user_form, vec![]).await?;

    let admin_person_ids = vec![inserted_fiona_person.id, inserted_delores_person.id];

    // Make sure fiona is marked as a higher admin than delores, and vice versa
    let fiona_higher_check =
      LocalUser::is_higher_admin_check(pool, inserted_fiona_person.id, admin_person_ids.clone())
        .await;
    assert!(fiona_higher_check.is_ok());

    // This should throw an error, since delores was added later
    let delores_higher_check =
      LocalUser::is_higher_admin_check(pool, inserted_delores_person.id, admin_person_ids).await;
    assert!(delores_higher_check.is_err());

    Instance::delete(pool, inserted_instance.id).await?;

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_email_taken() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let darwin_email = "charles.darwin@gmail.com";

    let inserted_instance = Instance::read_or_create(pool, "my_domain.tld").await?;

    let darwin_person = PersonInsertForm::test_form(inserted_instance.id, "darwin");
    let inserted_darwin_person = Person::create(pool, &darwin_person).await?;

    let mut darwin_local_user_form =
      LocalUserInsertForm::test_form_admin(inserted_darwin_person.id);
    darwin_local_user_form.email = Some(darwin_email.into());
    let inserted_darwin_local_user =
      LocalUser::create(pool, &darwin_local_user_form, vec![]).await?;

    let check =
      LocalUser::check_is_email_taken(pool, inserted_darwin_local_user.local_site_id, darwin_email)
        .await;
    assert!(check.is_err());

    let passed_check = LocalUser::check_is_email_taken(
      pool,
      inserted_darwin_local_user.local_site_id,
      "not_charles@gmail.com",
    )
    .await;
    assert!(passed_check.is_ok());

    Instance::delete(pool, inserted_instance.id).await?;

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn same_username_and_email_on_two_sites() -> LemmyResult<()> {
    use crate::{
      impls::presence::{
        SiteReportId,
        SiteReportObject,
        combined_report_id,
        ensure_report_site,
        lock_report_activity,
        read_report_site_state,
        record_report_activity,
        resolve_report_for_site,
        resolve_reports_for_site,
      },
      source::{
        community::{Community, CommunityActions, CommunityInsertForm, CommunityModeratorForm},
        custom_emoji::{CustomEmoji, CustomEmojiInsertForm, CustomEmojiUpdateForm},
        local_site::{LocalSite, LocalSiteInsertForm},
        oauth_provider::{AdminOAuthProvider, OAuthProviderInsertForm, OAuthProviderUpdateForm},
        post::{Post, PostInsertForm},
        post_report::{PostReport, PostReportForm},
        site::{Site, SiteUpsertForm},
        tagline::{Tagline, TaglineInsertForm, TaglineUpdateForm},
      },
      traits::Reportable,
    };
    use chrono::Utc;
    use diesel_async::AsyncConnection;
    use lemmy_diesel_utils::connection::get_conn;
    use lemmy_utils::error::LemmyError;

    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let first_domain = format!("first-{suffix}.example");
    let second_domain = format!("second-{suffix}.example");
    let first_instance = Instance::read_or_create(pool, &first_domain).await?;
    let second_instance = Instance::read_or_create(pool, &second_domain).await?;
    let first_site = Site::create(
      pool,
      &SiteUpsertForm::new("first site".into(), first_instance.id),
    )
    .await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("second site".into(), second_instance.id),
    )
    .await?;
    let first_system = Person::create(
      pool,
      &PersonInsertForm::test_form(first_instance.id, "first_system"),
    )
    .await?;
    let second_system = Person::create(
      pool,
      &PersonInsertForm::test_form(second_instance.id, "second_system"),
    )
    .await?;
    let first_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(first_site.id, first_system.id, first_domain.clone()),
    )
    .await?;
    let second_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(second_site.id, second_system.id, second_domain.clone()),
    )
    .await?;

    let username = format!("same_name_{suffix}");
    let first_person = Person::create(
      pool,
      &PersonInsertForm {
        origin_local_site_id: Some(first_local_site.id),
        ..PersonInsertForm::test_form(first_instance.id, &username)
      },
    )
    .await?;
    let second_person = Person::create(
      pool,
      &PersonInsertForm {
        origin_local_site_id: Some(second_local_site.id),
        ..PersonInsertForm::test_form(second_instance.id, &username)
      },
    )
    .await?;
    assert_ne!(first_person.id, second_person.id);

    let email = format!("same-{suffix}@example.com");
    let first_user = LocalUser::create(
      pool,
      &LocalUserInsertForm {
        email: Some(email.clone()),
        ..LocalUserInsertForm::new(first_person.id, first_local_site.id, Some("pass".into()))
      },
      vec![],
    )
    .await?;
    let second_user = LocalUser::create(
      pool,
      &LocalUserInsertForm {
        email: Some(email.clone()),
        ..LocalUserInsertForm::new(second_person.id, second_local_site.id, Some("pass".into()))
      },
      vec![],
    )
    .await?;
    assert_ne!(first_user.id, second_user.id);
    let admin_form = LocalUserUpdateForm {
      admin: Some(true),
      ..Default::default()
    };
    assert!(
      LocalUser::update_for_site(pool, second_user.id, first_local_site.id, &admin_form)
        .await
        .is_err()
    );
    let first_admin =
      LocalUser::update_for_site(pool, first_user.id, first_local_site.id, &admin_form).await?;
    assert!(first_admin.admin);
    let second_admin =
      LocalUser::update_for_site(pool, second_user.id, second_local_site.id, &admin_form).await?;
    assert!(second_admin.admin);
    assert!(
      LocalUser::is_higher_admin_check(pool, second_person.id, vec![first_person.id])
        .await
        .is_ok()
    );
    assert!(
      LocalUser::check_is_email_taken(pool, first_local_site.id, &email)
        .await
        .is_err()
    );
    assert!(
      LocalUser::check_is_email_taken(pool, second_local_site.id, &email)
        .await
        .is_err()
    );

    let community_name = format!("same_community_{suffix}");
    let first_community = Community::create(
      pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(first_local_site.id),
        ..CommunityInsertForm::new(first_instance.id, community_name.clone(), "pubkey".into())
      },
    )
    .await?;
    let second_community = Community::create(
      pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(second_local_site.id),
        ..CommunityInsertForm::new(second_instance.id, community_name, "pubkey".into())
      },
    )
    .await?;
    assert_ne!(first_community.id, second_community.id);
    CommunityActions::join(
      pool,
      &CommunityModeratorForm::new(first_community.id, second_person.id),
    )
    .await?;
    assert!(
      LocalUser::is_higher_mod_or_admin_check(
        pool,
        first_community.id,
        second_person.id,
        vec![first_person.id],
      )
      .await
      .is_err()
    );
    assert!(
      LocalUser::is_higher_mod_or_admin_check(
        pool,
        second_community.id,
        second_person.id,
        vec![first_person.id],
      )
      .await
      .is_ok()
    );

    let oauth_url = url::Url::parse("https://identity.example/oauth")?;
    let oauth_form = OAuthProviderInsertForm {
      local_site_id: first_local_site.id,
      display_name: "provider".into(),
      issuer: oauth_url.clone().into(),
      authorization_endpoint: oauth_url.clone().into(),
      token_endpoint: oauth_url.clone().into(),
      userinfo_endpoint: oauth_url.clone().into(),
      id_claim: "sub".into(),
      client_id: "shared-client".into(),
      client_secret: "secret".into(),
      scopes: "openid".into(),
      auto_verify_email: None,
      account_linking_enabled: None,
      use_pkce: None,
      enabled: None,
    };
    let first_provider = AdminOAuthProvider::create(pool, &oauth_form).await?;
    assert!(AdminOAuthProvider::create(pool, &oauth_form).await.is_err());
    let second_provider = AdminOAuthProvider::create(
      pool,
      &OAuthProviderInsertForm {
        local_site_id: second_local_site.id,
        ..oauth_form
      },
    )
    .await?;
    assert_ne!(first_provider.id, second_provider.id);
    let provider_update = OAuthProviderUpdateForm {
      display_name: Some("updated provider".into()),
      ..Default::default()
    };
    assert!(
      AdminOAuthProvider::update_for_site(
        pool,
        second_provider.id,
        first_local_site.id,
        &provider_update
      )
      .await
      .is_err()
    );
    assert_eq!(
      AdminOAuthProvider::update_for_site(
        pool,
        first_provider.id,
        first_local_site.id,
        &provider_update
      )
      .await?
      .display_name,
      "updated provider"
    );
    assert_eq!(
      AdminOAuthProvider::get_all(pool, first_local_site.id)
        .await?
        .len(),
      1
    );
    assert!(
      AdminOAuthProvider::read_for_site(pool, second_provider.id, first_local_site.id)
        .await
        .is_err()
    );
    assert!(
      AdminOAuthProvider::delete_for_site(pool, second_provider.id, first_local_site.id)
        .await
        .is_err()
    );

    let emoji_form = CustomEmojiInsertForm {
      local_site_id: first_local_site.id,
      shortcode: "same".into(),
      image_url: oauth_url.into(),
      alt_text: "same".into(),
      category: "test".into(),
    };
    let first_emoji = CustomEmoji::create(pool, &emoji_form).await?;
    assert!(CustomEmoji::create(pool, &emoji_form).await.is_err());
    let second_emoji = CustomEmoji::create(
      pool,
      &CustomEmojiInsertForm {
        local_site_id: second_local_site.id,
        ..emoji_form
      },
    )
    .await?;
    assert_ne!(first_emoji.id, second_emoji.id);
    let emoji_update = CustomEmojiUpdateForm {
      alt_text: Some("updated emoji".into()),
      ..Default::default()
    };
    assert!(
      CustomEmoji::update_for_site(pool, second_emoji.id, first_local_site.id, &emoji_update)
        .await
        .is_err()
    );
    assert_eq!(
      CustomEmoji::update_for_site(pool, first_emoji.id, first_local_site.id, &emoji_update)
        .await?
        .alt_text,
      "updated emoji"
    );
    assert!(
      CustomEmoji::delete_for_site(pool, second_emoji.id, first_local_site.id)
        .await
        .is_err()
    );

    let first_tagline = Tagline::create(
      pool,
      &TaglineInsertForm {
        local_site_id: first_local_site.id,
        content: "first".into(),
      },
    )
    .await?;
    let second_tagline = Tagline::create(
      pool,
      &TaglineInsertForm {
        local_site_id: second_local_site.id,
        content: "second".into(),
      },
    )
    .await?;
    let tagline_update = TaglineUpdateForm {
      content: "updated tagline".into(),
      updated_at: None,
    };
    assert!(
      Tagline::update_for_site(
        pool,
        second_tagline.id,
        first_local_site.id,
        &tagline_update
      )
      .await
      .is_err()
    );
    assert_eq!(
      Tagline::update_for_site(pool, first_tagline.id, first_local_site.id, &tagline_update)
        .await?
        .content,
      "updated tagline"
    );
    assert_eq!(
      Tagline::get_random(pool, first_local_site.id).await?.id,
      first_tagline.id
    );
    assert!(
      Tagline::delete_for_site(pool, second_tagline.id, first_local_site.id)
        .await
        .is_err()
    );

    let post = Post::create(
      pool,
      &PostInsertForm::new("report test".into(), first_person.id, first_community.id),
    )
    .await?;
    let report = PostReport::report(
      pool,
      &PostReportForm {
        creator_id: first_person.id,
        post_id: post.id,
        original_post_name: post.name.clone(),
        original_post_url: None,
        original_post_body: None,
        reason: "report test".into(),
        violates_instance_rules: true,
      },
    )
    .await?;
    let report_id = combined_report_id(pool, SiteReportId::Post(report.id)).await?;
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<_, LemmyError, _>(async |conn| {
        let pool = &mut conn.into();
        let activity = "https://remote.example/activities/flag/test";
        assert!(lock_report_activity(pool, activity).await?.is_none());
        record_report_activity(pool, activity, report_id).await?;
        assert_eq!(lock_report_activity(pool, activity).await?, Some(report_id));
        Ok(())
      })
      .await?;
    ensure_report_site(pool, first_local_site.id, report_id).await?;
    assert!(
      resolve_report_for_site(
        pool,
        second_local_site.id,
        report_id,
        second_person.id,
        true,
        None
      )
      .await
      .is_err()
    );
    ensure_report_site(pool, second_local_site.id, report_id).await?;
    ensure_report_site(pool, second_local_site.id, report_id).await?;
    resolve_report_for_site(
      pool,
      first_local_site.id,
      report_id,
      first_person.id,
      true,
      Some(Some("handled".into())),
    )
    .await?;
    let first_report = read_report_site_state(pool, first_local_site.id, report_id).await?;
    let second_report = read_report_site_state(pool, second_local_site.id, report_id).await?;
    assert!(first_report.resolved);
    assert_eq!(first_report.conclusion.as_deref(), Some("handled"));
    assert!(!second_report.resolved);
    assert!(second_report.resolver_id.is_none());
    assert!(second_report.conclusion.is_none());
    resolve_report_for_site(
      pool,
      first_local_site.id,
      report_id,
      first_person.id,
      false,
      None,
    )
    .await?;
    let first_report = read_report_site_state(pool, first_local_site.id, report_id).await?;
    assert!(!first_report.resolved);
    assert_eq!(first_report.conclusion.as_deref(), Some("handled"));
    assert_eq!(
      resolve_reports_for_site(
        pool,
        first_local_site.id,
        SiteReportObject::Post(post.id),
        None,
        first_person.id,
      )
      .await?,
      1
    );
    assert!(
      read_report_site_state(pool, first_local_site.id, report_id)
        .await?
        .resolved
    );
    assert!(
      !read_report_site_state(pool, second_local_site.id, report_id)
        .await?
        .resolved
    );

    Instance::delete(pool, first_instance.id).await?;
    Instance::delete(pool, second_instance.id).await?;
    Ok(())
  }
}
