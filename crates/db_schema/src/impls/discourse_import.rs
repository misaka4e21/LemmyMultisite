use crate::{
  impls::presence::{ensure_local_person_site, ensure_private_message_site},
  source::{
    comment::{Comment, CommentActions, CommentInsertForm, CommentLikeForm},
    community::{Community, CommunityInsertForm},
    community_tag::{CommunityTag, CommunityTagInsertForm},
    local_user::{LocalUser, LocalUserInsertForm},
    multi_community::{MultiCommunity, MultiCommunityInsertForm},
    person::{Person, PersonActions, PersonInsertForm},
    post::{Post, PostActions, PostInsertForm, PostLikeForm},
    private_message::{PrivateMessage, PrivateMessageInsertForm},
  },
  traits::Likeable,
};
use chrono::{DateTime, Utc};
use diesel::{
  OptionalExtension, QueryableByName,
  sql_types::{BigInt, Integer, Text, Timestamptz},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_db_schema_file::{
  PersonId,
  newtypes::{CommentId, LanguageId, LocalSiteId, PostId},
};
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  traits::Crud,
};
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

#[derive(QueryableByName, Debug, Clone)]
pub struct DiscourseImportRun {
  #[diesel(sql_type = BigInt)]
  pub id: i64,
  #[diesel(sql_type = Text)]
  pub status: String,
}

/// Whether a phase reached a durable completed state for this run.
pub async fn phase_completed(pool: &mut DbPool<'_>, run_id: i64, phase: &str) -> LemmyResult<bool> {
  #[derive(QueryableByName)]
  struct Phase {
    #[diesel(sql_type = Text)]
    status: String,
  }
  let conn = &mut get_conn(pool).await?;
  Ok(
    diesel::sql_query("SELECT status FROM discourse_import_phase WHERE run_id = $1 AND phase = $2")
      .bind::<BigInt, _>(run_id)
      .bind::<Text, _>(phase)
      .get_result::<Phase>(conn)
      .await
      .optional()?
      .is_some_and(|phase| phase.status == "completed"),
  )
}

/// Create the site-owned identity and its source mapping atomically.
pub async fn create_imported_account(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_user_id: &str,
  local_site_id: LocalSiteId,
  person_form: &PersonInsertForm,
  local_user_form: &LocalUserInsertForm,
  languages: Vec<LanguageId>,
) -> LemmyResult<(Person, LocalUser)> {
  if person_form.origin_local_site_id != Some(local_site_id)
    || local_user_form.local_site_id != local_site_id
    || local_user_form
      .password_encrypted
      .as_deref()
      .is_some_and(|hash| !super::password::is_supported_password_verifier(hash))
  {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<(Person, LocalUser), lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let person = Person::create(inner_pool, person_form).await?;
      ensure_local_person_site(inner_pool, local_site_id, person.id).await?;
      let mut local_user_form = local_user_form.clone();
      local_user_form.person_id = person.id;
      let local_user = if local_user_form.password_encrypted.is_some() {
        LocalUser::create_with_password_verifier(inner_pool, &local_user_form, languages).await?
      } else {
        LocalUser::create_without_password_verifier(inner_pool, &local_user_form, languages).await?
      };
      record_mapping(
        inner_pool,
        run_id,
        "user",
        source_user_id,
        i64::from(person.id.0),
      )
      .await?;
      Ok((person, local_user))
    })
    .await
}

/// Create a community and its source mapping in the same transaction.
pub async fn create_imported_community(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_category_id: &str,
  local_site_id: LocalSiteId,
  form: &CommunityInsertForm,
) -> LemmyResult<Community> {
  if form.origin_local_site_id != Some(local_site_id) {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<Community, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let community = Community::create(inner_pool, form).await?;
      record_mapping(
        inner_pool,
        run_id,
        "category_community",
        source_category_id,
        i64::from(community.id.0),
      )
      .await?;
      Ok(community)
    })
    .await
}

/// Create a multi-community and its source mapping atomically.
pub async fn create_imported_multi_community(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_category_id: &str,
  local_site_id: LocalSiteId,
  form: &MultiCommunityInsertForm,
) -> LemmyResult<MultiCommunity> {
  if form.origin_local_site_id != Some(local_site_id) {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<MultiCommunity, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let multi = MultiCommunity::create(inner_pool, form).await?;
      record_mapping(
        inner_pool,
        run_id,
        "category_multi_community",
        source_category_id,
        i64::from(multi.id.0),
      )
      .await?;
      Ok(multi)
    })
    .await
}

/// Create a community-scoped tag and its source mapping atomically.
pub async fn create_imported_community_tag(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_tag_key: &str,
  form: &CommunityTagInsertForm,
) -> LemmyResult<CommunityTag> {
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<CommunityTag, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let tag = CommunityTag::create(inner_pool, form).await?;
      record_mapping(
        inner_pool,
        run_id,
        "category_tag",
        source_tag_key,
        i64::from(tag.id.0),
      )
      .await?;
      Ok(tag)
    })
    .await
}

/// Create a discussion starter and both source mappings atomically.
pub async fn create_imported_post(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_topic_id: &str,
  source_post_id: &str,
  local_site_id: LocalSiteId,
  form: &PostInsertForm,
) -> LemmyResult<Post> {
  if form.origin_local_site_id != Some(local_site_id) {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<Post, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let post = Post::create(inner_pool, form).await?;
      record_mapping(
        inner_pool,
        run_id,
        "topic",
        source_topic_id,
        i64::from(post.id.0),
      )
      .await?;
      record_mapping(
        inner_pool,
        run_id,
        "source_discussion_post",
        source_post_id,
        i64::from(post.id.0),
      )
      .await?;
      Ok(post)
    })
    .await
}

/// Create a reply and its source mapping atomically.
pub async fn create_imported_comment(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_post_id: &str,
  local_site_id: LocalSiteId,
  form: &CommentInsertForm,
  parent_path: Option<&diesel_ltree::Ltree>,
) -> LemmyResult<Comment> {
  if form.origin_local_site_id != Some(local_site_id) {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<Comment, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let comment = Comment::create(inner_pool, form, parent_path).await?;
      record_mapping(
        inner_pool,
        run_id,
        "source_discussion_post",
        source_post_id,
        i64::from(comment.id.0),
      )
      .await?;
      Ok(comment)
    })
    .await
}

/// Create a private-message copy, both site-presence edges, and its source mapping atomically.
pub async fn create_imported_private_message(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_key: &str,
  source_post_id: &str,
  local_site_id: LocalSiteId,
  form: &PrivateMessageInsertForm,
) -> LemmyResult<PrivateMessage> {
  if form.origin_local_site_id != Some(local_site_id) || form.local != Some(true) {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<PrivateMessage, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      let message = PrivateMessage::create(inner_pool, form).await?;
      // `PrivateMessage::create` normally adds sender presence to the sender's
      // origin site. Imported messages belong only to the selected target site,
      // so remove any site edge it added before assigning both participant roles.
      diesel::sql_query(
        "DELETE FROM private_message_site WHERE private_message_id = $1 AND local_site_id <> $2",
      )
      .bind::<Integer, _>(message.id.0)
      .bind::<Integer, _>(local_site_id.0)
      .execute(&mut get_conn(inner_pool).await?)
      .await?;
      ensure_private_message_site(inner_pool, local_site_id, message.id, "both").await?;
      diesel::sql_query(
        "UPDATE private_message_site SET role = 'both' \
         WHERE local_site_id = $1 AND private_message_id = $2",
      )
      .bind::<Integer, _>(local_site_id.0)
      .bind::<Integer, _>(message.id.0)
      .execute(&mut get_conn(inner_pool).await?)
      .await?;
      record_mapping(
        inner_pool,
        run_id,
        "private_message_sender",
        source_post_id,
        i64::from(form.creator_id.0),
      )
      .await?;
      record_mapping(
        inner_pool,
        run_id,
        "private_message_recipient",
        source_key,
        i64::from(form.recipient_id.0),
      )
      .await?;
      record_mapping(
        inner_pool,
        run_id,
        "private_message",
        source_key,
        i64::from(message.id.0),
      )
      .await?;
      Ok(message)
    })
    .await
}

/// Import a vote with the normal aggregate update path and a resumable source mapping.
pub async fn create_imported_vote(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_action_id: &str,
  person_id: PersonId,
  target_id: i32,
  is_post: bool,
  voted_at: DateTime<Utc>,
) -> LemmyResult<bool> {
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<bool, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      if read_mapping(inner_pool, run_id, "like", source_action_id)
        .await?
        .is_some()
      {
        return Ok(false);
      }
      let target = if is_post {
        let post_id = PostId(target_id);
        let post = Post::read(inner_pool, post_id).await?;
        PostActions::like(
          inner_pool,
          &PostLikeForm::new(post_id, person_id, Some(true)),
        )
        .await?;
        PersonActions::like(inner_pool, person_id, post.creator_id, None, Some(true)).await?;
        diesel::sql_query(
          "UPDATE post_actions SET voted_at = $3 WHERE person_id = $1 AND post_id = $2",
        )
        .bind::<Integer, _>(person_id.0)
        .bind::<Integer, _>(target_id)
        .bind::<Timestamptz, _>(voted_at)
        .execute(&mut get_conn(inner_pool).await?)
        .await?;
        i64::from(target_id)
      } else {
        let comment_id = CommentId(target_id);
        let comment = Comment::read(inner_pool, comment_id).await?;
        CommentActions::like(
          inner_pool,
          &CommentLikeForm::new(comment_id, person_id, Some(true)),
        )
        .await?;
        PersonActions::like(inner_pool, person_id, comment.creator_id, None, Some(true)).await?;
        diesel::sql_query(
          "UPDATE comment_actions SET voted_at = $3 WHERE person_id = $1 AND comment_id = $2",
        )
        .bind::<Integer, _>(person_id.0)
        .bind::<Integer, _>(target_id)
        .bind::<Timestamptz, _>(voted_at)
        .execute(&mut get_conn(inner_pool).await?)
        .await?;
        i64::from(target_id)
      };
      record_mapping(
        inner_pool,
        run_id,
        "like_actor",
        source_action_id,
        i64::from(person_id.0),
      )
      .await?;
      record_mapping(inner_pool, run_id, "like", source_action_id, target).await?;
      Ok(true)
    })
    .await
}

/// Import a bookmark and its source mapping atomically; many bookmarks may target one object.
pub async fn create_imported_bookmark(
  pool: &mut DbPool<'_>,
  run_id: i64,
  source_bookmark_id: &str,
  person_id: PersonId,
  target_id: i32,
  is_post: bool,
  saved_at: DateTime<Utc>,
) -> LemmyResult<bool> {
  let conn = &mut get_conn(pool).await?;
  conn
    .transaction::<bool, lemmy_utils::error::LemmyError, _>(async |conn| {
      let inner_pool = &mut conn.into();
      if read_mapping(inner_pool, run_id, "bookmark", source_bookmark_id)
        .await?
        .is_some()
      {
        return Ok(false);
      }
      if is_post {
        diesel::sql_query(
          "INSERT INTO post_actions (person_id, post_id, saved_at) VALUES ($1, $2, $3) \
           ON CONFLICT (person_id, post_id) DO UPDATE SET saved_at = EXCLUDED.saved_at",
        )
        .bind::<Integer, _>(person_id.0)
        .bind::<Integer, _>(target_id)
        .bind::<Timestamptz, _>(saved_at)
        .execute(&mut get_conn(inner_pool).await?)
        .await?;
      } else {
        diesel::sql_query(
          "INSERT INTO comment_actions (person_id, comment_id, saved_at) VALUES ($1, $2, $3) \
           ON CONFLICT (person_id, comment_id) DO UPDATE SET saved_at = EXCLUDED.saved_at",
        )
        .bind::<Integer, _>(person_id.0)
        .bind::<Integer, _>(target_id)
        .bind::<Timestamptz, _>(saved_at)
        .execute(&mut get_conn(inner_pool).await?)
        .await?;
      }
      record_mapping(
        inner_pool,
        run_id,
        "bookmark_actor",
        source_bookmark_id,
        i64::from(person_id.0),
      )
      .await?;
      record_mapping(
        inner_pool,
        run_id,
        "bookmark",
        source_bookmark_id,
        i64::from(target_id),
      )
      .await?;
      Ok(true)
    })
    .await
}

/// Change the lifecycle state of an import run. Completed runs cannot be reopened.
pub async fn set_run_status(pool: &mut DbPool<'_>, run_id: i64, status: &str) -> LemmyResult<bool> {
  if !matches!(status, "previewed" | "running" | "failed" | "completed") {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  #[derive(QueryableByName)]
  struct Updated {
    #[diesel(sql_type = BigInt)]
    id: i64,
  }
  diesel::sql_query(
    "UPDATE discourse_import_run SET status = $2, updated_at = now() \
     WHERE id = $1 AND (status <> 'completed' OR $2 = 'completed') RETURNING id",
  )
  .bind::<BigInt, _>(run_id)
  .bind::<Text, _>(status)
  .get_result::<Updated>(conn)
  .await
  .map(|updated| updated.id > 0)
  .map_err(Into::into)
}

/// Persist the operator-facing report as JSON in the durable run record.
pub async fn store_run_report(
  pool: &mut DbPool<'_>,
  run_id: i64,
  report: &serde_json::Value,
) -> LemmyResult<bool> {
  #[derive(QueryableByName)]
  struct Updated {
    #[diesel(sql_type = BigInt)]
    id: i64,
  }
  let conn = &mut get_conn(pool).await?;
  diesel::sql_query(
    "UPDATE discourse_import_run SET report = report || $2, updated_at = now() WHERE id = $1 RETURNING id",
  )
  .bind::<BigInt, _>(run_id)
  .bind::<diesel::sql_types::Jsonb, _>(report)
  .get_result::<Updated>(conn)
  .await
  .map(|updated| updated.id > 0)
  .map_err(Into::into)
}

/// Get or create the run identified by both its source bytes and target site.
pub async fn get_or_create_run(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  fingerprint: &str,
  source_version: &str,
  schema_version: i64,
) -> LemmyResult<DiscourseImportRun> {
  if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  diesel::sql_query(
    "INSERT INTO discourse_import_run (local_site_id, source_fingerprint_sha256, source_version, source_schema_version) \
     VALUES ($1, lower($2), $3, $4) \
     ON CONFLICT (local_site_id, source_fingerprint_sha256) DO UPDATE SET updated_at = now() \
       WHERE discourse_import_run.source_version = EXCLUDED.source_version \
         AND discourse_import_run.source_schema_version = EXCLUDED.source_schema_version \
     RETURNING id, status",
  )
  .bind::<Integer, _>(site_id.0)
  .bind::<Text, _>(fingerprint)
  .bind::<Text, _>(source_version)
  .bind::<BigInt, _>(schema_version)
  .get_result(conn)
  .await
  .map_err(Into::into)
}

/// Atomically advance a resumable phase cursor and cumulative count.
pub async fn update_phase(
  pool: &mut DbPool<'_>,
  run_id: i64,
  phase: &str,
  last_source_id: Option<&str>,
  status: &str,
  processed_count: i64,
) -> LemmyResult<()> {
  if !matches!(status, "pending" | "running" | "failed" | "completed") || processed_count < 0 {
    return Err(LemmyErrorType::InvalidName.into());
  }
  let conn = &mut get_conn(pool).await?;
  diesel::sql_query(
    "INSERT INTO discourse_import_phase (run_id, phase, last_source_id, status, processed_count) \
     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (run_id, phase) DO UPDATE SET \
       last_source_id = EXCLUDED.last_source_id, status = EXCLUDED.status, \
       processed_count = EXCLUDED.processed_count, updated_at = now()",
  )
  .bind::<BigInt, _>(run_id)
  .bind::<Text, _>(phase)
  .bind::<diesel::sql_types::Nullable<Text>, _>(last_source_id)
  .bind::<Text, _>(status)
  .bind::<BigInt, _>(processed_count)
  .execute(conn)
  .await?;
  Ok(())
}

/// Persist a stable source-to-target mapping. Replays are accepted only when they agree.
pub async fn record_mapping(
  pool: &mut DbPool<'_>,
  run_id: i64,
  entity_type: &str,
  source_id: &str,
  target_id: i64,
) -> LemmyResult<()> {
  let conn = &mut get_conn(pool).await?;
  conn.transaction::<(), lemmy_utils::error::LemmyError, _>(async |conn| {
    diesel::sql_query(
      "INSERT INTO discourse_import_mapping (run_id, entity_type, source_id, target_id) \
       VALUES ($1, $2, $3, $4) ON CONFLICT (run_id, entity_type, source_id) DO NOTHING",
    )
    .bind::<BigInt, _>(run_id)
    .bind::<Text, _>(entity_type)
    .bind::<Text, _>(source_id)
    .bind::<BigInt, _>(target_id)
    .execute(conn)
    .await?;
    #[derive(QueryableByName)]
    struct Existing {
      #[diesel(sql_type = BigInt)]
      target_id: i64,
    }
    let existing = diesel::sql_query(
      "SELECT target_id FROM discourse_import_mapping WHERE run_id = $1 AND entity_type = $2 AND source_id = $3 FOR UPDATE",
    )
    .bind::<BigInt, _>(run_id)
    .bind::<Text, _>(entity_type)
    .bind::<Text, _>(source_id)
    .get_result::<Existing>(conn)
    .await?;
    if existing.target_id != target_id {
      return Err(LemmyErrorType::Unknown(format!(
        "source mapping conflict for {entity_type} {source_id}: existing target {}, requested target {target_id}",
        existing.target_id
      ))
      .into());
    }
    Ok(())
  })
  .await
}

/// Read an existing source mapping before resuming an entity phase.
pub async fn read_mapping(
  pool: &mut DbPool<'_>,
  run_id: i64,
  entity_type: &str,
  source_id: &str,
) -> LemmyResult<Option<i64>> {
  #[derive(QueryableByName)]
  struct Mapping {
    #[diesel(sql_type = BigInt)]
    target_id: i64,
  }
  let conn = &mut get_conn(pool).await?;
  diesel::sql_query(
    "SELECT target_id FROM discourse_import_mapping WHERE run_id = $1 AND entity_type = $2 AND source_id = $3",
  )
  .bind::<BigInt, _>(run_id)
  .bind::<Text, _>(entity_type)
  .bind::<Text, _>(source_id)
  .get_result::<Mapping>(conn)
  .await
  .optional()
  .map(|mapping| mapping.map(|row| row.target_id))
  .map_err(Into::into)
}
