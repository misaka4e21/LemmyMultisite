use chrono::{DateTime, Utc};
use diesel::{ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl, insert_into, update};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{
    CommentId,
    CommentReportId,
    CommunityId,
    CommunityReportId,
    LocalSiteId,
    MultiCommunityId,
    PostId,
    PostReportId,
    PrivateMessageId,
    PrivateMessageReportId,
    ReportCombinedId,
  },
  schema::{
    comment,
    comment_site,
    community,
    community_site,
    local_user,
    local_site,
    multi_community_site,
    person,
    person_site,
    post,
    post_site,
    private_message,
    private_message_site,
    report_activity,
    report_combined,
    report_site,
  },
};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyError, LemmyErrorExt, LemmyErrorType, LemmyResult};

#[derive(Clone, Copy)]
pub enum SiteReportId {
  Post(PostReportId),
  Comment(CommentReportId),
  Community(CommunityReportId),
  PrivateMessage(PrivateMessageReportId),
}

#[derive(Clone, Copy)]
pub enum SiteReportObject {
  Post(PostId),
  Comment(CommentId),
  Community(CommunityId),
  PrivateMessage(PrivateMessageId),
}

/// Delete only a canonical object owned by the given site.
pub async fn delete_post_for_site(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  id: PostId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  diesel::delete(
    post::table
      .find(id)
      .filter(post::origin_local_site_id.eq(Some(site_id))),
  )
  .get_result::<crate::source::post::Post>(conn)
  .await
  .with_lemmy_type(LemmyErrorType::NotFound)?;
  Ok(())
}

pub async fn delete_comment_for_site(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  id: CommentId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  diesel::delete(
    comment::table
      .find(id)
      .filter(comment::origin_local_site_id.eq(Some(site_id))),
  )
  .get_result::<crate::source::comment::Comment>(conn)
  .await
  .with_lemmy_type(LemmyErrorType::NotFound)?;
  Ok(())
}

pub async fn delete_community_for_site(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  id: CommunityId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  diesel::delete(
    community::table
      .find(id)
      .filter(community::origin_local_site_id.eq(Some(site_id))),
  )
  .get_result::<crate::source::community::Community>(conn)
  .await
  .with_lemmy_type(LemmyErrorType::NotFound)?;
  Ok(())
}

/// Resolves matching reports only for the receiving site.
pub async fn resolve_reports_for_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  object: SiteReportObject,
  report_creator_id: Option<PersonId>,
  resolver_id: PersonId,
) -> LemmyResult<usize> {
  let conn = &mut get_conn(pool).await?;
  let mut query = report_combined::table
    .select(report_combined::id)
    .into_boxed();
  query = match object {
    SiteReportObject::Post(id) => query
      .filter(report_combined::post_report_id.is_not_null())
      .filter(report_combined::post_id.eq(id)),
    SiteReportObject::Comment(id) => query
      .filter(report_combined::comment_report_id.is_not_null())
      .filter(report_combined::comment_id.eq(id)),
    SiteReportObject::Community(id) => query
      .filter(report_combined::community_report_id.is_not_null())
      .filter(report_combined::community_id.eq(id)),
    SiteReportObject::PrivateMessage(id) => query
      .filter(report_combined::private_message_report_id.is_not_null())
      .filter(report_combined::private_message_id.eq(id)),
  };
  if let Some(creator_id) = report_creator_id {
    query = query.filter(report_combined::report_creator_id.eq(creator_id));
  }
  let report_ids = query.load::<ReportCombinedId>(conn).await?;
  update(
    report_site::table
      .filter(report_site::local_site_id.eq(local_site_id))
      .filter(report_site::report_combined_id.eq_any(report_ids)),
  )
  .set((
    report_site::resolved.eq(true),
    report_site::resolver_id.eq(Some(resolver_id)),
    report_site::updated_at.eq(Some(Utc::now())),
  ))
  .execute(conn)
  .await
  .with_lemmy_type(LemmyErrorType::CouldntUpdate)
}

/// Serializes deliveries of the same ActivityPub report inside the caller's transaction.
pub async fn lock_report_activity(
  pool: &mut DbPool<'_>,
  ap_id: &str,
) -> LemmyResult<Option<ReportCombinedId>> {
  let conn = &mut get_conn(pool).await?;
  diesel::sql_query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
    .bind::<diesel::sql_types::Text, _>(ap_id)
    .execute(conn)
    .await?;
  report_activity::table
    .find(ap_id)
    .select(report_activity::report_combined_id)
    .first(conn)
    .await
    .optional()
    .with_lemmy_type(LemmyErrorType::NotFound)
}

pub async fn record_report_activity(
  pool: &mut DbPool<'_>,
  ap_id: &str,
  report_id: ReportCombinedId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(report_activity::table)
    .values((
      report_activity::ap_id.eq(ap_id),
      report_activity::report_combined_id.eq(report_id),
    ))
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

pub struct SiteReportState {
  pub resolved: bool,
  pub resolver_id: Option<PersonId>,
  pub conclusion: Option<String>,
  pub updated_at: Option<DateTime<Utc>>,
}

pub async fn read_report_site_state(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  report_id: ReportCombinedId,
) -> LemmyResult<SiteReportState> {
  let conn = &mut get_conn(pool).await?;
  let row = report_site::table
    .find((local_site_id, report_id))
    .select((
      report_site::resolved,
      report_site::resolver_id,
      report_site::conclusion,
      report_site::updated_at,
    ))
    .first(conn)
    .await
    .optional()
    .with_lemmy_type(LemmyErrorType::NotFound)?;
  if let Some((resolved, resolver_id, conclusion, updated_at)) = row {
    return Ok(SiteReportState {
      resolved,
      resolver_id,
      conclusion,
      updated_at,
    });
  }

  // Reports created before site state existed, or inserted directly, belong to their local
  // reporter's site. Fall back to the legacy state so direct reads and resolution keep working.
  let legacy = report_combined::table
    .inner_join(local_user::table.on(local_user::person_id.eq(report_combined::report_creator_id)))
    .filter(report_combined::id.eq(report_id))
    .filter(local_user::local_site_id.eq(local_site_id))
    .select((report_combined::resolved, report_combined::resolver_id))
    .first::<(bool, Option<PersonId>)>(conn)
    .await
    .optional()
    .with_lemmy_type(LemmyErrorType::NotFound)?;
  if let Some((resolved, resolver_id)) = legacy {
    return Ok(SiteReportState {
      resolved,
      resolver_id,
      conclusion: None,
      updated_at: None,
    });
  }

  // On a single-site installation, reports that predate site state belong to that site even
  // when their reporter has no local user row.
  let site_count: i64 = local_site::table.count().get_result(conn).await?;
  let only_site: Option<LocalSiteId> = local_site::table
    .select(local_site::id)
    .first(conn)
    .await
    .optional()?;
  if site_count <= 1 && (site_count == 0 || only_site == Some(local_site_id)) {
    let any_site: Option<LocalSiteId> = report_site::table
      .filter(report_site::report_combined_id.eq(report_id))
      .select(report_site::local_site_id)
      .first(conn)
      .await
      .optional()?;
    if any_site.is_none() {
      let (resolved, resolver_id) = report_combined::table
        .find(report_id)
        .select((report_combined::resolved, report_combined::resolver_id))
        .first::<(bool, Option<PersonId>)>(conn)
        .await?;
      return Ok(SiteReportState {
        resolved,
        resolver_id,
        conclusion: None,
        updated_at: None,
      });
    }
  }
  Err(LemmyErrorType::NotFound.into())
}

pub async fn combined_report_id(
  pool: &mut DbPool<'_>,
  report_id: SiteReportId,
) -> LemmyResult<ReportCombinedId> {
  let conn = &mut get_conn(pool).await?;
  let mut query = report_combined::table
    .select(report_combined::id)
    .into_boxed();
  query = match report_id {
    SiteReportId::Post(id) => query.filter(report_combined::post_report_id.eq(id)),
    SiteReportId::Comment(id) => query.filter(report_combined::comment_report_id.eq(id)),
    SiteReportId::Community(id) => query.filter(report_combined::community_report_id.eq(id)),
    SiteReportId::PrivateMessage(id) => {
      query.filter(report_combined::private_message_report_id.eq(id))
    }
  };
  query
    .first(conn)
    .await
    .with_lemmy_type(LemmyErrorType::NotFound)
}

/// Returns the local site that owns the given local account, if any.
pub async fn local_site_id_for_person(
  pool: &mut DbPool<'_>,
  person_id: PersonId,
) -> LemmyResult<Option<LocalSiteId>> {
  let conn = &mut get_conn(pool).await?;
  local_user::table
    .filter(local_user::person_id.eq(person_id))
    .select(local_user::local_site_id)
    .first(conn)
    .await
    .optional()
    .map_err(Into::into)
}

/// Changes only the selected site's report state and rejects missing associations.
pub async fn resolve_report_for_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  report_id: ReportCombinedId,
  resolver_id: PersonId,
  resolved: bool,
  conclusion: Option<Option<String>>,
) -> LemmyResult<()> {
  // Allow resolving legacy local reports by creating their site association first.
  read_report_site_state(pool, local_site_id, report_id).await?;
  ensure_report_site(pool, local_site_id, report_id).await?;
  let conn = &mut get_conn(pool).await?;
  let target = report_site::table.find((local_site_id, report_id));
  let changed = if let Some(conclusion) = conclusion {
    update(target)
      .set((
        report_site::resolved.eq(resolved),
        report_site::resolver_id.eq(Some(resolver_id)),
        report_site::conclusion.eq(conclusion),
        report_site::updated_at.eq(Some(Utc::now())),
      ))
      .execute(conn)
      .await
  } else {
    update(target)
      .set((
        report_site::resolved.eq(resolved),
        report_site::resolver_id.eq(Some(resolver_id)),
        report_site::updated_at.eq(Some(Utc::now())),
      ))
      .execute(conn)
      .await
  }
  .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
  if changed != 1 {
    return Err(LemmyErrorType::NotFound.into());
  }
  Ok(())
}

/// Records that a site received a report without changing another site's resolution state.
pub async fn ensure_report_site(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  report_id: ReportCombinedId,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  insert_into(report_site::table)
    .values((
      report_site::local_site_id.eq(local_site_id),
      report_site::report_combined_id.eq(report_id),
    ))
    .on_conflict_do_nothing()
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
  Ok(())
}

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

/// Change only the selected site's existing visibility records for a creator's content.
pub async fn set_creator_content_site_removed(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  creator_id: PersonId,
  removed: bool,
) -> LemmyResult<(Vec<PostId>, Vec<CommentId>, Vec<PrivateMessageId>)> {
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<_, LemmyError, _>(async |conn| {
      let post_ids = update(
        post_site::table
          .filter(post_site::local_site_id.eq(local_site_id))
          .filter(
            post_site::post_id.eq_any(
              post::table
                .filter(post::creator_id.eq(creator_id))
                .select(post::id),
            ),
          ),
      )
      .set(post_site::ban_removed.eq(removed))
      .returning(post_site::post_id)
      .get_results(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
      let comment_ids = update(
        comment_site::table
          .filter(comment_site::local_site_id.eq(local_site_id))
          .filter(
            comment_site::comment_id.eq_any(
              comment::table
                .filter(comment::creator_id.eq(creator_id))
                .select(comment::id),
            ),
          ),
      )
      .set(comment_site::ban_removed.eq(removed))
      .returning(comment_site::comment_id)
      .get_results(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
      let private_message_ids = update(
        private_message_site::table
          .filter(private_message_site::local_site_id.eq(local_site_id))
          .filter(
            private_message_site::private_message_id.eq_any(
              private_message::table
                .filter(private_message::creator_id.eq(creator_id))
                .select(private_message::id),
            ),
          ),
      )
      .set(private_message_site::ban_removed.eq(removed))
      .returning(private_message_site::private_message_id)
      .get_results(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)?;
      Ok((post_ids, comment_ids, private_message_ids))
    })
    .await
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
    comment::{Comment, CommentInsertForm},
    community::{Community, CommunityInsertForm},
    instance::Instance,
    local_site::{LocalSite, LocalSiteInsertForm},
    person::{Person, PersonInsertForm},
    post::{Post, PostInsertForm},
    private_message::{PrivateMessage, PrivateMessageInsertForm},
    site::{Site, SiteUpsertForm},
  };
  use chrono::Utc;
  use lemmy_diesel_utils::{connection::build_db_pool_for_tests, traits::Crud};
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn creator_content_removal_is_site_local() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = crate::test_data::TestData::create(pool).await?;
    let instance = Instance::read_or_create(pool, "second-presence.example").await?;
    let site = Site::create(pool, &SiteUpsertForm::new("second".into(), instance.id)).await?;
    let system = Person::create(pool, &PersonInsertForm::test_form(instance.id, "system")).await?;
    let second = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(site.id, system.id, instance.domain.clone()),
    )
    .await?;
    let community = Community::create(
      pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..CommunityInsertForm::new(data.instance.id, "community".into(), "key".into())
      },
    )
    .await?;
    let post = Post::create(
      pool,
      &PostInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..PostInsertForm::new("post".into(), data.person.id, community.id)
      },
    )
    .await?;
    let comment = Comment::create(
      pool,
      &CommentInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..CommentInsertForm::new(data.person.id, post.id, community.id, "comment".into())
      },
      None,
    )
    .await?;
    let message = PrivateMessage::create(
      pool,
      &PrivateMessageInsertForm::new(data.person.id, system.id, "message".into()),
    )
    .await?;
    for site_id in [data.local_site.id, second.id] {
      ensure_post_site(pool, site_id, post.id).await?;
      ensure_comment_site(pool, site_id, comment.id).await?;
      ensure_private_message_site(pool, site_id, message.id, "both").await?;
    }
    set_comment_site_removed(pool, data.local_site.id, comment.id, true).await?;
    let (posts, comments, messages) =
      set_creator_content_site_removed(pool, data.local_site.id, data.person.id, true).await?;
    assert_eq!(posts, vec![post.id]);
    assert_eq!(comments, vec![comment.id]);
    assert_eq!(messages, vec![message.id]);
    for (site_id, expected) in [(data.local_site.id, true), (second.id, false)] {
      let conn = &mut get_conn(pool).await?;
      let post_removed: bool = post_site::table
        .find((site_id, post.id))
        .select(post_site::ban_removed)
        .first(conn)
        .await?;
      let comment_removed: bool = comment_site::table
        .find((site_id, comment.id))
        .select(comment_site::ban_removed)
        .first(conn)
        .await?;
      let message_removed: bool = private_message_site::table
        .find((site_id, message.id))
        .select(private_message_site::ban_removed)
        .first(conn)
        .await?;
      assert_eq!(
        (post_removed, comment_removed, message_removed),
        (expected, expected, expected)
      );
    }
    assert!(!Post::read(pool, post.id).await?.removed);
    assert!(!Comment::read(pool, comment.id).await?.removed);
    assert!(!PrivateMessage::read(pool, message.id).await?.removed);
    set_creator_content_site_removed(pool, data.local_site.id, data.person.id, false).await?;
    let conn = &mut get_conn(pool).await?;
    assert!(
      !post_site::table
        .find((data.local_site.id, post.id))
        .select(post_site::ban_removed)
        .first::<bool>(conn)
        .await?
    );
    assert!(
      !comment_site::table
        .find((data.local_site.id, comment.id))
        .select(comment_site::ban_removed)
        .first::<bool>(conn)
        .await?
    );
    assert!(
      comment_site::table
        .find((data.local_site.id, comment.id))
        .select(comment_site::removed)
        .first::<bool>(conn)
        .await?
    );
    assert!(
      !private_message_site::table
        .find((data.local_site.id, message.id))
        .select(private_message_site::ban_removed)
        .first::<bool>(conn)
        .await?
    );
    assert!(
      delete_comment_for_site(pool, second.id, comment.id)
        .await
        .is_err()
    );
    assert!(
      delete_post_for_site(pool, second.id, post.id)
        .await
        .is_err()
    );
    assert!(
      delete_community_for_site(pool, second.id, community.id)
        .await
        .is_err()
    );
    Comment::read(pool, comment.id).await?;
    Post::read(pool, post.id).await?;
    Community::read(pool, community.id).await?;
    delete_comment_for_site(pool, data.local_site.id, comment.id).await?;
    delete_post_for_site(pool, data.local_site.id, post.id).await?;
    delete_community_for_site(pool, data.local_site.id, community.id).await?;
    assert!(Comment::read(pool, comment.id).await.is_err());
    assert!(Post::read(pool, post.id).await.is_err());
    assert!(Community::read(pool, community.id).await.is_err());
    Instance::delete(pool, instance.id).await?;
    data.delete(pool).await
  }

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
