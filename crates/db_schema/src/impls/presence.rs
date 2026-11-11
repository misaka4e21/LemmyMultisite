use diesel::{ExpressionMethods, QueryDsl, insert_into, update};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{CommentId, CommunityId, LocalSiteId, MultiCommunityId, PostId},
  schema::{
    comment_site,
    community_site,
    local_site,
    multi_community_site,
    person,
    person_site,
    post_site,
    private_message_site,
  },
};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

/// Returns the only local site when the installation has exactly one site.
///
/// This preserves single-site behavior for callers and tests which insert local rows without
/// setting the origin site on the insert form. A multisite installation must pass the origin
/// explicitly.
pub async fn default_local_site_id(pool: &mut DbPool<'_>) -> LemmyResult<Option<LocalSiteId>> {
  let conn = &mut get_conn(pool).await?;
  let ids = local_site::table
    .select(local_site::id)
    .limit(2)
    .load::<LocalSiteId>(conn)
    .await
    .with_lemmy_type(LemmyErrorType::NotFound)?;
  Ok(if ids.len() == 1 {
    ids.into_iter().next()
  } else {
    None
  })
}

pub async fn ensure_person_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  person_id: PersonId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(person_site::table)
    .values((
      person_site::local_site_id.eq(local_site_id),
      person_site::person_id.eq(person_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

/// Marks a local Person as belonging to a site and makes that site see the Person.
///
/// Callers must pass the Person's owning site. An existing origin is left unchanged, so remote
/// People keep a NULL origin and an assigned site is not overwritten. The presence insert is
/// idempotent.
pub async fn ensure_local_person_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  person_id: PersonId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  update(person::table.find(person_id))
    .filter(person::origin_local_site_id.is_null())
    .set(person::origin_local_site_id.eq(local_site_id))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  insert_into(person_site::table)
    .values((
      person_site::local_site_id.eq(local_site_id),
      person_site::person_id.eq(person_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub async fn ensure_community_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  community_id: CommunityId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(community_site::table)
    .values((
      community_site::local_site_id.eq(local_site_id),
      community_site::community_id.eq(community_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub async fn ensure_post_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  post_id: PostId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(post_site::table)
    .values((
      post_site::local_site_id.eq(local_site_id),
      post_site::post_id.eq(post_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub async fn ensure_comment_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  comment_id: CommentId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(comment_site::table)
    .values((
      comment_site::local_site_id.eq(local_site_id),
      comment_site::comment_id.eq(comment_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub async fn ensure_multi_community_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  multi_community_id: MultiCommunityId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(multi_community_site::table)
    .values((
      multi_community_site::local_site_id.eq(local_site_id),
      multi_community_site::multi_community_id.eq(multi_community_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub async fn ensure_private_message_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  private_message_id: lemmy_db_schema_file::newtypes::PrivateMessageId,
  role: &str,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(private_message_site::table)
    .values((
      private_message_site::local_site_id.eq(local_site_id),
      private_message_site::private_message_id.eq(private_message_id),
      private_message_site::role.eq(role),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub async fn set_post_site_removed(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  post_id: PostId,
  removed: bool,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(post_site::table)
    .values((
      post_site::local_site_id.eq(local_site_id),
      post_site::post_id.eq(post_id),
      post_site::removed.eq(removed),
    ))
    .on_conflict((post_site::local_site_id, post_site::post_id))
    .do_update()
    .set(post_site::removed.eq(removed))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  Ok(())
}

pub async fn set_comment_site_removed(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  comment_id: CommentId,
  removed: bool,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(comment_site::table)
    .values((
      comment_site::local_site_id.eq(local_site_id),
      comment_site::comment_id.eq(comment_id),
      comment_site::removed.eq(removed),
    ))
    .on_conflict((comment_site::local_site_id, comment_site::comment_id))
    .do_update()
    .set(comment_site::removed.eq(removed))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  Ok(())
}

pub async fn set_community_site_removed(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  community_id: CommunityId,
  removed: bool,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(community_site::table)
    .values((
      community_site::local_site_id.eq(local_site_id),
      community_site::community_id.eq(community_id),
      community_site::removed.eq(removed),
    ))
    .on_conflict((community_site::local_site_id, community_site::community_id))
    .do_update()
    .set(community_site::removed.eq(removed))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  Ok(())
}

pub async fn set_post_site_locked(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  post_id: PostId,
  locked: bool,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(post_site::table)
    .values((
      post_site::local_site_id.eq(local_site_id),
      post_site::post_id.eq(post_id),
      post_site::locked.eq(locked),
    ))
    .on_conflict((post_site::local_site_id, post_site::post_id))
    .do_update()
    .set(post_site::locked.eq(locked))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  Ok(())
}

pub async fn set_post_site_featured(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  post_id: PostId,
  featured: bool,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(post_site::table)
    .values((
      post_site::local_site_id.eq(local_site_id),
      post_site::post_id.eq(post_id),
      post_site::featured.eq(featured),
    ))
    .on_conflict((post_site::local_site_id, post_site::post_id))
    .do_update()
    .set(post_site::featured.eq(featured))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::source::{
    instance::Instance,
    local_site::{LocalSite, LocalSiteInsertForm},
    person::{Person, PersonInsertForm},
    site::{Site, SiteUpsertForm},
  };
  use chrono::Utc;
  use lemmy_diesel_utils::{connection::build_db_pool_for_tests, traits::Crud};
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn ensure_local_person_site_sets_origin_and_presence() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let domain = format!("presence-test-{unique}.example");
    let instance = Instance::read_or_create(pool, &domain).await?;
    let site = Site::create(
      pool,
      &SiteUpsertForm::new("presence-test".to_string(), instance.id),
    )
    .await?;
    let system = Person::create(
      pool,
      &PersonInsertForm::test_form(instance.id, "presence_system"),
    )
    .await?;
    let local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(site.id, system.id, instance.domain.clone()),
    )
    .await?;

    // The system account is created before the LocalSite, so Person::create cannot add presence.
    {
      let conn = &mut get_conn(pool).await?;
      let presence: i64 = person_site::table
        .filter(person_site::person_id.eq(system.id))
        .count()
        .get_result(conn)
        .await?;
      assert_eq!(presence, 0);
    }

    ensure_local_person_site(pool, local_site.id, system.id).await?;
    {
      let conn = &mut get_conn(pool).await?;
      let origin: Option<LocalSiteId> = person::table
        .find(system.id)
        .select(person::origin_local_site_id)
        .first(conn)
        .await?;
      assert_eq!(origin, Some(local_site.id));
      let presence: i64 = person_site::table
        .filter(person_site::person_id.eq(system.id))
        .count()
        .get_result(conn)
        .await?;
      assert_eq!(presence, 1);
    }

    let with_origin = Person::create(
      pool,
      &PersonInsertForm {
        origin_local_site_id: Some(local_site.id),
        ..PersonInsertForm::test_form(instance.id, "presence_with_origin")
      },
    )
    .await?;
    ensure_local_person_site(pool, local_site.id, with_origin.id).await?;
    ensure_local_person_site(pool, local_site.id, with_origin.id).await?;
    {
      let conn = &mut get_conn(pool).await?;
      let origin: Option<LocalSiteId> = person::table
        .find(with_origin.id)
        .select(person::origin_local_site_id)
        .first(conn)
        .await?;
      assert_eq!(origin, Some(local_site.id));
      let presence: i64 = person_site::table
        .filter(person_site::person_id.eq(with_origin.id))
        .count()
        .get_result(conn)
        .await?;
      assert_eq!(presence, 1);
    }

    Instance::delete(pool, instance.id).await?;
    Ok(())
  }
}
