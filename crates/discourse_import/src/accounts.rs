use anyhow::{Context, Result, anyhow, bail};
use aws_sdk_s3::{
  Client as S3Client,
  config::{Credentials, Region},
  primitives::ByteStream,
};
use chrono::{DateTime, Utc};
use diesel::{
  OptionalExtension, QueryableByName,
  sql_types::{Array, BigInt, Bool, Integer, Nullable, Text},
};
use diesel_async::{AsyncConnection, RunQueryDsl, pg::AsyncPgConnection};
use lemmy_db_schema::{
  impls::discourse_import::{
    create_imported_account, create_imported_bookmark, create_imported_comment,
    create_imported_community, create_imported_community_tag, create_imported_multi_community,
    create_imported_post, create_imported_private_message, create_imported_vote,
    ensure_imported_private_message_notification, get_or_create_run, phase_completed, read_mapping,
    record_mapping, set_run_status, store_run_report, update_phase,
  },
  source::{
    actor_language::SiteLanguage,
    comment::{Comment, CommentInsertForm},
    community::{Community, CommunityInsertForm},
    community_tag::{CommunityTag, CommunityTagInsertForm, PostCommunityTag},
    file::{FileObject, FileObjectInsertForm, FileReference, FileReferenceInsertForm},
    local_user::LocalUserInsertForm,
    media::{MediaObject, MediaObjectInsertForm, MediaReference, MediaReferenceInsertForm},
    multi_community::{
      MultiCommunity, MultiCommunityEntry, MultiCommunityEntryForm, MultiCommunityInsertForm,
    },
    person::{Person, PersonInsertForm},
    post::{Post, PostInsertForm},
    private_message::PrivateMessageInsertForm,
  },
  traits::ApubActor,
};
use lemmy_db_schema_file::{
  InstanceId,
  enums::TagColor,
  newtypes::{
    CommentId, CommunityId, LocalSiteId, MultiCommunityId, PostId, PrivateMessageId, SiteId,
  },
};
use lemmy_diesel_utils::{
  connection::{DbPool, build_db_pool, get_conn},
  dburl::DbUrl,
  traits::Crud,
};
use lemmy_utils::{settings::SETTINGS, utils::validation::is_valid_local_actor_name};
use serde::Serialize;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};
use std::{
  collections::{BTreeMap, HashMap, HashSet},
  path::Path,
};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccountConflictPolicy {
  Report,
  Rename,
  Skip,
}

#[derive(Serialize)]
pub struct AccountImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_user_count: u64,
  pub imported_user_count: u64,
  pub resumed_user_count: u64,
  pub skipped_user_count: u64,
  pub users_without_password_count: u64,
  pub inactive_or_unapproved_users_count: u64,
  pub suspended_users_count: u64,
  pub silenced_users_count: u64,
  pub staged_users_count: u64,
  pub privileged_users_count: u64,
  pub users_with_profile_privacy_restrictions_count: u64,
  pub conflict_policy: &'static str,
  pub import_run_status: &'static str,
  pub warnings: Vec<String>,
  pub conflicts: Vec<String>,
}

#[derive(Serialize)]
pub struct AccountConflictPreviewReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_user_count: usize,
  pub conflict_policy: &'static str,
  pub unresolved_conflict_count: usize,
  pub conflicts: Vec<AccountIdentityConflict>,
}

#[derive(Serialize)]
pub struct AccountIdentityConflict {
  pub source_user_id: i32,
  pub source_username: String,
  pub normalized_username: String,
  pub username_collision: bool,
  pub username_alias_collision: bool,
  pub resolved_username: Option<String>,
  pub source_email: Option<String>,
  pub email_collision: bool,
  pub email_action: &'static str,
  pub blocks_import: bool,
}

#[derive(QueryableByName)]
struct SourceAccountIdentity {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Text)]
  username: String,
  #[diesel(sql_type = Nullable<Text>)]
  email: Option<String>,
}

#[derive(QueryableByName)]
struct ImportRunId {
  #[diesel(sql_type = BigInt)]
  id: i64,
}

#[derive(QueryableByName)]
struct MappedPersonName {
  #[diesel(sql_type = Text)]
  name: String,
}

#[derive(Serialize)]
pub struct CategoryImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_category_count: usize,
  pub imported_community_count: usize,
  pub resumed_community_count: usize,
  pub imported_multi_community_count: usize,
  pub resumed_multi_community_count: usize,
  pub restricted_category_ids: Vec<i32>,
  pub unresolved_category_ids: Vec<i32>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct DiscussionImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_post_count: usize,
  pub imported_topic_count: usize,
  pub imported_reply_count: usize,
  pub resumed_object_count: usize,
  pub unresolved_source_post_ids: Vec<i32>,
  pub historical_polls: Vec<HistoricalPollReport>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct PrivateMessageImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_pm_post_count: usize,
  pub source_chat_message_count: usize,
  pub imported_pm_message_count: usize,
  pub resumed_pm_message_count: usize,
  pub unresolved_source_pm_post_ids: Vec<i32>,
  pub pairwise_group_topic_ids: Vec<i32>,
  pub pairwise_group_chat_channel_ids: Vec<i64>,
  pub unresolved_chat_message_ids: Vec<i64>,
  pub source_chat_messages_with_attachments: usize,
  pub source_chat_messages_with_structured_blocks: usize,
  pub warnings: Vec<String>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct LegacyUrlExportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub generated_entry_count: usize,
  pub unmapped_source_post_ids: Vec<i32>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct InteractionImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_active_like_count: usize,
  pub imported_vote_count: usize,
  pub resumed_vote_count: usize,
  pub source_bookmark_count: usize,
  pub imported_bookmark_count: usize,
  pub resumed_bookmark_count: usize,
  pub unsupported_bookmark_count: usize,
  pub unsupported_action_count: usize,
  pub unresolved_interaction_ids: Vec<i64>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct TagImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_topic_tag_count: usize,
  pub imported_tag_count: usize,
  pub resumed_tag_count: usize,
  pub imported_post_tag_count: usize,
  pub unresolved_topic_ids: Vec<i32>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct MediaImportReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_upload_count: usize,
  pub archived_upload_file_count: usize,
  pub imported_image_count: usize,
  pub resumed_image_count: usize,
  pub imported_attachment_count: usize,
  pub resumed_attachment_count: usize,
  pub retained_secure_upload_ids: Vec<i32>,
  pub retained_non_image_upload_ids: Vec<i32>,
  pub retained_upload_manifest: Vec<RetainedUpload>,
  pub missing_archive_upload_ids: Vec<i32>,
  pub invalid_upload_ids: Vec<i32>,
  pub unresolved_content_source_post_ids: Vec<i32>,
  pub unresolved_private_message_source_post_ids: Vec<i32>,
  pub rewritten_avatar_count: usize,
  pub unresolved_avatar_source_user_ids: Vec<i32>,
  pub rewritten_category_icon_count: usize,
  pub rewritten_category_banner_count: usize,
  pub unresolved_category_image_ids: Vec<i32>,
  pub rewritten_source_post_count: usize,
  pub rewritten_media_reference_count: usize,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct ImportValidationReport {
  pub target_site_id: i32,
  pub source_fingerprint_sha256: String,
  pub source_counts: BTreeMap<String, i64>,
  pub mapped_counts: BTreeMap<String, i64>,
  pub archived_upload_file_count: usize,
  pub invalid_target_reference_counts: BTreeMap<String, i64>,
  pub blocking_reasons: Vec<String>,
  pub import_run_status: &'static str,
}

#[derive(Serialize)]
pub struct HistoricalPollReport {
  pub source_poll_id: i64,
  pub source_post_id: i64,
  pub prompt: String,
  pub status: i32,
  pub results_visibility: i32,
  pub is_closed: bool,
  pub show_public_results: bool,
  pub options: Vec<HistoricalPollOption>,
}

#[derive(Serialize)]
pub struct HistoricalPollOption {
  pub source_option_id: i64,
  pub label: String,
  pub votes: Vec<HistoricalPollVote>,
}

#[derive(Serialize)]
pub struct HistoricalPollVote {
  pub source_user_id: Option<i64>,
  pub rank: i32,
  pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct RetainedUpload {
  pub source_upload_id: i32,
  pub original_filename: String,
  pub source_url: String,
  pub archive_member: Option<String>,
  pub filesize_bytes: i64,
  pub reason: &'static str,
}

#[derive(QueryableByName)]
struct SourceUser {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Text)]
  username: String,
  #[diesel(sql_type = Nullable<Text>)]
  display_name: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  bio: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  location: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  website: Option<String>,
  #[diesel(sql_type = Nullable<Bool>)]
  allow_private_messages: Option<bool>,
  #[diesel(sql_type = Nullable<Bool>)]
  external_links_in_new_tab: Option<bool>,
  #[diesel(sql_type = Nullable<Bool>)]
  hide_profile_and_presence: Option<bool>,
  #[diesel(sql_type = Nullable<Bool>)]
  hide_profile: Option<bool>,
  #[diesel(sql_type = Nullable<Bool>)]
  hide_presence: Option<bool>,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  updated_at: Option<DateTime<Utc>>,
  #[diesel(sql_type = Nullable<Text>)]
  email: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  password_hash: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  password_salt: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  password_algorithm: Option<String>,
  #[diesel(sql_type = Bool)]
  active: bool,
  #[diesel(sql_type = Bool)]
  approved: bool,
  #[diesel(sql_type = Bool)]
  admin: bool,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  suspended_till: Option<DateTime<Utc>>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  silenced_till: Option<DateTime<Utc>>,
  #[diesel(sql_type = Bool)]
  staged: bool,
  #[diesel(sql_type = Nullable<Bool>)]
  moderator: Option<bool>,
}

#[derive(QueryableByName)]
struct TargetSite {
  #[diesel(sql_type = Integer)]
  site_id: i32,
  #[diesel(sql_type = Integer)]
  instance_id: i32,
  #[diesel(sql_type = Text)]
  hostname: String,
  #[diesel(sql_type = Bool)]
  federation_enabled: bool,
  #[diesel(sql_type = Bool)]
  file_upload_disabled: bool,
  #[diesel(sql_type = BigInt)]
  file_max_upload_size_bytes: i64,
  #[diesel(sql_type = Array<Text>)]
  file_allowed_mime_types: Vec<String>,
}

#[derive(QueryableByName, Clone)]
struct SourceCategory {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Text)]
  name: String,
  #[diesel(sql_type = Nullable<Text>)]
  slug: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  description: Option<String>,
  #[diesel(sql_type = Nullable<Integer>)]
  parent_category_id: Option<i32>,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  updated_at: Option<DateTime<Utc>>,
  #[diesel(sql_type = Bool)]
  read_restricted: bool,
  #[diesel(sql_type = Bool)]
  has_group_permissions: bool,
  #[diesel(sql_type = BigInt)]
  topic_count: i64,
}

#[derive(QueryableByName)]
struct Creator {
  #[diesel(sql_type = Integer)]
  person_id: i32,
}

#[derive(QueryableByName)]
struct SourceDiscussionPost {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  post_number: i32,
  #[diesel(sql_type = Nullable<Integer>)]
  reply_to_post_number: Option<i32>,
  #[diesel(sql_type = Nullable<Integer>)]
  user_id: Option<i32>,
  #[diesel(sql_type = Text)]
  raw: String,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  updated_at: Option<DateTime<Utc>>,
  #[diesel(sql_type = Bool)]
  deleted: bool,
  #[diesel(sql_type = Nullable<Text>)]
  title: Option<String>,
  #[diesel(sql_type = Nullable<Integer>)]
  category_id: Option<i32>,
  #[diesel(sql_type = Nullable<Bool>)]
  topic_closed: Option<bool>,
}

#[derive(QueryableByName)]
struct SourceDiscussionRelation {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  user_id: i32,
  #[diesel(sql_type = Nullable<Integer>)]
  category_id: Option<i32>,
  #[diesel(sql_type = Integer)]
  post_number: i32,
  #[diesel(sql_type = Nullable<Integer>)]
  reply_to_post_number: Option<i32>,
  #[diesel(sql_type = Nullable<Integer>)]
  parent_source_post_id: Option<i32>,
}

#[derive(QueryableByName)]
struct SourceCategoryLink {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Nullable<Integer>)]
  parent_category_id: Option<i32>,
}

#[derive(QueryableByName)]
struct TargetMultiCommunityEntry {
  #[diesel(sql_type = Integer)]
  multi_community_id: i32,
  #[diesel(sql_type = Integer)]
  community_id: i32,
}

/// Read-only preflight for username and primary-email collisions against one target site.
pub async fn preview_account_conflicts(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  conflict_policy: AccountConflictPolicy,
) -> Result<AccountConflictPreviewReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  load_target_site(pool, site_id).await?;

  let run = diesel::sql_query(
    "SELECT id FROM discourse_import_run \
     WHERE local_site_id = $1 AND source_fingerprint_sha256 = $2",
  )
  .bind::<Integer, _>(site_id.0)
  .bind::<Text, _>(source_fingerprint)
  .get_result::<ImportRunId>(&mut get_conn(pool).await?)
  .await
  .optional()
  .context("failed to inspect an existing import run for preview")?;
  let mut reserved_names = HashSet::new();
  let mut conflicts = Vec::new();
  let mut unresolved_conflict_count = 0;
  let mut source_user_count = 0;
  let mut last_id = 0_i32;
  loop {
    let identities = diesel::sql_query(
      "SELECT u.id, u.username, ue.email FROM users u \
       LEFT JOIN user_emails ue ON ue.user_id = u.id AND ue.primary = TRUE \
       WHERE u.id > $1 ORDER BY u.id LIMIT 500",
    )
    .bind::<Integer, _>(last_id)
    .load::<SourceAccountIdentity>(&mut source)
    .await
    .context("failed to read source account identities for preview")?;
    if identities.is_empty() {
      break;
    }
    for identity in identities {
      last_id = identity.id;
      source_user_count += 1;
      if let Some(run) = &run
        && let Some(target_id) = read_mapping(pool, run.id, "user", &identity.id.to_string())
          .await
          .map_err(convert_lemmy_error)?
      {
        reserved_names.insert(read_mapped_person_name(pool, target_id).await?);
        continue;
      }

      let normalized_username = normalize_username(&identity.username, identity.id);
      let username_collision =
        username_taken(pool, site_id, &normalized_username, &reserved_names).await?;
      let mut username_alias_collision = false;
      let resolved_username = if username_collision {
        match conflict_policy {
          AccountConflictPolicy::Report | AccountConflictPolicy::Skip => None,
          AccountConflictPolicy::Rename => {
            let alias = renamed_username(&normalized_username, identity.id)?;
            username_alias_collision =
              username_taken(pool, site_id, &alias, &reserved_names).await?;
            (!username_alias_collision).then_some(alias)
          }
        }
      } else {
        Some(normalized_username.clone())
      };
      if let Some(name) = &resolved_username {
        reserved_names.insert(name.clone());
      }
      let email_collision = match identity.email.as_deref() {
        Some(email) => email_taken(pool, site_id, email).await?,
        None => false,
      };
      let blocks_import = username_alias_collision
        || ((username_collision || email_collision)
          && !matches!(conflict_policy, AccountConflictPolicy::Rename));
      if blocks_import {
        unresolved_conflict_count += 1;
      }
      if username_collision || email_collision {
        conflicts.push(AccountIdentityConflict {
          source_user_id: identity.id,
          source_username: identity.username,
          normalized_username,
          username_collision,
          username_alias_collision,
          resolved_username,
          source_email: identity.email.clone(),
          email_collision,
          email_action: if email_collision
            && matches!(conflict_policy, AccountConflictPolicy::Rename)
          {
            "omit_conflicting_email"
          } else if identity.email.is_some() {
            "keep_source_email"
          } else {
            "no_source_email"
          },
          blocks_import,
        });
      }
    }
  }

  Ok(AccountConflictPreviewReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_user_count,
    conflict_policy: match conflict_policy {
      AccountConflictPolicy::Report => "report",
      AccountConflictPolicy::Rename => "rename",
      AccountConflictPolicy::Skip => "skip",
    },
    unresolved_conflict_count,
    conflicts,
  })
}

pub async fn import_accounts(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
  conflict_policy: AccountConflictPolicy,
) -> Result<AccountImportReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool()
    .map_err(convert_lemmy_error)
    .context("failed to build the LemmyMultisite target pool")?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing Discourse accounts");
  }
  let protocol_and_hostname = format!(
    "{}://{}",
    SETTINGS.get_protocol_string(),
    target_site.hostname
  );

  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)
  .context("failed to create or resume the source archive ledger")?;
  if run.status == "completed" {
    bail!("this archive has already completed for the selected target site");
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;
  let site_languages = SiteLanguage::read(pool, SiteId(target_site.site_id))
    .await
    .map_err(convert_lemmy_error)?;
  let mut report = AccountImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_user_count: 0,
    imported_user_count: 0,
    resumed_user_count: 0,
    skipped_user_count: 0,
    users_without_password_count: 0,
    inactive_or_unapproved_users_count: 0,
    suspended_users_count: 0,
    silenced_users_count: 0,
    staged_users_count: 0,
    privileged_users_count: 0,
    users_with_profile_privacy_restrictions_count: 0,
    conflict_policy: match conflict_policy {
      AccountConflictPolicy::Report => "report",
      AccountConflictPolicy::Rename => "rename",
      AccountConflictPolicy::Skip => "skip",
    },
    import_run_status: "running",
    warnings: Vec::new(),
    conflicts: Vec::new(),
  };
  let mut last_id = 0_i32;
  let mut reserved_names = HashSet::new();
  loop {
    let batch = diesel::sql_query(
      "SELECT u.id, u.username, u.name AS display_name, up.bio_raw AS bio, \
       up.location, up.website, \
       uo.allow_private_messages, uo.external_links_in_new_tab, \
       uo.hide_profile_and_presence, uo.hide_profile, uo.hide_presence, \
       u.created_at AT TIME ZONE 'UTC' AS created_at, \
       u.updated_at AT TIME ZONE 'UTC' AS updated_at, ue.email, \
       upw.password_hash, upw.password_salt, upw.password_algorithm, \
       u.active, u.approved, u.admin, u.suspended_till AT TIME ZONE 'UTC' AS suspended_till, \
       u.silenced_till AT TIME ZONE 'UTC' AS silenced_till, u.staged, u.moderator \
       FROM users u LEFT JOIN user_profiles up ON up.user_id = u.id \
       LEFT JOIN user_options uo ON uo.user_id = u.id \
       LEFT JOIN user_emails ue ON ue.user_id = u.id AND ue.primary = TRUE \
       LEFT JOIN user_passwords upw ON upw.user_id = u.id AND upw.password_expired_at IS NULL \
       WHERE u.id > $1 ORDER BY u.id LIMIT 500",
    )
    .bind::<Integer, _>(last_id)
    .load::<SourceUser>(&mut source)
    .await
    .context("failed to read users from the restored Discourse database")?;
    if batch.is_empty() {
      break;
    }
    for user in batch {
      last_id = user.id;
      report.source_user_count += 1;
      if !user.active || !user.approved {
        report.inactive_or_unapproved_users_count += 1;
        report.warnings.push(format!(
          "user {} was inactive or unapproved in Discourse; the imported person is marked deleted so login remains blocked independently of target-site settings",
          user.id
        ));
      }
      if user.suspended_till.is_some_and(|until| until > Utc::now()) {
        report.suspended_users_count += 1;
        report.warnings.push(format!(
          "user {} was suspended in Discourse; Lemmy has no temporary local-user suspension, so review target account access",
          user.id
        ));
      }
      if user.silenced_till.is_some_and(|until| until > Utc::now()) {
        report.silenced_users_count += 1;
        report.warnings.push(format!(
          "user {} was silenced in Discourse; Lemmy has no equivalent posting restriction",
          user.id
        ));
      }
      if user.staged {
        report.staged_users_count += 1;
        report.warnings.push(format!(
          "user {} was a staged Discourse account; review whether it should remain usable",
          user.id
        ));
      }
      if user.admin || user.moderator.unwrap_or(false) {
        report.privileged_users_count += 1;
        report.warnings.push(format!(
          "user {} had a Discourse admin or moderator role; no Lemmy privileges were granted",
          user.id
        ));
      }
      if user.hide_profile_and_presence.unwrap_or(false)
        || user.hide_profile.unwrap_or(false)
        || user.hide_presence.unwrap_or(false)
      {
        report.users_with_profile_privacy_restrictions_count += 1;
        report.warnings.push(format!(
          "user {} hid profile details or presence in Discourse; Lemmy has no equivalent local-user visibility setting",
          user.id
        ));
      }
      if let Some(target_id) = read_mapping(pool, run.id, "user", &user.id.to_string())
        .await
        .map_err(convert_lemmy_error)?
      {
        reserved_names.insert(read_mapped_person_name(pool, target_id).await?);
        report.resumed_user_count += 1;
        update_phase(
          pool,
          run.id,
          "users",
          Some(&user.id.to_string()),
          "running",
          (report.imported_user_count + report.resumed_user_count + report.skipped_user_count)
            as i64,
        )
        .await
        .map_err(convert_lemmy_error)?;
        continue;
      }
      let normalized = normalize_username(&user.username, user.id);
      let candidate = match resolve_username(
        pool,
        site_id,
        &normalized,
        user.id,
        conflict_policy,
        &mut reserved_names,
      )
      .await?
      {
        Some(name) => name,
        None => {
          report.skipped_user_count += 1;
          report.conflicts.push(format!(
            "user {} username {:?} conflicts with an existing account",
            user.id, user.username
          ));
          continue;
        }
      };
      let email = match user.email.as_deref() {
        Some(value) if email_taken(pool, site_id, value).await? => match conflict_policy {
          AccountConflictPolicy::Rename => {
            report.conflicts.push(format!(
              "user {} email was omitted because it conflicts with a target account",
              user.id
            ));
            None
          }
          AccountConflictPolicy::Report | AccountConflictPolicy::Skip => {
            report.skipped_user_count += 1;
            report.conflicts.push(format!(
              "user {} email conflicts with a target account",
              user.id
            ));
            continue;
          }
        },
        Some(value) => Some(value.to_lowercase()),
        None => None,
      };
      let password_verifier = match (
        user.password_algorithm.as_deref(),
        user.password_salt.as_deref(),
        user.password_hash.as_deref(),
      ) {
        (Some(algorithm), Some(salt), Some(digest)) => {
          lemmy_db_schema::impls::password::encode_discourse_pbkdf2_verifier(
            algorithm, salt, digest,
          )
        }
        _ => None,
      };
      if password_verifier.is_none() {
        report.users_without_password_count += 1;
        if user.password_hash.is_some()
          || user.password_salt.is_some()
          || user.password_algorithm.is_some()
        {
          report.warnings.push(format!(
            "user {} has a password verifier format unsupported by this importer; password login requires account recovery",
            user.id
          ));
        }
      }
      let keypair = activitypub_federation::http_signatures::generate_actor_keypair()
        .context("failed to generate a target actor keypair")?;
      let actor_id = Person::generate_local_actor_url(&candidate, &protocol_and_hostname)
        .map_err(convert_lemmy_error)?;
      let inbox_url: DbUrl = Url::parse(&format!("{protocol_and_hostname}/inbox"))?.into();
      let mut person_form = PersonInsertForm::new(
        candidate.clone(),
        keypair.public_key,
        InstanceId(target_site.instance_id),
      );
      // Login checks person deletion independently of target-wide registration
      // and email-verification settings, so this preserves the source account
      // gate without changing settings for existing target users.
      person_form.deleted = Some(!user.active || !user.approved);
      person_form.display_name = user.display_name;
      person_form.bio =
        merge_profile_fields(user.bio, user.location.as_deref(), user.website.as_deref());
      person_form.published_at = Some(user.created_at);
      person_form.updated_at = user.updated_at;
      person_form.ap_id = Some(actor_id);
      person_form.local = Some(true);
      person_form.private_key = Some(keypair.private_key);
      person_form.inbox_url = Some(inbox_url);
      person_form.origin_local_site_id = Some(site_id);
      let mut local_user_form = LocalUserInsertForm::new(
        lemmy_db_schema_file::PersonId(0),
        site_id,
        password_verifier,
      );
      local_user_form.email = email;
      local_user_form.email_verified = Some(local_user_form.email.is_some() && user.active);
      local_user_form.accepted_application = Some(user.approved);
      local_user_form.private_messages_enabled = Some(user.allow_private_messages.unwrap_or(true));
      local_user_form.open_links_in_new_tab = Some(user.external_links_in_new_tab.unwrap_or(false));
      create_imported_account(
        pool,
        run.id,
        &user.id.to_string(),
        site_id,
        &person_form,
        &local_user_form,
        site_languages.clone(),
      )
      .await
      .map_err(convert_lemmy_error)
      .with_context(|| format!("failed to import Discourse user {}", user.id))?;
      reserved_names.insert(candidate);
      report.imported_user_count += 1;
      update_phase(
        pool,
        run.id,
        "users",
        Some(&user.id.to_string()),
        "running",
        (report.imported_user_count + report.resumed_user_count + report.skipped_user_count) as i64,
      )
      .await
      .map_err(convert_lemmy_error)?;
    }
  }
  // With the explicit Skip policy, unresolved accounts are intentionally omitted;
  // dependent imports can then skip objects owned by those accounts.
  let had_unresolved_users =
    report.skipped_user_count > 0 && conflict_policy != AccountConflictPolicy::Skip;
  update_phase(
    pool,
    run.id,
    "users",
    Some(&last_id.to_string()),
    if had_unresolved_users {
      "failed"
    } else {
      "completed"
    },
    (report.imported_user_count + report.resumed_user_count + report.skipped_user_count) as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  let durable_report = serde_json::to_value(&report)?;
  let durable_report = serde_json::json!({"accounts": durable_report});
  store_run_report(pool, run.id, &durable_report)
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(
    pool,
    run.id,
    if had_unresolved_users {
      "failed"
    } else {
      "running"
    },
  )
  .await
  .map_err(convert_lemmy_error)?;
  Ok(report)
}

pub async fn import_categories(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
) -> Result<CategoryImportReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing Discourse categories");
  }
  let protocol_and_hostname = format!(
    "{}://{}",
    SETTINGS.get_protocol_string(),
    target_site.hostname
  );
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if run.status == "completed" {
    bail!("this archive has already completed for the selected target site");
  }
  if !phase_completed(pool, run.id, "users")
    .await
    .map_err(convert_lemmy_error)?
  {
    bail!(
      "import accounts first and resolve all blocking account conflicts before importing categories"
    );
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;

  let creator = load_multi_community_creator(pool, site_id).await?;
  let categories = diesel::sql_query(
    "SELECT c.id, c.name, c.slug, c.description, c.parent_category_id, \
     c.created_at AT TIME ZONE 'UTC' AS created_at, \
     c.updated_at AT TIME ZONE 'UTC' AS updated_at, c.read_restricted, \
     EXISTS (SELECT 1 FROM category_groups cg WHERE cg.category_id = c.id) AS has_group_permissions, \
     (SELECT count(*) FROM topics t WHERE t.category_id = c.id AND t.deleted_at IS NULL) AS topic_count \
     FROM categories c ORDER BY c.parent_category_id NULLS FIRST, c.id",
  )
  .load::<SourceCategory>(&mut source)
  .await
  .context("failed to read Discourse categories")?;
  let mut report = CategoryImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_category_count: categories.len(),
    imported_community_count: 0,
    resumed_community_count: 0,
    imported_multi_community_count: 0,
    resumed_multi_community_count: 0,
    restricted_category_ids: categories
      .iter()
      .filter(|category| category.read_restricted || category.has_group_permissions)
      .map(|category| category.id)
      .collect(),
    unresolved_category_ids: Vec::new(),
    import_run_status: "running",
  };
  let child_counts = categories
    .iter()
    .filter_map(|category| category.parent_category_id)
    .fold(
      std::collections::HashMap::<i32, usize>::new(),
      |mut counts, parent| {
        *counts.entry(parent).or_default() += 1;
        counts
      },
    );
  let mut communities = std::collections::HashMap::<i32, CommunityId>::new();
  let mut reserved_community_names = HashSet::new();
  for category in &categories {
    let is_parent = child_counts.get(&category.id).copied().unwrap_or_default() > 0;
    let needs_community = !is_parent || category.topic_count > 0;
    if !needs_community {
      continue;
    }
    let source_id = category.id.to_string();
    if let Some(target_id) = read_mapping(pool, run.id, "category_community", &source_id)
      .await
      .map_err(convert_lemmy_error)?
    {
      let community = Community::read(pool, CommunityId(target_id as i32))
        .await
        .map_err(convert_lemmy_error)?;
      communities.insert(category.id, community.id);
      reserved_community_names.insert(community.name.clone());
      report.resumed_community_count += 1;
      continue;
    }
    let slug = category
      .slug
      .as_deref()
      .filter(|slug| !slug.trim().is_empty())
      .unwrap_or(&category.name);
    let name =
      resolve_community_name(pool, site_id, slug, category.id, &reserved_community_names).await?;
    let keypair = activitypub_federation::http_signatures::generate_actor_keypair()
      .context("failed to generate a community actor keypair")?;
    let actor_id = Community::generate_local_actor_url(&name, &protocol_and_hostname)
      .map_err(convert_lemmy_error)?;
    let inbox_url: DbUrl = Url::parse(&format!("{protocol_and_hostname}/inbox"))?.into();
    let mut form = CommunityInsertForm::new(
      InstanceId(target_site.instance_id),
      name.clone(),
      keypair.public_key,
    );
    form.title = Some(truncate_chars(&category.name, 50));
    form.summary = category
      .description
      .as_deref()
      .map(|summary| truncate_chars(summary, 150));
    form.published_at = Some(category.created_at);
    form.updated_at = category.updated_at;
    form.ap_id = Some(actor_id);
    form.local = Some(true);
    form.private_key = Some(keypair.private_key);
    form.inbox_url = Some(inbox_url);
    form.origin_local_site_id = Some(site_id);
    let community = create_imported_community(pool, run.id, &source_id, site_id, &form)
      .await
      .map_err(convert_lemmy_error)
      .with_context(|| format!("failed to import Discourse category {}", category.id))?;
    communities.insert(category.id, community.id);
    reserved_community_names.insert(name);
    report.imported_community_count += 1;
    update_phase(
      pool,
      run.id,
      "categories_communities",
      Some(&source_id),
      "running",
      (report.imported_community_count + report.resumed_community_count) as i64,
    )
    .await
    .map_err(convert_lemmy_error)?;
  }

  let mut reserved_multi_names = HashSet::new();
  for category in &categories {
    if child_counts.get(&category.id).copied().unwrap_or_default() == 0 {
      continue;
    }
    let source_id = category.id.to_string();
    let (multi, resumed) = if let Some(target_id) =
      read_mapping(pool, run.id, "category_multi_community", &source_id)
        .await
        .map_err(convert_lemmy_error)?
    {
      (
        MultiCommunity::read(pool, MultiCommunityId(target_id as i32))
          .await
          .map_err(convert_lemmy_error)?,
        true,
      )
    } else {
      let slug = category
        .slug
        .as_deref()
        .filter(|slug| !slug.trim().is_empty())
        .unwrap_or(&category.name);
      let name =
        resolve_multi_community_name(pool, site_id, slug, category.id, &reserved_multi_names)
          .await?;
      let keypair = activitypub_federation::http_signatures::generate_actor_keypair()
        .context("failed to generate a multi-community actor keypair")?;
      let actor_id = MultiCommunity::generate_local_actor_url(&name, &protocol_and_hostname)
        .map_err(convert_lemmy_error)?;
      let inbox_url: DbUrl = Url::parse(&format!("{protocol_and_hostname}/inbox"))?.into();
      let mut form = MultiCommunityInsertForm::new(
        lemmy_db_schema_file::PersonId(creator.person_id),
        InstanceId(target_site.instance_id),
        name.clone(),
        keypair.public_key,
      );
      form.title = Some(truncate_chars(&category.name, 50));
      form.summary = category
        .description
        .as_deref()
        .map(|summary| truncate_chars(summary, 150));
      form.ap_id = Some(actor_id);
      form.local = Some(true);
      form.private_key = Some(keypair.private_key.into());
      form.inbox_url = Some(inbox_url);
      form.origin_local_site_id = Some(site_id);
      let inserted = create_imported_multi_community(pool, run.id, &source_id, site_id, &form)
        .await
        .map_err(convert_lemmy_error)
        .with_context(|| format!("failed to import parent category {}", category.id))?;
      reserved_multi_names.insert(name);
      report.imported_multi_community_count += 1;
      (inserted, false)
    };
    if resumed {
      report.resumed_multi_community_count += 1;
    }
    let descendants = descendant_category_ids(category.id, &categories);
    let descendant_communities = descendants
      .iter()
      .filter_map(|category_id| communities.get(category_id).copied())
      .collect::<Vec<_>>();
    if descendant_communities.len() >= 50 {
      report.unresolved_category_ids.push(category.id);
      continue;
    }
    for community_id in descendant_communities {
      if multi_entry_exists(pool, multi.id, community_id).await? {
        continue;
      }
      MultiCommunityEntry::create(pool, &MultiCommunityEntryForm::new(multi.id, community_id))
        .await
        .map_err(convert_lemmy_error)?;
    }
    update_phase(
      pool,
      run.id,
      "categories_multi_communities",
      Some(&source_id),
      "running",
      report.imported_multi_community_count as i64,
    )
    .await
    .map_err(convert_lemmy_error)?;
  }
  let unresolved = !report.unresolved_category_ids.is_empty();
  update_phase(
    pool,
    run.id,
    "categories_communities",
    categories
      .last()
      .map(|category| category.id.to_string())
      .as_deref(),
    if unresolved { "failed" } else { "completed" },
    (report.imported_community_count + report.resumed_community_count) as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  update_phase(
    pool,
    run.id,
    "categories_multi_communities",
    categories
      .last()
      .map(|category| category.id.to_string())
      .as_deref(),
    if unresolved { "failed" } else { "completed" },
    report.imported_multi_community_count as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  let durable_report = serde_json::json!({"categories": report});
  store_run_report(pool, run.id, &durable_report)
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, if unresolved { "failed" } else { "running" })
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

pub async fn import_discussions(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
) -> Result<DiscussionImportReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing Discourse discussions");
  }
  let protocol_and_hostname = format!(
    "{}://{}",
    SETTINGS.get_protocol_string(),
    target_site.hostname
  );
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if run.status == "completed" {
    bail!("this archive has already completed for the selected target site");
  }
  if !phase_completed(pool, run.id, "users")
    .await
    .map_err(convert_lemmy_error)?
    || !phase_completed(pool, run.id, "categories_multi_communities")
      .await
      .map_err(convert_lemmy_error)?
  {
    bail!("import accounts and categories before importing discussions");
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;

  let mut report = DiscussionImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_post_count: 0,
    imported_topic_count: 0,
    imported_reply_count: 0,
    resumed_object_count: 0,
    unresolved_source_post_ids: Vec::new(),
    historical_polls: Vec::new(),
    import_run_status: "running",
  };
  let mut last_topic = 0_i32;
  let mut last_number = 0_i32;
  loop {
    let batch = diesel::sql_query(
      "SELECT p.id, p.topic_id, p.post_number, p.reply_to_post_number, p.user_id, \
       COALESCE(p.raw, '') AS raw, p.created_at AT TIME ZONE 'UTC' AS created_at, \
       p.updated_at AT TIME ZONE 'UTC' AS updated_at, p.deleted_at IS NOT NULL AS deleted, \
       t.title, t.category_id, t.closed AS topic_closed \
       FROM posts p JOIN topics t ON t.id = p.topic_id \
       WHERE t.archetype <> 'private_message' AND p.post_type = 1 \
         AND (t.id > $1 OR (t.id = $1 AND p.post_number > $2)) \
       ORDER BY t.id, p.post_number LIMIT 500",
    )
    .bind::<Integer, _>(last_topic)
    .bind::<Integer, _>(last_number)
    .load::<SourceDiscussionPost>(&mut source)
    .await
    .context("failed to read Discourse posts")?;
    if batch.is_empty() {
      break;
    }
    for source_post in batch {
      last_topic = source_post.topic_id;
      last_number = source_post.post_number;
      report.source_post_count += 1;
      let historical_polls = read_historical_polls(&mut source, source_post.id).await?;
      let imported_content = append_historical_polls(&source_post.raw, &historical_polls);
      report.historical_polls.extend(historical_polls);
      let source_id = source_post.id.to_string();
      let Some(source_user_id) = source_post.user_id else {
        report.unresolved_source_post_ids.push(source_post.id);
        continue;
      };
      let target_user_id = read_mapping(pool, run.id, "user", &source_user_id.to_string())
        .await
        .map_err(convert_lemmy_error)?;
      let target_user_id = match target_user_id {
        Some(id) => lemmy_db_schema_file::PersonId(id as i32),
        None => {
          report.unresolved_source_post_ids.push(source_post.id);
          continue;
        }
      };
      let source_category_id = match source_post.category_id {
        Some(id) => id,
        None => {
          report.unresolved_source_post_ids.push(source_post.id);
          continue;
        }
      };
      let target_community_id = read_mapping(
        pool,
        run.id,
        "category_community",
        &source_category_id.to_string(),
      )
      .await
      .map_err(convert_lemmy_error)?;
      let target_community_id = match target_community_id {
        Some(id) => CommunityId(id as i32),
        None => {
          report.unresolved_source_post_ids.push(source_post.id);
          continue;
        }
      };

      if source_post.post_number == 1 {
        if let Some(target_id) =
          read_mapping(pool, run.id, "topic", &source_post.topic_id.to_string())
            .await
            .map_err(convert_lemmy_error)?
        {
          let target_post_id = PostId(target_id as i32);
          let existing_post = Post::read(pool, target_post_id)
            .await
            .map_err(convert_lemmy_error)?;
          if existing_post.creator_id != target_user_id
            || existing_post.community_id != target_community_id
            || existing_post.origin_local_site_id != Some(site_id)
          {
            bail!(
              "Discourse topic {} maps to a post with conflicting ownership",
              source_post.topic_id
            );
          }
          record_mapping(
            pool,
            run.id,
            "source_discussion_post",
            &source_id,
            i64::from(target_id as i32),
          )
          .await
          .map_err(convert_lemmy_error)?;
          report.resumed_object_count += 1;
          continue;
        }
        let ap_id = imported_object_ap_id(
          &protocol_and_hostname,
          source_fingerprint,
          "topic",
          i64::from(source_post.topic_id),
        )?;
        let mut form = PostInsertForm::new(
          source_post
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| format!("Untitled topic {}", source_post.topic_id)),
          target_user_id,
          target_community_id,
        );
        form.body = Some(imported_content.clone());
        form.locked = source_post.topic_closed;
        form.published_at = Some(source_post.created_at);
        form.updated_at = source_post.updated_at;
        form.deleted = Some(source_post.deleted);
        form.ap_id = Some(ap_id);
        form.local = Some(true);
        form.origin_local_site_id = Some(site_id);
        create_imported_post(
          pool,
          run.id,
          &source_post.topic_id.to_string(),
          &source_id,
          site_id,
          &form,
        )
        .await
        .map_err(convert_lemmy_error)
        .with_context(|| format!("failed to import Discourse topic {}", source_post.topic_id))?;
        report.imported_topic_count += 1;
      } else {
        let target_post_id =
          match read_mapping(pool, run.id, "topic", &source_post.topic_id.to_string())
            .await
            .map_err(convert_lemmy_error)?
          {
            Some(id) => PostId(id as i32),
            None => {
              report.unresolved_source_post_ids.push(source_post.id);
              continue;
            }
          };
        if read_mapping(pool, run.id, "source_discussion_post", &source_id)
          .await
          .map_err(convert_lemmy_error)?
          .is_some()
        {
          report.resumed_object_count += 1;
          continue;
        }
        let parent = if source_post.reply_to_post_number.unwrap_or(1) <= 1 {
          None
        } else {
          let parent =
            diesel::sql_query("SELECT id FROM posts WHERE topic_id = $1 AND post_number = $2")
              .bind::<Integer, _>(source_post.topic_id)
              .bind::<Integer, _>(source_post.reply_to_post_number.unwrap_or_default())
              .get_result::<SourceParent>(&mut source)
              .await
              .optional()
              .context("failed to resolve a Discourse reply parent")?;
          let Some(parent) = parent else {
            report.unresolved_source_post_ids.push(source_post.id);
            continue;
          };
          let Some(target_parent_id) = read_mapping(
            pool,
            run.id,
            "source_discussion_post",
            &parent.id.to_string(),
          )
          .await
          .map_err(convert_lemmy_error)?
          else {
            report.unresolved_source_post_ids.push(source_post.id);
            continue;
          };
          Some(
            Comment::read(pool, CommentId(target_parent_id as i32))
              .await
              .map_err(convert_lemmy_error)?,
          )
        };
        let ap_id = imported_object_ap_id(
          &protocol_and_hostname,
          source_fingerprint,
          "reply",
          i64::from(source_post.id),
        )?;
        let mut form = CommentInsertForm::new(
          target_user_id,
          target_post_id,
          target_community_id,
          imported_content,
        );
        form.published_at = Some(source_post.created_at);
        form.updated_at = source_post.updated_at;
        form.deleted = Some(source_post.deleted);
        form.ap_id = Some(ap_id);
        form.local = Some(true);
        form.origin_local_site_id = Some(site_id);
        let parent_path = parent.as_ref().map(|comment| &comment.path);
        create_imported_comment(pool, run.id, &source_id, site_id, &form, parent_path)
          .await
          .map_err(convert_lemmy_error)
          .with_context(|| format!("failed to import Discourse reply {}", source_post.id))?;
        report.imported_reply_count += 1;
      }
      update_phase(
        pool,
        run.id,
        "discussions",
        Some(&source_id),
        "running",
        (report.imported_topic_count + report.imported_reply_count + report.resumed_object_count)
          as i64,
      )
      .await
      .map_err(convert_lemmy_error)?;
    }
  }
  // A skipped or otherwise unmappable source post should be reported and later
  // caught by validation, but must not prevent import-all from processing the
  // remaining phases that can still be imported.
  update_phase(
    pool,
    run.id,
    "discussions",
    Some(&format!("{}:{}", last_topic, last_number)),
    "completed",
    (report.imported_topic_count + report.imported_reply_count + report.resumed_object_count)
      as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  let durable_report = serde_json::json!({"discussions": report});
  store_run_report(pool, run.id, &durable_report)
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

pub async fn import_private_messages(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
) -> Result<PrivateMessageImportReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing Discourse private messages");
  }
  let protocol_and_hostname = format!(
    "{}://{}",
    SETTINGS.get_protocol_string(),
    target_site.hostname
  );
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if run.status == "completed" {
    bail!("this archive has already completed for the selected target site");
  }
  if !phase_completed(pool, run.id, "users")
    .await
    .map_err(convert_lemmy_error)?
  {
    bail!("import accounts before importing private messages");
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;

  let mut report = PrivateMessageImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_pm_post_count: 0,
    source_chat_message_count: 0,
    imported_pm_message_count: 0,
    resumed_pm_message_count: 0,
    unresolved_source_pm_post_ids: Vec::new(),
    pairwise_group_topic_ids: Vec::new(),
    pairwise_group_chat_channel_ids: Vec::new(),
    unresolved_chat_message_ids: Vec::new(),
    source_chat_messages_with_attachments: 0,
    source_chat_messages_with_structured_blocks: 0,
    warnings: Vec::new(),
    import_run_status: "running",
  };
  let mut participant_cache: HashMap<i32, Vec<i32>> = HashMap::new();
  let mut last_source_post_id = 0_i32;
  loop {
    let batch = diesel::sql_query(
      "SELECT p.id, p.topic_id, p.user_id, COALESCE(p.raw, '') AS raw, \
       p.created_at AT TIME ZONE 'UTC' AS created_at, \
       p.updated_at AT TIME ZONE 'UTC' AS updated_at, p.deleted_at IS NOT NULL AS deleted \
       FROM posts p JOIN topics t ON t.id = p.topic_id \
       WHERE t.archetype = 'private_message' AND p.post_type = 1 AND p.id > $1 \
       ORDER BY p.id LIMIT 500",
    )
    .bind::<Integer, _>(last_source_post_id)
    .load::<SourcePrivateMessagePost>(&mut source)
    .await
    .context("failed to read Discourse private-message posts")?;
    if batch.is_empty() {
      break;
    }
    for source_post in batch {
      last_source_post_id = source_post.id;
      report.source_pm_post_count += 1;
      let participants = if let Some(participants) = participant_cache.get(&source_post.topic_id) {
        participants.clone()
      } else {
        let participants = diesel::sql_query(
          "SELECT user_id FROM topic_allowed_users WHERE topic_id = $1 \
           UNION SELECT user_id FROM posts WHERE topic_id = $1 ORDER BY user_id",
        )
        .bind::<Integer, _>(source_post.topic_id)
        .load::<SourceParticipant>(&mut source)
        .await
        .context("failed to read Discourse private-message participants")?
        .into_iter()
        .map(|participant| participant.user_id)
        .collect::<Vec<_>>();
        participant_cache.insert(source_post.topic_id, participants.clone());
        participants
      };
      let recipients = participants
        .iter()
        .copied()
        .filter(|user_id| *user_id != source_post.user_id)
        .collect::<Vec<_>>();
      if recipients.is_empty() {
        report.unresolved_source_pm_post_ids.push(source_post.id);
        continue;
      }
      if participants.len() > 2
        && !report
          .pairwise_group_topic_ids
          .contains(&source_post.topic_id)
      {
        report.pairwise_group_topic_ids.push(source_post.topic_id);
      }
      let sender = read_mapping(pool, run.id, "user", &source_post.user_id.to_string())
        .await
        .map_err(convert_lemmy_error)?;
      let Some(sender_id) = sender else {
        report.unresolved_source_pm_post_ids.push(source_post.id);
        continue;
      };
      let sender_id = lemmy_db_schema_file::PersonId(sender_id as i32);

      for source_recipient_id in recipients {
        let recipient = read_mapping(pool, run.id, "user", &source_recipient_id.to_string())
          .await
          .map_err(convert_lemmy_error)?;
        let Some(recipient_id) = recipient else {
          report.unresolved_source_pm_post_ids.push(source_post.id);
          continue;
        };
        let target_recipient_id = lemmy_db_schema_file::PersonId(recipient_id as i32);
        let mapping_key = format!(
          "{}:{}:{}",
          source_post.topic_id, source_post.id, source_recipient_id
        );
        if let Some(target_id) = read_mapping(pool, run.id, "private_message", &mapping_key)
          .await
          .map_err(convert_lemmy_error)?
        {
          ensure_imported_private_message_notification(
            pool,
            PrivateMessageId(target_id as i32),
            true,
          )
          .await
          .map_err(convert_lemmy_error)?;
          report.resumed_pm_message_count += 1;
          continue;
        }
        let ap_id = imported_object_ap_id(
          &protocol_and_hostname,
          source_fingerprint,
          &format!("private-message/{source_recipient_id}"),
          i64::from(source_post.id),
        )?;
        let mut form =
          PrivateMessageInsertForm::new(sender_id, target_recipient_id, source_post.raw.clone());
        form.deleted = Some(source_post.deleted);
        form.published_at = Some(source_post.created_at);
        form.updated_at = source_post.updated_at;
        form.ap_id = Some(ap_id);
        form.local = Some(true);
        form.origin_local_site_id = Some(site_id);
        create_imported_private_message(
          pool,
          run.id,
          &mapping_key,
          &source_post.id.to_string(),
          site_id,
          true,
          &form,
        )
        .await
        .map_err(convert_lemmy_error)
        .with_context(|| {
          format!(
            "failed to import Discourse private message {}",
            source_post.id
          )
        })?;
        report.imported_pm_message_count += 1;
      }
      update_phase(
        pool,
        run.id,
        "private_messages",
        Some(&source_post.id.to_string()),
        "running",
        (report.imported_pm_message_count + report.resumed_pm_message_count) as i64,
      )
      .await
      .map_err(convert_lemmy_error)?;
    }
  }
  let chat_tables = diesel::sql_query(
    "SELECT to_regclass('public.chat_messages') IS NOT NULL AS has_messages, \
     to_regclass('public.chat_channels') IS NOT NULL AS has_channels, \
     to_regclass('public.direct_message_channels') IS NOT NULL AS has_dm_channels, \
     to_regclass('public.direct_message_users') IS NOT NULL AS has_dm_users",
  )
  .get_result::<SourceChatTables>(&mut source)
  .await
  .context("failed to inspect optional Discourse Chat tables")?;
  let all_chat_tables_present = chat_tables.has_messages
    && chat_tables.has_channels
    && chat_tables.has_dm_channels
    && chat_tables.has_dm_users;
  let any_chat_table_present = chat_tables.has_messages
    || chat_tables.has_channels
    || chat_tables.has_dm_channels
    || chat_tables.has_dm_users;
  let incomplete_chat_tables = any_chat_table_present && !all_chat_tables_present;
  if all_chat_tables_present {
    let mut last_chat_message_id = 0_i64;
    let mut chat_participant_cache = HashMap::<i64, Vec<i32>>::new();
    loop {
      let batch = diesel::sql_query(
        "SELECT m.id, dmc.id AS direct_message_channel_id, \
         dmc.\"group\" AS is_group, m.user_id, COALESCE(m.message, m.cooked, '') AS raw, \
         m.created_at AT TIME ZONE 'UTC' AS created_at, \
         m.updated_at AT TIME ZONE 'UTC' AS updated_at, \
         m.deleted_at IS NOT NULL AS deleted, \
         EXISTS (SELECT 1 FROM upload_references ur WHERE ur.target_id = m.id \
           AND ur.target_type = 'ChatMessage') AS has_attachments, \
         m.blocks IS NOT NULL AND m.blocks <> 'null'::jsonb AS has_structured_blocks \
         FROM chat_messages m \
         JOIN chat_channels cc ON cc.id = m.chat_channel_id AND cc.chatable_type = 'DirectMessage' \
         JOIN direct_message_channels dmc ON dmc.id = cc.chatable_id \
         WHERE m.id > $1 ORDER BY m.id LIMIT 500",
      )
      .bind::<BigInt, _>(last_chat_message_id)
      .load::<SourceChatDirectMessage>(&mut source)
      .await
      .context("failed to read Discourse direct-message chat messages")?;
      if batch.is_empty() {
        break;
      }
      for chat_message in batch {
        last_chat_message_id = chat_message.id;
        report.source_chat_message_count += 1;
        report.source_chat_messages_with_attachments += usize::from(chat_message.has_attachments);
        report.source_chat_messages_with_structured_blocks +=
          usize::from(chat_message.has_structured_blocks);
        let Some(source_sender_id) = chat_message.user_id else {
          report.unresolved_chat_message_ids.push(chat_message.id);
          continue;
        };
        let participants = if let Some(participants) =
          chat_participant_cache.get(&chat_message.direct_message_channel_id)
        {
          participants.clone()
        } else {
          let participants = diesel::sql_query(
            "SELECT user_id FROM direct_message_users \
             WHERE direct_message_channel_id = $1 ORDER BY user_id",
          )
          .bind::<BigInt, _>(chat_message.direct_message_channel_id)
          .load::<SourceParticipant>(&mut source)
          .await
          .context("failed to read Discourse direct-message channel participants")?
          .into_iter()
          .map(|participant| participant.user_id)
          .collect::<Vec<_>>();
          chat_participant_cache
            .insert(chat_message.direct_message_channel_id, participants.clone());
          participants
        };
        let recipients = participants
          .iter()
          .copied()
          .filter(|user_id| *user_id != source_sender_id)
          .collect::<Vec<_>>();
        if (chat_message.is_group || recipients.len() > 1)
          && !report
            .pairwise_group_chat_channel_ids
            .contains(&chat_message.direct_message_channel_id)
        {
          report
            .pairwise_group_chat_channel_ids
            .push(chat_message.direct_message_channel_id);
        }
        if recipients.is_empty() {
          report.unresolved_chat_message_ids.push(chat_message.id);
          continue;
        }
        let Some(sender_id) = read_mapping(pool, run.id, "user", &source_sender_id.to_string())
          .await
          .map_err(convert_lemmy_error)?
        else {
          report.unresolved_chat_message_ids.push(chat_message.id);
          continue;
        };
        for source_recipient_id in recipients {
          let Some(recipient_id) =
            read_mapping(pool, run.id, "user", &source_recipient_id.to_string())
              .await
              .map_err(convert_lemmy_error)?
          else {
            report.unresolved_chat_message_ids.push(chat_message.id);
            continue;
          };
          // A leading zero distinguishes Chat messages from legacy PM post IDs;
          // both still fit the validation ledger's sender lookup convention.
          let source_message_key = format!("0:{}", chat_message.id);
          let mapping_key = format!(
            "{}:{}:{}",
            source_message_key, chat_message.direct_message_channel_id, source_recipient_id
          );
          if let Some(target_id) = read_mapping(pool, run.id, "private_message", &mapping_key)
            .await
            .map_err(convert_lemmy_error)?
          {
            ensure_imported_private_message_notification(
              pool,
              PrivateMessageId(target_id as i32),
              true,
            )
            .await
            .map_err(convert_lemmy_error)?;
            report.resumed_pm_message_count += 1;
            continue;
          }
          let ap_id = imported_object_ap_id(
            &protocol_and_hostname,
            source_fingerprint,
            &format!(
              "chat-private-message/{}/{source_recipient_id}",
              chat_message.direct_message_channel_id
            ),
            chat_message.id,
          )?;
          let mut form = PrivateMessageInsertForm::new(
            lemmy_db_schema_file::PersonId(sender_id as i32),
            lemmy_db_schema_file::PersonId(recipient_id as i32),
            chat_message.raw.clone(),
          );
          form.deleted = Some(chat_message.deleted);
          form.published_at = Some(chat_message.created_at);
          form.updated_at = chat_message.updated_at;
          form.ap_id = Some(ap_id);
          form.local = Some(true);
          form.origin_local_site_id = Some(site_id);
          create_imported_private_message(
            pool,
            run.id,
            &mapping_key,
            &source_message_key,
            site_id,
            true,
            &form,
          )
          .await
          .map_err(convert_lemmy_error)
          .with_context(|| {
            format!(
              "failed to import Discourse Chat message {}",
              chat_message.id
            )
          })?;
          report.imported_pm_message_count += 1;
        }
        update_phase(
          pool,
          run.id,
          "private_messages",
          Some(&last_chat_message_id.to_string()),
          "running",
          (report.imported_pm_message_count + report.resumed_pm_message_count) as i64,
        )
        .await
        .map_err(convert_lemmy_error)?;
      }
    }
  } else if incomplete_chat_tables {
    report.warnings.push(
      "Discourse Chat tables are incomplete; direct-message chat history was not imported and the phase cannot complete"
        .to_string(),
    );
  } else {
    report.warnings.push(
      "Discourse Chat tables are absent; direct-message chat history was not imported".to_string(),
    );
  }
  if report.source_chat_messages_with_attachments > 0 {
    report.warnings.push(format!(
      "{} source direct-message Chat messages contain upload references that were not rewritten",
      report.source_chat_messages_with_attachments
    ));
  }
  if report.source_chat_messages_with_structured_blocks > 0 {
    report.warnings.push(format!(
      "{} source direct-message Chat messages contain structured blocks; only the text field is preserved when the message is imported",
      report.source_chat_messages_with_structured_blocks
    ));
  }
  if report.source_chat_message_count > 0 {
    report.warnings.push(
      "Chat direct-message recipients come from the current channel membership snapshot; historical members who have left cannot be reconstructed"
        .to_string(),
    );
  }
  report.pairwise_group_topic_ids.sort_unstable();
  report.pairwise_group_topic_ids.dedup();
  report.pairwise_group_chat_channel_ids.sort_unstable();
  report.pairwise_group_chat_channel_ids.dedup();
  report.unresolved_chat_message_ids.sort_unstable();
  report.unresolved_chat_message_ids.dedup();
  let unresolved = !report.unresolved_source_pm_post_ids.is_empty()
    || !report.unresolved_chat_message_ids.is_empty()
    || incomplete_chat_tables;
  update_phase(
    pool,
    run.id,
    "private_messages",
    Some(&last_source_post_id.to_string()),
    if unresolved { "failed" } else { "completed" },
    (report.imported_pm_message_count + report.resumed_pm_message_count) as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  let durable_report = serde_json::json!({"private_messages": report});
  store_run_report(pool, run.id, &durable_report)
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, if unresolved { "failed" } else { "running" })
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

pub async fn export_legacy_urls(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
) -> Result<(Vec<(String, String)>, LegacyUrlExportReport)> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let _target_site = load_target_site(pool, site_id).await?;
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if !phase_completed(pool, run.id, "discussions")
    .await
    .map_err(convert_lemmy_error)?
  {
    bail!("complete the public discussion phase before exporting legacy URLs");
  }

  let target_conn = &mut get_conn(pool).await?;
  let topic_mappings = diesel::sql_query(
    "SELECT source_id, target_id FROM discourse_import_mapping \
     WHERE run_id = $1 AND entity_type = 'topic'",
  )
  .bind::<BigInt, _>(run.id)
  .load::<TargetSourceMapping>(target_conn)
  .await
  .context("failed to read imported topic mappings")?;
  let topic_targets = topic_mappings
    .into_iter()
    .filter_map(|mapping| {
      mapping
        .source_id
        .parse::<i32>()
        .ok()
        .map(|source_id| (source_id, mapping.target_id))
    })
    .collect::<HashMap<_, _>>();
  let reply_mappings = diesel::sql_query(
    "SELECT source_id, target_id FROM discourse_import_mapping \
     WHERE run_id = $1 AND entity_type = 'source_discussion_post'",
  )
  .bind::<BigInt, _>(run.id)
  .load::<TargetSourceMapping>(target_conn)
  .await
  .context("failed to read imported reply mappings")?;
  let reply_targets = reply_mappings
    .into_iter()
    .filter_map(|mapping| {
      mapping
        .source_id
        .parse::<i32>()
        .ok()
        .map(|source_id| (source_id, mapping.target_id))
    })
    .collect::<HashMap<_, _>>();

  let source_posts = diesel::sql_query(
    "SELECT t.id AS topic_id, p.id AS source_post_id, p.post_number \
     FROM topics t JOIN posts p ON p.topic_id = t.id \
     WHERE t.archetype <> 'private_message' AND p.post_type = 1 \
     ORDER BY t.id, p.post_number",
  )
  .load::<SourceLegacyPost>(&mut source)
  .await
  .context("failed to read Discourse URLs for imported discussions")?;
  let mut entries = std::collections::BTreeMap::new();
  let mut report = LegacyUrlExportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    generated_entry_count: 0,
    unmapped_source_post_ids: Vec::new(),
    import_run_status: "running",
  };
  for source_post in source_posts {
    let Some(target_post_id) = topic_targets.get(&source_post.topic_id).copied() else {
      report
        .unmapped_source_post_ids
        .push(source_post.source_post_id);
      continue;
    };
    let target_path = if source_post.post_number <= 1 {
      format!("/post/{target_post_id}")
    } else if let Some(target_comment_id) = reply_targets.get(&source_post.source_post_id) {
      format!("/post/{target_post_id}/{target_comment_id}")
    } else {
      report
        .unmapped_source_post_ids
        .push(source_post.source_post_id);
      continue;
    };
    entries.insert(
      format!(
        "{}:{}",
        source_post.topic_id,
        source_post.post_number.max(1)
      ),
      target_path.clone(),
    );
    entries.insert(format!("{}:last", source_post.topic_id), target_path);
  }
  report.unmapped_source_post_ids.sort_unstable();
  report.unmapped_source_post_ids.dedup();
  report.generated_entry_count = entries.len();
  let entries = entries.into_iter().collect();
  Ok((entries, report))
}

pub async fn import_interactions(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
) -> Result<InteractionImportReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing interactions");
  }
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if !phase_completed(pool, run.id, "users")
    .await
    .map_err(convert_lemmy_error)?
    || !phase_completed(pool, run.id, "discussions")
      .await
      .map_err(convert_lemmy_error)?
  {
    bail!("import users and public discussions before importing interactions");
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;
  let _ = rustls::crypto::ring::default_provider().install_default();
  let mut report = InteractionImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_active_like_count: 0,
    imported_vote_count: 0,
    resumed_vote_count: 0,
    source_bookmark_count: 0,
    imported_bookmark_count: 0,
    resumed_bookmark_count: 0,
    unsupported_bookmark_count: 0,
    unsupported_action_count: 0,
    unresolved_interaction_ids: Vec::new(),
    import_run_status: "running",
  };
  let mut action_cursor = 0_i32;
  loop {
    let actions = diesel::sql_query(
      "SELECT pa.id, p.id AS source_post_id, p.topic_id, p.post_number, pa.user_id, \
       pa.created_at AT TIME ZONE 'UTC' AS created_at \
       FROM post_actions pa JOIN posts p ON p.id = pa.post_id \
       JOIN topics t ON t.id = p.topic_id \
       WHERE pa.id > $1 AND pa.post_action_type_id = 2 AND pa.deleted_at IS NULL \
         AND t.archetype <> 'private_message' AND p.post_type = 1 \
       ORDER BY pa.id LIMIT 500",
    )
    .bind::<Integer, _>(action_cursor)
    .load::<SourceLike>(&mut source)
    .await
    .context("failed to read Discourse like actions")?;
    if actions.is_empty() {
      break;
    }
    for action in actions {
      action_cursor = action.id;
      report.source_active_like_count += 1;
      let person = read_mapping(pool, run.id, "user", &action.user_id.to_string())
        .await
        .map_err(convert_lemmy_error)?;
      let object_id = if action.post_number == 1 {
        read_mapping(pool, run.id, "topic", &action.topic_id.to_string())
          .await
          .map_err(convert_lemmy_error)?
      } else {
        read_mapping(
          pool,
          run.id,
          "source_discussion_post",
          &action.source_post_id.to_string(),
        )
        .await
        .map_err(convert_lemmy_error)?
      };
      let (Some(person_id), Some(object_id)) = (person, object_id) else {
        report.unresolved_interaction_ids.push(i64::from(action.id));
        continue;
      };
      let inserted = create_imported_vote(
        pool,
        run.id,
        &action.id.to_string(),
        lemmy_db_schema_file::PersonId(person_id as i32),
        object_id as i32,
        action.post_number == 1,
        action.created_at,
      )
      .await
      .map_err(convert_lemmy_error)
      .with_context(|| format!("failed to import Discourse like {}", action.id))?;
      if inserted {
        report.imported_vote_count += 1;
      } else {
        report.resumed_vote_count += 1;
      }
    }
  }

  let unsupported_action_count = diesel::sql_query(
    "SELECT \
       (SELECT COUNT(*) FROM post_actions WHERE deleted_at IS NULL AND post_action_type_id <> 2) + \
       (SELECT COUNT(*) FROM post_actions pa JOIN posts p ON p.id = pa.post_id \
        JOIN topics t ON t.id = p.topic_id WHERE pa.deleted_at IS NULL \
        AND pa.post_action_type_id = 2 \
        AND (t.archetype = 'private_message' OR p.post_type <> 1)) AS count",
  )
  .get_result::<CountResult>(&mut source)
  .await
  .context("failed to count unsupported Discourse post actions")?;
  report.unsupported_action_count = unsupported_action_count.count as usize;

  let mut bookmark_cursor = 0_i64;
  loop {
    let bookmarks = diesel::sql_query(
      "SELECT id, user_id, bookmarkable_id, bookmarkable_type, \
       created_at AT TIME ZONE 'UTC' AS created_at \
       FROM bookmarks WHERE id > $1 ORDER BY id LIMIT 500",
    )
    .bind::<BigInt, _>(bookmark_cursor)
    .load::<SourceBookmark>(&mut source)
    .await
    .context("failed to read Discourse bookmarks")?;
    if bookmarks.is_empty() {
      break;
    }
    for bookmark in bookmarks {
      bookmark_cursor = bookmark.id;
      report.source_bookmark_count += 1;
      let target_person = read_mapping(pool, run.id, "user", &bookmark.user_id.to_string())
        .await
        .map_err(convert_lemmy_error)?;
      let Some(target_person) = target_person else {
        report.unresolved_interaction_ids.push(bookmark.id);
        continue;
      };
      let (target_object, is_post) = match (
        bookmark.bookmarkable_type.as_deref(),
        bookmark.bookmarkable_id,
      ) {
        (Some("Topic"), Some(source_topic_id)) => {
          let topic = diesel::sql_query("SELECT archetype FROM topics WHERE id = $1")
            .bind::<BigInt, _>(source_topic_id)
            .get_result::<SourceTopicArchetype>(&mut source)
            .await
            .optional()
            .context("failed to resolve bookmarked Discourse topic")?;
          if !topic.is_some_and(|topic| topic.archetype != "private_message") {
            report.unsupported_bookmark_count += 1;
            continue;
          }
          (
            read_mapping(pool, run.id, "topic", &source_topic_id.to_string())
              .await
              .map_err(convert_lemmy_error)?,
            true,
          )
        }
        (Some("Post"), Some(source_post_id)) => {
          let source_post = diesel::sql_query(
            "SELECT p.topic_id, p.post_number FROM posts p JOIN topics t ON t.id = p.topic_id \
             WHERE p.id = $1 AND t.archetype <> 'private_message' AND p.post_type = 1",
          )
          .bind::<BigInt, _>(source_post_id)
          .get_result::<SourcePostLocation>(&mut source)
          .await
          .optional()
          .context("failed to resolve bookmarked Discourse post")?;
          let Some(source_post) = source_post else {
            report.unsupported_bookmark_count += 1;
            continue;
          };
          let entity_type = if source_post.post_number == 1 {
            "topic"
          } else {
            "source_discussion_post"
          };
          let source_key = if source_post.post_number == 1 {
            source_post.topic_id.to_string()
          } else {
            source_post_id.to_string()
          };
          (
            read_mapping(pool, run.id, entity_type, &source_key)
              .await
              .map_err(convert_lemmy_error)?,
            source_post.post_number == 1,
          )
        }
        _ => {
          report.unsupported_bookmark_count += 1;
          continue;
        }
      };
      let Some(target_object) = target_object else {
        report.unresolved_interaction_ids.push(bookmark.id);
        continue;
      };
      let inserted = create_imported_bookmark(
        pool,
        run.id,
        &bookmark.id.to_string(),
        lemmy_db_schema_file::PersonId(target_person as i32),
        target_object as i32,
        is_post,
        bookmark.created_at,
      )
      .await
      .map_err(convert_lemmy_error)
      .with_context(|| format!("failed to import Discourse bookmark {}", bookmark.id))?;
      if inserted {
        report.imported_bookmark_count += 1;
      } else {
        report.resumed_bookmark_count += 1;
      }
    }
  }
  report.unresolved_interaction_ids.sort_unstable();
  report.unresolved_interaction_ids.dedup();
  let unresolved = !report.unresolved_interaction_ids.is_empty();
  update_phase(
    pool,
    run.id,
    "interactions",
    Some(&format!(
      "likes:{action_cursor};bookmarks:{bookmark_cursor}"
    )),
    if unresolved { "failed" } else { "completed" },
    (report.imported_vote_count
      + report.resumed_vote_count
      + report.imported_bookmark_count
      + report.resumed_bookmark_count) as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  let durable_report = serde_json::json!({"interactions": report});
  store_run_report(pool, run.id, &durable_report)
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, if unresolved { "failed" } else { "running" })
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

pub async fn import_tags(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
) -> Result<TagImportReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing tags");
  }
  let protocol_and_hostname = format!(
    "{}://{}",
    SETTINGS.get_protocol_string(),
    target_site.hostname
  );
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if !phase_completed(pool, run.id, "discussions")
    .await
    .map_err(convert_lemmy_error)?
  {
    bail!("import public discussions before importing tags");
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;

  let source_rows = diesel::sql_query(
    "SELECT tt.topic_id, t.category_id, tg.id AS tag_id, tg.name \
     FROM topic_tags tt JOIN topics t ON t.id = tt.topic_id \
     JOIN tags tg ON tg.id = tt.tag_id \
     WHERE t.archetype <> 'private_message' AND t.deleted_at IS NULL \
       AND EXISTS (SELECT 1 FROM posts p WHERE p.topic_id = t.id \
         AND p.post_number = 1 AND p.post_type = 1) \
     ORDER BY tt.topic_id, tt.tag_id",
  )
  .load::<SourceTopicTag>(&mut source)
  .await
  .context("failed to read Discourse topic tags")?;
  let mut rows_by_topic = BTreeMap::<i32, Vec<SourceTopicTag>>::new();
  for row in source_rows {
    rows_by_topic.entry(row.topic_id).or_default().push(row);
  }
  let mut report = TagImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_topic_tag_count: 0,
    imported_tag_count: 0,
    resumed_tag_count: 0,
    imported_post_tag_count: 0,
    unresolved_topic_ids: Vec::new(),
    import_run_status: "running",
  };

  for (topic_id, source_tags) in rows_by_topic {
    report.source_topic_tag_count += source_tags.len();
    let Some(post_id) = read_mapping(pool, run.id, "topic", &topic_id.to_string())
      .await
      .map_err(convert_lemmy_error)?
    else {
      report.unresolved_topic_ids.push(topic_id);
      continue;
    };
    let post = Post::read(pool, PostId(post_id as i32))
      .await
      .map_err(convert_lemmy_error)?;
    let mut target_tag_ids = Vec::new();
    let mut reserved_names = CommunityTag::read_for_community(pool, post.community_id)
      .await
      .map_err(convert_lemmy_error)?
      .into_iter()
      .map(|tag| tag.name)
      .collect::<HashSet<_>>();

    for source_tag in source_tags {
      let Some(source_category_id) = source_tag.category_id else {
        report.unresolved_topic_ids.push(topic_id);
        continue;
      };
      let mapping_key = format!("{source_category_id}:{}", source_tag.tag_id);
      let tag_id = if let Some(mapped_id) = read_mapping(pool, run.id, "category_tag", &mapping_key)
        .await
        .map_err(convert_lemmy_error)?
      {
        report.resumed_tag_count += 1;
        mapped_id
      } else {
        let community = Community::read(pool, post.community_id)
          .await
          .map_err(convert_lemmy_error)?;
        let tag_name = unique_tag_name(&source_tag.name, source_tag.tag_id, &reserved_names);
        let display_name = source_tag.name.chars().take(255).collect::<String>();
        let actor_id = Url::parse(&format!(
          "{protocol_and_hostname}/c/{}/tag/{tag_name}",
          community.name
        ))?;
        let tag = create_imported_community_tag(
          pool,
          run.id,
          &mapping_key,
          &CommunityTagInsertForm {
            ap_id: actor_id.into(),
            name: tag_name.clone(),
            display_name: Some(display_name),
            summary: None,
            community_id: post.community_id,
            deleted: Some(false),
            color: Some(TagColor::Color01),
          },
        )
        .await
        .map_err(convert_lemmy_error)
        .with_context(|| format!("failed to import Discourse tag {}", source_tag.tag_id))?;
        reserved_names.insert(tag_name);
        report.imported_tag_count += 1;
        i64::from(tag.id.0)
      };
      record_mapping(
        pool,
        run.id,
        "topic_tag",
        &format!("{topic_id}:{}", source_tag.tag_id),
        tag_id,
      )
      .await
      .map_err(convert_lemmy_error)?;
      target_tag_ids.push(lemmy_db_schema_file::newtypes::CommunityTagId(
        tag_id as i32,
      ));
    }
    target_tag_ids.sort_unstable_by_key(|tag_id| tag_id.0);
    target_tag_ids.dedup_by_key(|tag_id| tag_id.0);
    if !target_tag_ids.is_empty() {
      PostCommunityTag::update(pool, &post, &target_tag_ids)
        .await
        .map_err(convert_lemmy_error)
        .with_context(|| format!("failed to assign imported tags to topic {topic_id}"))?;
      report.imported_post_tag_count += target_tag_ids.len();
    }
    update_phase(
      pool,
      run.id,
      "tags",
      Some(&topic_id.to_string()),
      "running",
      (report.imported_tag_count + report.resumed_tag_count) as i64,
    )
    .await
    .map_err(convert_lemmy_error)?;
  }

  report.unresolved_topic_ids.sort_unstable();
  report.unresolved_topic_ids.dedup();
  let unresolved = !report.unresolved_topic_ids.is_empty();
  update_phase(
    pool,
    run.id,
    "tags",
    None,
    if unresolved { "failed" } else { "completed" },
    (report.imported_tag_count + report.resumed_tag_count) as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  store_run_report(pool, run.id, &serde_json::json!({"tags": report}))
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, if unresolved { "failed" } else { "running" })
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

pub async fn import_media(
  archive: &std::path::Path,
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
  archived_upload_file_count: usize,
) -> Result<MediaImportReport> {
  const MAX_IMAGE_BYTES: u64 = 268_435_456;
  const MAX_ATTACHMENT_BYTES: u64 = 1_073_741_824;
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must be disabled before importing media");
  }
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  if !phase_completed(pool, run.id, "discussions")
    .await
    .map_err(convert_lemmy_error)?
  {
    bail!("import public discussions before importing media");
  }
  set_run_status(pool, run.id, "running")
    .await
    .map_err(convert_lemmy_error)?;

  let _ = rustls::crypto::ring::default_provider().install_default();

  let members = if archive
    .file_name()
    .and_then(|name| name.to_str())
    .is_some_and(|name| name.ends_with(".tar.gz"))
  {
    super::list_tar_members(archive)?
  } else {
    Vec::new()
  };
  let uploads = diesel::sql_query(
    "SELECT id, user_id, original_filename, filesize, url, sha1, extension, secure, \
     access_control_post_id FROM uploads ORDER BY id",
  )
  .load::<SourceUpload>(&mut source)
  .await
  .context("failed to read Discourse upload rows")?;
  let pictrs = SETTINGS.pictrs().map_err(convert_lemmy_error)?;
  let image_endpoint = format!("{}/image", pictrs.url.as_str().trim_end_matches('/'));
  let client = reqwest::Client::builder()
    .build()
    .context("failed to create the pict-rs upload client")?;
  let s3_storage = SETTINGS
    .s3()
    .map(|config| (build_s3_client(&config), config.bucket));
  let mut report = MediaImportReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_upload_count: uploads.len(),
    archived_upload_file_count,
    imported_image_count: 0,
    resumed_image_count: 0,
    imported_attachment_count: 0,
    resumed_attachment_count: 0,
    retained_secure_upload_ids: Vec::new(),
    retained_non_image_upload_ids: Vec::new(),
    retained_upload_manifest: Vec::new(),
    missing_archive_upload_ids: Vec::new(),
    invalid_upload_ids: Vec::new(),
    unresolved_content_source_post_ids: Vec::new(),
    unresolved_private_message_source_post_ids: Vec::new(),
    rewritten_avatar_count: 0,
    unresolved_avatar_source_user_ids: Vec::new(),
    rewritten_category_icon_count: 0,
    rewritten_category_banner_count: 0,
    unresolved_category_image_ids: Vec::new(),
    rewritten_source_post_count: 0,
    rewritten_media_reference_count: 0,
    import_run_status: "running",
  };
  let mut media_items = Vec::new();
  let mut all_source_tokens = Vec::new();
  for upload in &uploads {
    let source_sha1 = upload_source_sha1(upload);
    let mut source_tokens = upload_source_reference_tokens(upload);
    let optimized_images =
      diesel::sql_query("SELECT url FROM optimized_images WHERE upload_id = $1 ORDER BY id")
        .bind::<Integer, _>(upload.id)
        .load::<SourceUploadVariant>(&mut source)
        .await
        .context("failed to read Discourse optimized upload image paths")?;
    for variant in optimized_images {
      source_tokens.push(variant.url.clone());
      if let Some(path) = upload_url_path(&variant.url) {
        source_tokens.push(path);
      }
    }
    source_tokens.sort_by(|left, right| right.len().cmp(&left.len()).then(left.cmp(right)));
    source_tokens.dedup();
    all_source_tokens.extend(source_tokens.iter().cloned());
    let archive_member = find_archive_upload_member(
      &upload.url,
      upload.id,
      source_sha1.as_deref(),
      upload.extension.as_deref(),
      &upload.original_filename,
      &members,
    );
    if upload.secure || upload.access_control_post_id.is_some() {
      report.retained_secure_upload_ids.push(upload.id);
      report.retained_upload_manifest.push(RetainedUpload {
        source_upload_id: upload.id,
        original_filename: upload.original_filename.clone(),
        source_url: upload.url.clone(),
        archive_member,
        filesize_bytes: upload.filesize,
        reason: "source access control cannot be represented safely",
      });
      continue;
    }
    let extension = upload
      .extension
      .as_deref()
      .or_else(|| {
        Path::new(&upload.original_filename)
          .extension()
          .and_then(|e| e.to_str())
      })
      .unwrap_or_default()
      .trim_start_matches('.')
      .to_ascii_lowercase();
    if !matches!(
      extension.as_str(),
      "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp"
    ) {
      let owner_id = read_mapping(pool, run.id, "user", &upload.user_id.to_string())
        .await
        .map_err(convert_lemmy_error)?;
      let mut imported = false;
      if !target_site.file_upload_disabled
        && upload.filesize > 0
        && upload.filesize <= target_site.file_max_upload_size_bytes
        && owner_id.is_some()
        && let (Some(member), Some((storage, bucket))) = (archive_member.as_deref(), &s3_storage)
        && let Ok(bytes) = super::read_archive_member(archive, member, MAX_ATTACHMENT_BYTES)
        && bytes.len() as i64 == upload.filesize
      {
        let sha1 = hex::encode(<Sha1 as Sha1Digest>::digest(&bytes));
        if source_sha1
          .as_deref()
          .is_none_or(|expected| expected.eq_ignore_ascii_case(&sha1))
          && let Some(content_type) = infer::get(&bytes).map(|kind| kind.mime_type().to_string())
          && !content_type.starts_with("image/")
          && !content_type.starts_with("video/")
          && (target_site.file_allowed_mime_types.is_empty()
            || target_site
              .file_allowed_mime_types
              .iter()
              .any(|allowed| allowed == &content_type))
        {
          let owner_id = owner_id.context("mapped file owner is missing")?;
          let object = ensure_generic_file_object(
            pool,
            storage,
            bucket,
            bytes,
            upload.filesize,
            &content_type,
          )
          .await?;
          if let Some(reference_id) =
            read_mapping(pool, run.id, "file_upload", &upload.id.to_string())
              .await
              .map_err(convert_lemmy_error)?
          {
            let reference = diesel::sql_query(
              "SELECT fr.id, fr.public_id::text, fo.content_hash FROM file_reference fr \
               JOIN file_object fo ON fo.id = fr.file_id \
               WHERE fr.id = $1 AND fr.local_site_id = $2",
            )
            .bind::<Integer, _>(reference_id as i32)
            .bind::<Integer, _>(site_id.0)
            .get_result::<TargetFileReference>(&mut get_conn(pool).await?)
            .await
            .context("mapped generic file reference is missing")?;
            if reference.content_hash != object.content_hash {
              bail!(
                "mapped generic file reference no longer matches source upload {}",
                upload.id
              );
            }
            let filename = safe_import_filename(&upload.original_filename);
            let target_url = format!(
              "{}://{}/api/v4/file/{}/{}",
              SETTINGS.get_protocol_string(),
              target_site.hostname,
              reference.public_id,
              urlencoding::encode(&filename)
            );
            media_items.push(ImportedMediaItem {
              media_id: None,
              target_url,
              replacements: source_tokens,
            });
            report.resumed_attachment_count += 1;
            continue;
          }
          let filename = safe_import_filename(&upload.original_filename);
          let target_owner_id = lemmy_db_schema_file::PersonId(owner_id as i32);
          let existing_reference = diesel::sql_query(
            "SELECT fr.id, fr.public_id::text, fo.content_hash FROM file_reference fr \
             JOIN file_object fo ON fo.id = fr.file_id WHERE fr.local_site_id = $1 \
             AND fr.file_id = $2 AND fr.owner_person_id = $3 \
             AND fr.original_filename = $4 LIMIT 1",
          )
          .bind::<Integer, _>(site_id.0)
          .bind::<Integer, _>(object.id.0)
          .bind::<Integer, _>(target_owner_id.0)
          .bind::<Text, _>(&filename)
          .get_result::<TargetFileReference>(&mut get_conn(pool).await?)
          .await
          .optional()
          .context("failed to look up an existing imported file reference")?;
          let reference = if let Some(reference) = existing_reference {
            reference
          } else {
            let created = FileReference::create(
              pool,
              &FileReferenceInsertForm {
                public_id: Uuid::new_v4(),
                local_site_id: site_id,
                file_id: object.id,
                owner_person_id: target_owner_id,
                original_filename: filename.clone(),
              },
            )
            .await
            .map_err(convert_lemmy_error)?;
            TargetFileReference {
              id: created.id,
              public_id: created.public_id.to_string(),
              content_hash: object.content_hash.clone(),
            }
          };
          record_mapping(
            pool,
            run.id,
            "file_upload",
            &upload.id.to_string(),
            i64::from(reference.id),
          )
          .await
          .map_err(convert_lemmy_error)?;
          let target_url = format!(
            "{}://{}/api/v4/file/{}/{}",
            SETTINGS.get_protocol_string(),
            target_site.hostname,
            reference.public_id,
            urlencoding::encode(&filename)
          );
          media_items.push(ImportedMediaItem {
            media_id: None,
            target_url,
            replacements: source_tokens,
          });
          report.imported_attachment_count += 1;
          imported = true;
        }
      }
      if !imported {
        report.retained_non_image_upload_ids.push(upload.id);
        report.retained_upload_manifest.push(RetainedUpload {
          source_upload_id: upload.id,
          original_filename: upload.original_filename.clone(),
          source_url: upload.url.clone(),
          archive_member,
          filesize_bytes: upload.filesize,
          reason: if target_site.file_upload_disabled || s3_storage.is_none() {
            "generic file storage is disabled or unavailable"
          } else {
            "file is not in the archive, exceeds policy, has an unsupported type, or has no mapped owner"
          },
        });
      }
      continue;
    }
    let Some(member) = archive_member else {
      report.missing_archive_upload_ids.push(upload.id);
      continue;
    };
    if upload.filesize < 0 || upload.filesize as u64 > MAX_IMAGE_BYTES {
      report.invalid_upload_ids.push(upload.id);
      continue;
    }
    let bytes = match super::read_archive_member(archive, &member, MAX_IMAGE_BYTES) {
      Ok(bytes) if bytes.len() as i64 == upload.filesize => bytes,
      _ => {
        report.invalid_upload_ids.push(upload.id);
        continue;
      }
    };
    let actual_sha1 = <Sha1 as Sha1Digest>::digest(&bytes);
    let source_sha1_hex = hex::encode(actual_sha1);
    if source_sha1
      .as_deref()
      .is_some_and(|expected| !expected.eq_ignore_ascii_case(&source_sha1_hex))
    {
      report.invalid_upload_ids.push(upload.id);
      continue;
    }
    let content_hash = hex::encode(<Sha256 as Sha2Digest>::digest(&bytes));
    let existing_target_id = read_mapping(pool, run.id, "upload", &upload.id.to_string())
      .await
      .map_err(convert_lemmy_error)?;
    let (media, resumed) = if let Some(target_id) = existing_target_id {
      let media =
        diesel::sql_query("SELECT id, canonical_pictrs_alias FROM media_object WHERE id = $1")
          .bind::<BigInt, _>(target_id)
          .get_result::<TargetMediaObject>(&mut get_conn(pool).await?)
          .await
          .context("mapped target media object no longer exists")?;
      (media, true)
    } else {
      let file_name = Path::new(&upload.original_filename)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("imported-image");
      let part = reqwest::multipart::Part::bytes(bytes.clone())
        .file_name(file_name.to_string())
        .mime_str("application/octet-stream")?;
      let response = client
        .post(&image_endpoint)
        .query(&[("allow_video", "false")])
        .multipart(reqwest::multipart::Form::new().part("images[]", part))
        .send()
        .await;
      let Ok(response) = response else {
        report.invalid_upload_ids.push(upload.id);
        continue;
      };
      let Ok(response) = response.error_for_status() else {
        report.invalid_upload_ids.push(upload.id);
        continue;
      };
      let Ok(pictrs_response) = response.json::<PictrsUploadResponse>().await else {
        report.invalid_upload_ids.push(upload.id);
        continue;
      };
      let _pictrs_message = &pictrs_response.msg;
      let Some(image) = pictrs_response.files.into_iter().next() else {
        report.invalid_upload_ids.push(upload.id);
        continue;
      };
      let pictrs_alias = image.file;
      let media_form = MediaObjectInsertForm {
        content_hash,
        canonical_pictrs_alias: pictrs_alias.clone(),
        size_bytes: upload.filesize,
        width: i32::from(image.details.width),
        height: i32::from(image.details.height),
        content_type: image.details.content_type,
        blurhash: image.details.blurhash,
      };
      let (media, created) = MediaObject::create_or_read(pool, &media_form)
        .await
        .map_err(convert_lemmy_error)?;
      if !created && media.canonical_pictrs_alias != pictrs_alias {
        delete_pictrs_alias(
          &client,
          &pictrs.url,
          pictrs.api_key.as_deref(),
          &pictrs_alias,
        )
        .await;
      }
      record_mapping(pool, run.id, "upload", &upload.id.to_string(), media.id)
        .await
        .map_err(convert_lemmy_error)?;
      (
        TargetMediaObject {
          id: media.id,
          canonical_pictrs_alias: media.canonical_pictrs_alias,
        },
        false,
      )
    };
    let owner_person_id = read_mapping(pool, run.id, "user", &upload.user_id.to_string())
      .await
      .map_err(convert_lemmy_error)?
      .map(|id| lemmy_db_schema_file::PersonId(id as i32));
    MediaReference::ensure(
      pool,
      &MediaReferenceInsertForm {
        local_site_id: site_id,
        media_id: media.id,
        owner_person_id,
        post_id: None,
        purpose: "other".to_string(),
      },
    )
    .await
    .map_err(convert_lemmy_error)?;
    if resumed {
      report.resumed_image_count += 1;
    } else {
      report.imported_image_count += 1;
    }
    let target_url = format!(
      "{}://{}/api/v4/image/{}",
      SETTINGS.get_protocol_string(),
      target_site.hostname,
      media.canonical_pictrs_alias
    );
    media_items.push(ImportedMediaItem {
      media_id: Some(media.id),
      target_url,
      replacements: source_tokens,
    });
  }
  media_items.sort_by(|left, right| {
    let left_len = left
      .replacements
      .iter()
      .map(String::len)
      .max()
      .unwrap_or_default();
    let right_len = right
      .replacements
      .iter()
      .map(String::len)
      .max()
      .unwrap_or_default();
    right_len.cmp(&left_len)
  });

  rewrite_imported_media_references(
    &mut source,
    pool,
    run.id,
    site_id,
    &media_items,
    &all_source_tokens,
    &mut report,
  )
  .await?;
  let source_avatars = diesel::sql_query(
    "SELECT id AS user_id, uploaded_avatar_id FROM users WHERE uploaded_avatar_id IS NOT NULL ORDER BY id",
  )
  .load::<SourceAvatar>(&mut source)
  .await
  .context("failed to read Discourse user avatar references")?;
  for avatar in source_avatars {
    let source_user_id = avatar.user_id;
    let Some(target_person_id) = read_mapping(pool, run.id, "user", &source_user_id.to_string())
      .await
      .map_err(convert_lemmy_error)?
    else {
      report
        .unresolved_avatar_source_user_ids
        .push(source_user_id);
      continue;
    };
    let Some(target_media_id) = read_mapping(
      pool,
      run.id,
      "upload",
      &avatar.uploaded_avatar_id.to_string(),
    )
    .await
    .map_err(convert_lemmy_error)?
    else {
      report
        .unresolved_avatar_source_user_ids
        .push(source_user_id);
      continue;
    };
    let media =
      diesel::sql_query("SELECT id, canonical_pictrs_alias FROM media_object WHERE id = $1")
        .bind::<BigInt, _>(target_media_id)
        .get_result::<TargetMediaObject>(&mut get_conn(pool).await?)
        .await
        .context("mapped Discourse avatar media object no longer exists")?;
    let target_avatar_url = format!(
      "{}://{}/api/v4/image/{}",
      SETTINGS.get_protocol_string(),
      target_site.hostname,
      media.canonical_pictrs_alias
    );
    let updated = diesel::sql_query(
      "UPDATE person SET avatar = $1 WHERE id = $2 AND origin_local_site_id = $3",
    )
    .bind::<Text, _>(target_avatar_url)
    .bind::<Integer, _>(target_person_id as i32)
    .bind::<Integer, _>(site_id.0)
    .execute(&mut get_conn(pool).await?)
    .await
    .context("failed to attach imported media to a local user avatar")?;
    if updated == 1 {
      report.rewritten_avatar_count += 1;
    } else {
      report
        .unresolved_avatar_source_user_ids
        .push(source_user_id);
    }
  }
  let source_category_images = diesel::sql_query(
    "SELECT id AS category_id, uploaded_logo_id, uploaded_background_id \
     FROM categories WHERE uploaded_logo_id IS NOT NULL OR uploaded_background_id IS NOT NULL \
     ORDER BY id",
  )
  .load::<SourceCategoryImages>(&mut source)
  .await
  .context("failed to read Discourse category icon and banner references")?;
  for category_images in source_category_images {
    let Some(target_community_id) = read_mapping(
      pool,
      run.id,
      "category_community",
      &category_images.category_id.to_string(),
    )
    .await
    .map_err(convert_lemmy_error)?
    else {
      report
        .unresolved_category_image_ids
        .push(category_images.category_id);
      continue;
    };
    for (source_upload_id, field) in [
      (category_images.uploaded_logo_id, "icon"),
      (category_images.uploaded_background_id, "banner"),
    ] {
      let Some(source_upload_id) = source_upload_id else {
        continue;
      };
      let Some(target_media_id) =
        read_mapping(pool, run.id, "upload", &source_upload_id.to_string())
          .await
          .map_err(convert_lemmy_error)?
      else {
        report
          .unresolved_category_image_ids
          .push(category_images.category_id);
        continue;
      };
      let media =
        diesel::sql_query("SELECT id, canonical_pictrs_alias FROM media_object WHERE id = $1")
          .bind::<BigInt, _>(target_media_id)
          .get_result::<TargetMediaObject>(&mut get_conn(pool).await?)
          .await
          .context("mapped Discourse category image no longer exists")?;
      let target_image_url = format!(
        "{}://{}/api/v4/image/{}",
        SETTINGS.get_protocol_string(),
        target_site.hostname,
        media.canonical_pictrs_alias
      );
      let update_sql = match field {
        "icon" => "UPDATE community SET icon = $1 WHERE id = $2 AND origin_local_site_id = $3",
        "banner" => "UPDATE community SET banner = $1 WHERE id = $2 AND origin_local_site_id = $3",
        _ => unreachable!(),
      };
      let updated = diesel::sql_query(update_sql)
        .bind::<Text, _>(target_image_url)
        .bind::<Integer, _>(target_community_id as i32)
        .bind::<Integer, _>(site_id.0)
        .execute(&mut get_conn(pool).await?)
        .await
        .context("failed to attach imported media to a community image")?;
      if updated == 1 {
        if field == "icon" {
          report.rewritten_category_icon_count += 1;
        } else {
          report.rewritten_category_banner_count += 1;
        }
      } else {
        report
          .unresolved_category_image_ids
          .push(category_images.category_id);
      }
    }
  }
  report.retained_secure_upload_ids.sort_unstable();
  report.retained_secure_upload_ids.dedup();
  report.retained_non_image_upload_ids.sort_unstable();
  report.retained_non_image_upload_ids.dedup();
  report.missing_archive_upload_ids.sort_unstable();
  report.missing_archive_upload_ids.dedup();
  report.invalid_upload_ids.sort_unstable();
  report.invalid_upload_ids.dedup();
  report.unresolved_content_source_post_ids.sort_unstable();
  report.unresolved_content_source_post_ids.dedup();
  report
    .unresolved_private_message_source_post_ids
    .sort_unstable();
  report.unresolved_private_message_source_post_ids.dedup();
  report.unresolved_avatar_source_user_ids.sort_unstable();
  report.unresolved_avatar_source_user_ids.dedup();
  report.unresolved_category_image_ids.sort_unstable();
  report.unresolved_category_image_ids.dedup();
  let unresolved = !report.missing_archive_upload_ids.is_empty()
    || !report.invalid_upload_ids.is_empty()
    || !report.unresolved_content_source_post_ids.is_empty()
    || !report.unresolved_private_message_source_post_ids.is_empty();
  update_phase(
    pool,
    run.id,
    "media",
    None,
    if unresolved { "failed" } else { "completed" },
    (report.imported_image_count
      + report.resumed_image_count
      + report.imported_attachment_count
      + report.resumed_attachment_count) as i64,
  )
  .await
  .map_err(convert_lemmy_error)?;
  store_run_report(pool, run.id, &serde_json::json!({"media": report}))
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, if unresolved { "failed" } else { "running" })
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

async fn validate_discussion_relationships(
  source: &mut AsyncPgConnection,
  pool: &mut DbPool<'_>,
  run_id: i64,
  site_id: LocalSiteId,
) -> Result<i64> {
  let mut invalid_count = 0_i64;
  let mut last_id = 0_i32;
  loop {
    let batch = diesel::sql_query(
      "SELECT p.id, p.topic_id, p.user_id, t.category_id, p.post_number, \
       p.reply_to_post_number, parent.id AS parent_source_post_id \
       FROM posts p JOIN topics t ON t.id = p.topic_id \
       LEFT JOIN posts parent ON parent.topic_id = p.topic_id \
         AND parent.post_number = p.reply_to_post_number AND parent.post_type = 1 \
       WHERE p.id > $1 AND t.archetype <> 'private_message' AND p.post_type = 1 \
       ORDER BY p.id LIMIT 500",
    )
    .bind::<Integer, _>(last_id)
    .load::<SourceDiscussionRelation>(source)
    .await
    .context("failed to read source discussion relationships for validation")?;
    if batch.is_empty() {
      break;
    }
    last_id = batch.last().map_or(last_id, |row| row.id);
    let source_post_ids = batch.iter().map(|row| row.id).collect::<Vec<_>>();
    let topic_ids = batch.iter().map(|row| row.topic_id).collect::<Vec<_>>();
    let user_ids = batch.iter().map(|row| row.user_id).collect::<Vec<_>>();
    let category_ids = batch.iter().map(|row| row.category_id).collect::<Vec<_>>();
    let post_numbers = batch.iter().map(|row| row.post_number).collect::<Vec<_>>();
    let reply_to_numbers = batch
      .iter()
      .map(|row| row.reply_to_post_number)
      .collect::<Vec<_>>();
    let parent_source_post_ids = batch
      .iter()
      .map(|row| row.parent_source_post_id)
      .collect::<Vec<_>>();
    let invalid = diesel::sql_query(
      "WITH source_refs AS ( \
         SELECT * FROM unnest($3::integer[], $4::integer[], $5::integer[], \
           $6::integer[], $7::integer[], $8::integer[], $9::integer[]) \
           AS r(source_post_id, topic_id, user_id, category_id, post_number, \
             reply_to_post_number, parent_source_post_id) \
       ) \
       SELECT COUNT(*) AS count FROM source_refs r \
       LEFT JOIN discourse_import_mapping object_map ON object_map.run_id = $1 \
         AND object_map.entity_type = 'source_discussion_post' \
         AND object_map.source_id = r.source_post_id::text \
       LEFT JOIN discourse_import_mapping topic_map ON topic_map.run_id = $1 \
         AND topic_map.entity_type = 'topic' AND topic_map.source_id = r.topic_id::text \
       LEFT JOIN discourse_import_mapping user_map ON user_map.run_id = $1 \
         AND user_map.entity_type = 'user' AND user_map.source_id = r.user_id::text \
       LEFT JOIN discourse_import_mapping category_map ON category_map.run_id = $1 \
         AND category_map.entity_type = 'category_community' \
         AND category_map.source_id = r.category_id::text \
       LEFT JOIN discourse_import_mapping parent_map ON parent_map.run_id = $1 \
         AND parent_map.entity_type = 'source_discussion_post' \
         AND parent_map.source_id = r.parent_source_post_id::text \
       LEFT JOIN comment c ON c.id = object_map.target_id \
       WHERE object_map.target_id IS NULL OR topic_map.target_id IS NULL \
         OR user_map.target_id IS NULL OR category_map.target_id IS NULL \
         OR (r.post_number = 1 AND (topic_map.target_id IS NULL \
           OR NOT EXISTS (SELECT 1 FROM post p JOIN post_site ps ON ps.post_id = p.id \
             WHERE p.id = topic_map.target_id AND p.creator_id = user_map.target_id \
               AND p.community_id = category_map.target_id \
               AND p.origin_local_site_id = $2 AND ps.local_site_id = $2))) \
         OR (r.post_number <> 1 AND (c.id IS NULL \
           OR c.post_id <> topic_map.target_id OR c.creator_id <> user_map.target_id \
           OR c.community_id <> category_map.target_id \
           OR c.origin_local_site_id IS DISTINCT FROM $2 \
           OR NOT EXISTS (SELECT 1 FROM comment_site cs WHERE cs.local_site_id = $2 \
             AND cs.comment_id = c.id) \
           OR (COALESCE(r.reply_to_post_number, 1) <= 1 AND nlevel(c.path) <> 2) \
           OR (COALESCE(r.reply_to_post_number, 1) > 1 AND (parent_map.target_id IS NULL \
             OR subpath(c.path, nlevel(c.path) - 2, 1)::text <> parent_map.target_id::text))))",
    )
    .bind::<BigInt, _>(run_id)
    .bind::<Integer, _>(site_id.0)
    .bind::<Array<Integer>, _>(source_post_ids)
    .bind::<Array<Integer>, _>(topic_ids)
    .bind::<Array<Integer>, _>(user_ids)
    .bind::<Array<Nullable<Integer>>, _>(category_ids)
    .bind::<Array<Integer>, _>(post_numbers)
    .bind::<Array<Nullable<Integer>>, _>(reply_to_numbers)
    .bind::<Array<Nullable<Integer>>, _>(parent_source_post_ids)
    .get_result::<CountResult>(&mut get_conn(pool).await?)
    .await
    .context("failed to validate mapped discussion relationships")?;
    invalid_count += invalid.count;
  }
  Ok(invalid_count)
}

async fn validate_category_memberships(
  source: &mut AsyncPgConnection,
  pool: &mut DbPool<'_>,
  run_id: i64,
) -> Result<i64> {
  let categories = diesel::sql_query("SELECT id, parent_category_id FROM categories ORDER BY id")
    .load::<SourceCategoryLink>(source)
    .await
    .context("failed to read source category hierarchy for validation")?;
  let mut children = std::collections::HashMap::<i32, Vec<i32>>::new();
  for category in &categories {
    if let Some(parent_id) = category.parent_category_id {
      children.entry(parent_id).or_default().push(category.id);
    }
  }
  let multi_mappings = diesel::sql_query(
    "SELECT source_id, target_id FROM discourse_import_mapping \
     WHERE run_id = $1 AND entity_type = 'category_multi_community'",
  )
  .bind::<BigInt, _>(run_id)
  .load::<TargetSourceMapping>(&mut get_conn(pool).await?)
  .await
  .context("failed to read mapped parent categories for validation")?;
  let multi_by_source = multi_mappings
    .iter()
    .filter_map(|mapping| {
      mapping
        .source_id
        .parse::<i32>()
        .ok()
        .map(|source_id| (source_id, mapping.target_id as i32))
    })
    .collect::<HashMap<_, _>>();
  let community_mappings = diesel::sql_query(
    "SELECT source_id, target_id FROM discourse_import_mapping \
     WHERE run_id = $1 AND entity_type = 'category_community'",
  )
  .bind::<BigInt, _>(run_id)
  .load::<TargetSourceMapping>(&mut get_conn(pool).await?)
  .await
  .context("failed to read mapped communities for category validation")?;
  let community_by_source = community_mappings
    .iter()
    .filter_map(|mapping| {
      mapping
        .source_id
        .parse::<i32>()
        .ok()
        .map(|source_id| (source_id, mapping.target_id as i32))
    })
    .collect::<HashMap<_, _>>();
  let multi_ids = multi_by_source.values().copied().collect::<Vec<_>>();
  let entries = if multi_ids.is_empty() {
    Vec::new()
  } else {
    diesel::sql_query(
      "SELECT multi_community_id, community_id FROM multi_community_entry \
       WHERE multi_community_id = ANY($1)",
    )
    .bind::<Array<Integer>, _>(multi_ids)
    .load::<TargetMultiCommunityEntry>(&mut get_conn(pool).await?)
    .await
    .context("failed to read target multi-community entries for validation")?
  };
  let entry_set = entries
    .into_iter()
    .map(|entry| (entry.multi_community_id, entry.community_id))
    .collect::<HashSet<_>>();

  let mut invalid_count = 0_i64;
  for category in &categories {
    let Some(descendant_ids) = children.get(&category.id) else {
      continue;
    };
    let Some(multi_id) = multi_by_source.get(&category.id).copied() else {
      invalid_count += 1;
      continue;
    };
    let mut stack = descendant_ids.clone();
    while let Some(descendant_id) = stack.pop() {
      if let Some(nested_children) = children.get(&descendant_id) {
        stack.extend(nested_children.iter().copied());
      }
      if let Some(community_id) = community_by_source.get(&descendant_id)
        && !entry_set.contains(&(multi_id, *community_id))
      {
        invalid_count += 1;
      }
    }
  }
  Ok(invalid_count)
}

pub async fn validate_import(
  source_database_url: &str,
  site_id: i32,
  source_fingerprint: &str,
  discourse_version: &str,
  schema_version: i64,
  archived_upload_file_count: usize,
) -> Result<ImportValidationReport> {
  let mut source = AsyncPgConnection::establish(source_database_url)
    .await
    .context("failed to connect to the restored Discourse source database")?;
  let target_pool = build_db_pool().map_err(convert_lemmy_error)?;
  let pool = &mut DbPool::Pool(&target_pool);
  let site_id = LocalSiteId(site_id);
  let target_site = load_target_site(pool, site_id).await?;
  if target_site.federation_enabled {
    bail!("target site federation must remain disabled through import validation");
  }
  let run = get_or_create_run(
    pool,
    site_id,
    source_fingerprint,
    discourse_version,
    schema_version,
  )
  .await
  .map_err(convert_lemmy_error)?;
  let invalid_discussion_relationship_count =
    validate_discussion_relationships(&mut source, pool, run.id, site_id).await?;
  let invalid_category_membership_count =
    validate_category_memberships(&mut source, pool, run.id).await?;
  let mut source_counts = BTreeMap::new();
  let queries = [
    ("users", "SELECT COUNT(*) AS count FROM users"),
    ("categories", "SELECT COUNT(*) AS count FROM categories"),
    (
      "parent_categories",
      "SELECT COUNT(*) AS count FROM categories c WHERE EXISTS \
       (SELECT 1 FROM categories child WHERE child.parent_category_id = c.id)",
    ),
    (
      "community_categories",
      "SELECT COUNT(*) AS count FROM categories c WHERE NOT EXISTS \
       (SELECT 1 FROM categories child WHERE child.parent_category_id = c.id) \
       OR EXISTS (SELECT 1 FROM topics t WHERE t.category_id = c.id AND t.deleted_at IS NULL)",
    ),
    (
      "public_topics",
      "SELECT COUNT(DISTINCT t.id) AS count FROM topics t JOIN posts p ON p.topic_id = t.id \
       WHERE t.archetype <> 'private_message' AND p.post_type = 1 AND p.post_number = 1",
    ),
    (
      "public_posts",
      "SELECT COUNT(*) AS count FROM topics t JOIN posts p ON p.topic_id = t.id \
       WHERE t.archetype <> 'private_message' AND p.post_type = 1",
    ),
    (
      "private_message_posts",
      "SELECT COUNT(*) AS count FROM topics t JOIN posts p ON p.topic_id = t.id \
       WHERE t.archetype = 'private_message' AND p.post_type = 1",
    ),
    (
      "private_message_copies",
      "SELECT COALESCE(SUM(participant_count), 0)::bigint AS count FROM ( \
         SELECT p.id, GREATEST(COUNT(DISTINCT participants.user_id) - 1, 0) AS participant_count \
         FROM topics t JOIN posts p ON p.topic_id = t.id \
         CROSS JOIN LATERAL ( \
           SELECT user_id FROM topic_allowed_users WHERE topic_id = t.id \
           UNION SELECT user_id FROM posts WHERE topic_id = t.id \
         ) participants \
         WHERE t.archetype = 'private_message' AND p.post_type = 1 \
         GROUP BY p.id \
       ) per_recipient",
    ),
    (
      "active_likes",
      "SELECT COUNT(*) AS count FROM post_actions pa JOIN posts p ON p.id = pa.post_id \
       JOIN topics t ON t.id = p.topic_id WHERE pa.deleted_at IS NULL \
       AND pa.post_action_type_id = 2 AND t.archetype <> 'private_message' AND p.post_type = 1",
    ),
    (
      "supported_bookmarks",
      "SELECT COUNT(*) AS count FROM bookmarks b WHERE \
       (b.bookmarkable_type = 'Topic' AND EXISTS (SELECT 1 FROM topics t \
        WHERE t.id = b.bookmarkable_id AND t.archetype <> 'private_message')) \
       OR (b.bookmarkable_type = 'Post' AND EXISTS (SELECT 1 FROM posts p JOIN topics t \
        ON t.id = p.topic_id WHERE p.id = b.bookmarkable_id \
        AND p.post_type = 1 AND t.archetype <> 'private_message'))",
    ),
    ("upload_records", "SELECT COUNT(*) AS count FROM uploads"),
    (
      "public_topic_tags",
      "SELECT COUNT(*) AS count FROM topic_tags tt JOIN topics t ON t.id = tt.topic_id \
       WHERE t.archetype <> 'private_message' AND t.deleted_at IS NULL \
       AND EXISTS (SELECT 1 FROM posts p WHERE p.topic_id = t.id \
         AND p.post_number = 1 AND p.post_type = 1)",
    ),
  ];
  for (name, query) in queries {
    let result = diesel::sql_query(query)
      .get_result::<CountResult>(&mut source)
      .await
      .with_context(|| format!("failed to count source {name}"))?;
    source_counts.insert(name.to_string(), result.count);
  }
  let chat_tables = diesel::sql_query(
    "SELECT to_regclass('public.chat_messages') IS NOT NULL AS has_messages, \
     to_regclass('public.chat_channels') IS NOT NULL AS has_channels, \
     to_regclass('public.direct_message_channels') IS NOT NULL AS has_dm_channels, \
     to_regclass('public.direct_message_users') IS NOT NULL AS has_dm_users",
  )
  .get_result::<SourceChatTables>(&mut source)
  .await
  .context("failed to inspect optional Discourse Chat tables during validation")?;
  let (chat_message_count, chat_message_copy_count) = if chat_tables.has_messages
    && chat_tables.has_channels
    && chat_tables.has_dm_channels
    && chat_tables.has_dm_users
  {
    #[derive(QueryableByName)]
    struct ChatCounts {
      #[diesel(sql_type = BigInt)]
      messages: i64,
      #[diesel(sql_type = BigInt)]
      copies: i64,
    }
    let result = diesel::sql_query(
      "SELECT COUNT(*)::bigint AS messages, COALESCE(SUM(recipient_count), 0)::bigint AS copies \
       FROM (SELECT m.id, COUNT(DISTINCT dmu.user_id) FILTER (WHERE dmu.user_id <> m.user_id) AS recipient_count \
         FROM chat_messages m \
         JOIN chat_channels cc ON cc.id = m.chat_channel_id AND cc.chatable_type = 'DirectMessage' \
         JOIN direct_message_channels dmc ON dmc.id = cc.chatable_id \
         LEFT JOIN direct_message_users dmu ON dmu.direct_message_channel_id = dmc.id \
         GROUP BY m.id) direct_messages",
    )
    .get_result::<ChatCounts>(&mut source)
    .await
    .context("failed to count Discourse direct-message chat rows")?;
    (result.messages, result.copies)
  } else {
    (0, 0)
  };
  source_counts.insert("chat_direct_messages".to_string(), chat_message_count);
  source_counts.insert(
    "chat_direct_message_copies".to_string(),
    chat_message_copy_count,
  );

  let mut mapped_counts = BTreeMap::new();
  for entity_type in [
    "user",
    "category_community",
    "category_multi_community",
    "topic",
    "source_discussion_post",
    "private_message",
    "private_message_sender",
    "private_message_recipient",
    "like",
    "like_actor",
    "bookmark",
    "bookmark_actor",
    "category_tag",
    "topic_tag",
    "upload",
    "file_upload",
  ] {
    let result = diesel::sql_query(
      "SELECT COUNT(*) AS count FROM discourse_import_mapping \
       WHERE run_id = $1 AND entity_type = $2",
    )
    .bind::<BigInt, _>(run.id)
    .bind::<Text, _>(entity_type)
    .get_result::<CountResult>(&mut get_conn(pool).await?)
    .await
    .with_context(|| format!("failed to count target mappings for {entity_type}"))?;
    mapped_counts.insert(entity_type.to_string(), result.count);
  }

  let mut invalid_target_reference_counts = BTreeMap::new();
  let invalid_queries = [
    (
      "users",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       LEFT JOIN person p ON p.id = m.target_id \
       LEFT JOIN local_user lu ON lu.person_id = p.id \
       WHERE m.run_id = $1 AND m.entity_type = 'user' AND \
       (p.id IS NULL OR p.origin_local_site_id IS DISTINCT FROM $2 \
        OR lu.person_id IS NULL OR lu.local_site_id <> $2 \
        OR NOT EXISTS (SELECT 1 FROM person_site ps WHERE ps.local_site_id = $2 AND ps.person_id = p.id))",
    ),
    (
      "communities",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       LEFT JOIN community c ON c.id = m.target_id \
       WHERE m.run_id = $1 AND m.entity_type = 'category_community' AND \
       (c.id IS NULL OR c.origin_local_site_id IS DISTINCT FROM $2 \
        OR NOT EXISTS (SELECT 1 FROM community_site cs WHERE cs.local_site_id = $2 AND cs.community_id = c.id))",
    ),
    (
      "multi_communities",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       LEFT JOIN multi_community mc ON mc.id = m.target_id \
       WHERE m.run_id = $1 AND m.entity_type = 'category_multi_community' AND \
       (mc.id IS NULL OR mc.origin_local_site_id IS DISTINCT FROM $2 \
        OR NOT EXISTS (SELECT 1 FROM multi_community_site ms WHERE ms.local_site_id = $2 AND ms.multi_community_id = mc.id))",
    ),
    (
      "topics",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       LEFT JOIN post p ON p.id = m.target_id \
       WHERE m.run_id = $1 AND m.entity_type = 'topic' AND \
       (p.id IS NULL OR p.origin_local_site_id IS DISTINCT FROM $2 \
        OR NOT EXISTS (SELECT 1 FROM post_site ps WHERE ps.local_site_id = $2 AND ps.post_id = p.id))",
    ),
    (
      "private_messages",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       LEFT JOIN private_message pm ON pm.id = m.target_id \
       WHERE m.run_id = $1 AND m.entity_type = 'private_message' AND \
       (pm.id IS NULL OR pm.origin_local_site_id IS DISTINCT FROM $2 \
        OR NOT EXISTS (SELECT 1 FROM private_message_site ps WHERE ps.local_site_id = $2 \
          AND ps.private_message_id = pm.id AND ps.role IN ('sender', 'both')) \
        OR NOT EXISTS (SELECT 1 FROM private_message_site pr WHERE pr.local_site_id = $2 \
          AND pr.private_message_id = pm.id AND pr.role IN ('recipient', 'both')) \
        OR NOT EXISTS (SELECT 1 FROM discourse_import_mapping sm WHERE sm.run_id = m.run_id \
          AND sm.entity_type = 'private_message_sender' \
          AND sm.source_id IN (split_part(m.source_id, ':', 2), \
            '0:' || split_part(m.source_id, ':', 2)) \
          AND sm.target_id = pm.creator_id) \
        OR NOT EXISTS (SELECT 1 FROM discourse_import_mapping rm WHERE rm.run_id = m.run_id \
          AND rm.entity_type = 'private_message_recipient' AND rm.source_id = m.source_id \
          AND rm.target_id = pm.recipient_id))",
    ),
    (
      "votes_and_bookmarks",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       WHERE m.run_id = $1 AND m.entity_type IN ('like', 'bookmark') AND NOT ( \
         (m.entity_type = 'like' AND ( \
           (EXISTS (SELECT 1 FROM post p JOIN post_site ps ON ps.post_id = p.id \
             WHERE p.id = m.target_id AND p.origin_local_site_id = $2 AND ps.local_site_id = $2) \
            AND EXISTS (SELECT 1 FROM discourse_import_mapping va JOIN post_actions pa \
             ON pa.person_id = va.target_id AND pa.post_id = m.target_id \
             WHERE va.run_id = m.run_id AND va.entity_type = 'like_actor' \
               AND va.source_id = m.source_id AND pa.vote_is_upvote IS TRUE)) \
           OR (EXISTS (SELECT 1 FROM comment c JOIN comment_site cs ON cs.comment_id = c.id \
             WHERE c.id = m.target_id AND c.origin_local_site_id = $2 AND cs.local_site_id = $2) \
            AND EXISTS (SELECT 1 FROM discourse_import_mapping va JOIN comment_actions ca \
             ON ca.person_id = va.target_id AND ca.comment_id = m.target_id \
             WHERE va.run_id = m.run_id AND va.entity_type = 'like_actor' \
               AND va.source_id = m.source_id AND ca.vote_is_upvote IS TRUE)))) \
         OR (m.entity_type = 'bookmark' AND ( \
           (EXISTS (SELECT 1 FROM post p JOIN post_site ps ON ps.post_id = p.id \
             WHERE p.id = m.target_id AND p.origin_local_site_id = $2 AND ps.local_site_id = $2) \
            AND EXISTS (SELECT 1 FROM discourse_import_mapping ba JOIN post_actions pa \
             ON pa.person_id = ba.target_id AND pa.post_id = m.target_id \
             WHERE ba.run_id = m.run_id AND ba.entity_type = 'bookmark_actor' \
               AND ba.source_id = m.source_id AND pa.saved_at IS NOT NULL)) \
           OR (EXISTS (SELECT 1 FROM comment c JOIN comment_site cs ON cs.comment_id = c.id \
             WHERE c.id = m.target_id AND c.origin_local_site_id = $2 AND cs.local_site_id = $2) \
            AND EXISTS (SELECT 1 FROM discourse_import_mapping ba JOIN comment_actions ca \
             ON ca.person_id = ba.target_id AND ca.comment_id = m.target_id \
             WHERE ba.run_id = m.run_id AND ba.entity_type = 'bookmark_actor' \
               AND ba.source_id = m.source_id AND ca.saved_at IS NOT NULL)))))",
    ),
    (
      "tags",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       WHERE m.run_id = $1 AND m.entity_type IN ('category_tag', 'topic_tag') \
       AND NOT EXISTS (SELECT 1 FROM community_tag ct JOIN community c ON c.id = ct.community_id \
         JOIN community_site cs ON cs.community_id = c.id \
         WHERE ct.id = m.target_id AND c.origin_local_site_id = $2 AND cs.local_site_id = $2)",
    ),
    (
      "uploads",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       WHERE m.run_id = $1 AND m.entity_type = 'upload' AND NOT EXISTS ( \
         SELECT 1 FROM media_object mo JOIN media_reference mr ON mr.media_id = mo.id \
         WHERE mo.id = m.target_id AND mr.local_site_id = $2)",
    ),
    (
      "generic_file_uploads",
      "SELECT COUNT(*) AS count FROM discourse_import_mapping m \
       LEFT JOIN file_reference fr ON fr.id = m.target_id \
       LEFT JOIN file_object fo ON fo.id = fr.file_id \
       WHERE m.run_id = $1 AND m.entity_type = 'file_upload' AND \
       (fr.id IS NULL OR fr.local_site_id <> $2 OR fo.id IS NULL OR \
        NOT EXISTS (SELECT 1 FROM person_site ps WHERE ps.person_id = fr.owner_person_id \
          AND ps.local_site_id = $2))",
    ),
  ];
  for (name, query) in invalid_queries {
    let result = diesel::sql_query(query)
      .bind::<BigInt, _>(run.id)
      .bind::<Integer, _>(site_id.0)
      .get_result::<CountResult>(&mut get_conn(pool).await?)
      .await
      .with_context(|| format!("failed to validate target references for {name}"))?;
    invalid_target_reference_counts.insert(name.to_string(), result.count);
  }
  let stored_file_keys = diesel::sql_query(
    "SELECT DISTINCT fo.storage_key FROM discourse_import_mapping m \
     JOIN file_reference fr ON fr.id = m.target_id \
     JOIN file_object fo ON fo.id = fr.file_id \
     WHERE m.run_id = $1 AND m.entity_type = 'file_upload' ORDER BY fo.storage_key",
  )
  .bind::<BigInt, _>(run.id)
  .load::<StoredFileKey>(&mut get_conn(pool).await?)
  .await
  .context("failed to read imported generic file objects for storage validation")?;
  let mut invalid_file_object_count = 0_i64;
  let mut file_storage_validation_error = None;
  if !stored_file_keys.is_empty() {
    if let Some(config) = SETTINGS.s3() {
      let storage = build_s3_client(&config);
      for file in &stored_file_keys {
        match storage
          .head_object()
          .bucket(&config.bucket)
          .key(&file.storage_key)
          .send()
          .await
        {
          Ok(_) => {}
          Err(error)
            if error
              .raw_response()
              .is_some_and(|response| response.status().as_u16() == 404) =>
          {
            invalid_file_object_count += 1;
          }
          Err(error) => {
            invalid_file_object_count = stored_file_keys.len() as i64;
            file_storage_validation_error = Some(format!(
              "could not verify generic file storage object: {error}"
            ));
            break;
          }
        }
      }
    } else {
      invalid_file_object_count = stored_file_keys.len() as i64;
      file_storage_validation_error = Some(
        "generic file storage is not configured, so imported attachment objects cannot be verified"
          .to_string(),
      );
    }
  }
  invalid_target_reference_counts.insert(
    "generic_file_objects".to_string(),
    invalid_file_object_count,
  );
  invalid_target_reference_counts.insert(
    "discussion_relationships".to_string(),
    invalid_discussion_relationship_count,
  );
  invalid_target_reference_counts.insert(
    "category_memberships".to_string(),
    invalid_category_membership_count,
  );

  let comparisons = [
    ("users", "user"),
    ("community_categories", "category_community"),
    ("parent_categories", "category_multi_community"),
    ("public_topics", "topic"),
    ("active_likes", "like"),
    ("supported_bookmarks", "bookmark"),
    ("public_topic_tags", "topic_tag"),
  ];
  let mut blocking_reasons = Vec::new();
  if invalid_discussion_relationship_count > 0 {
    blocking_reasons.push(format!(
      "{invalid_discussion_relationship_count} discussion posts or replies have invalid target relationships"
    ));
  }
  if let Some(error) = file_storage_validation_error {
    blocking_reasons.push(error);
  }
  #[derive(QueryableByName)]
  struct FailedImportPhase {
    #[diesel(sql_type = Text)]
    phase: String,
  }
  let failed_phases = diesel::sql_query(
    "SELECT phase FROM discourse_import_phase WHERE run_id = $1 AND status = 'failed' \
     AND phase <> 'validation' ORDER BY phase",
  )
  .bind::<BigInt, _>(run.id)
  .load::<FailedImportPhase>(&mut get_conn(pool).await?)
  .await
  .context("failed to inspect failed import phases")?;
  for failed_phase in failed_phases {
    blocking_reasons.push(format!(
      "import phase '{}' has a failed status",
      failed_phase.phase
    ));
  }
  for (source_name, mapped_name) in comparisons {
    let expected = source_counts.get(source_name).copied().unwrap_or_default();
    let actual = mapped_counts.get(mapped_name).copied().unwrap_or_default();
    if expected != actual {
      blocking_reasons.push(format!(
        "{source_name} count is {expected}, but {mapped_name} mappings contain {actual}"
      ));
    }
  }
  let expected_private_message_copies =
    source_counts["private_message_copies"] + source_counts["chat_direct_message_copies"];
  if expected_private_message_copies != mapped_counts["private_message"] {
    blocking_reasons.push(format!(
      "private message copy count is {expected_private_message_copies}, but private_message mappings contain {}",
      mapped_counts["private_message"]
    ));
  }
  let expected_public_post_mappings = source_counts["public_posts"];
  let actual_public_post_mappings = mapped_counts["source_discussion_post"];
  if expected_public_post_mappings != actual_public_post_mappings {
    blocking_reasons.push(format!(
      "public_posts count is {expected_public_post_mappings}, but topic and reply mappings contain {actual_public_post_mappings}"
    ));
  }
  if source_counts["active_likes"] + source_counts["supported_bookmarks"]
    != mapped_counts["like"] + mapped_counts["bookmark"]
  {
    blocking_reasons.push(format!(
      "supported interactions count is {}, but like and bookmark mappings contain {}",
      source_counts["active_likes"] + source_counts["supported_bookmarks"],
      mapped_counts["like"] + mapped_counts["bookmark"]
    ));
  }
  let phase_requirements = [
    ("users", "users"),
    ("categories", "categories_communities"),
    ("parent categories", "categories_multi_communities"),
    ("public discussions", "discussions"),
    ("private messages", "private_messages"),
    ("votes and bookmarks", "interactions"),
    ("topic tags", "tags"),
    ("image uploads", "media"),
  ];
  for (label, phase) in phase_requirements {
    let required_count = match phase {
      "users" => source_counts["users"],
      "categories_communities" => source_counts["categories"],
      "categories_multi_communities" => source_counts["parent_categories"],
      "discussions" => source_counts["public_posts"],
      "private_messages" => {
        source_counts["private_message_posts"] + source_counts["chat_direct_messages"]
      }
      "interactions" => source_counts["active_likes"] + source_counts["supported_bookmarks"],
      "tags" => source_counts["public_topic_tags"],
      "media" => source_counts["upload_records"],
      _ => 0,
    };
    if required_count > 0
      && !phase_completed(pool, run.id, phase)
        .await
        .map_err(convert_lemmy_error)?
    {
      blocking_reasons.push(format!(
        "required phase '{phase}' for {label} is not complete"
      ));
    }
  }
  for (kind, count) in &invalid_target_reference_counts {
    if *count > 0 {
      blocking_reasons.push(format!(
        "{count} {kind} mappings point to missing or wrong-site target records"
      ));
    }
  }
  let missing_community_mappings =
    source_counts["community_categories"] - mapped_counts["category_community"];
  if missing_community_mappings > 0 {
    blocking_reasons.push(format!(
      "{missing_community_mappings} source categories have no imported community mapping"
    ));
  }
  let expected_parent_mappings = source_counts["parent_categories"];
  let actual_parent_mappings = mapped_counts["category_multi_community"];
  if expected_parent_mappings != actual_parent_mappings {
    blocking_reasons.push(format!(
      "parent category count is {expected_parent_mappings}, but multi-community mappings contain {actual_parent_mappings}"
    ));
  }
  if (source_counts["upload_records"] > 0 || archived_upload_file_count > 0)
    && !phase_completed(pool, run.id, "media")
      .await
      .map_err(convert_lemmy_error)?
  {
    blocking_reasons.push(format!(
      "media phase is not complete: source has {} upload rows and the archive has {archived_upload_file_count} files",
      source_counts["upload_records"]
    ));
  }
  blocking_reasons.sort();
  blocking_reasons.dedup();
  let is_valid = blocking_reasons.is_empty();
  let report = ImportValidationReport {
    target_site_id: site_id.0,
    source_fingerprint_sha256: source_fingerprint.to_string(),
    source_counts,
    mapped_counts,
    archived_upload_file_count,
    invalid_target_reference_counts,
    blocking_reasons,
    import_run_status: if is_valid { "completed" } else { "failed" },
  };
  update_phase(
    pool,
    run.id,
    "validation",
    None,
    if is_valid { "completed" } else { "failed" },
    0,
  )
  .await
  .map_err(convert_lemmy_error)?;
  store_run_report(pool, run.id, &serde_json::json!({"validation": report}))
    .await
    .map_err(convert_lemmy_error)?;
  set_run_status(pool, run.id, if is_valid { "completed" } else { "failed" })
    .await
    .map_err(convert_lemmy_error)?;
  Ok(report)
}

#[derive(QueryableByName)]
struct SourceUpload {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Integer)]
  user_id: i32,
  #[diesel(sql_type = Text)]
  original_filename: String,
  #[diesel(sql_type = BigInt)]
  filesize: i64,
  #[diesel(sql_type = Text)]
  url: String,
  #[diesel(sql_type = Nullable<Text>)]
  sha1: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  extension: Option<String>,
  #[diesel(sql_type = Bool)]
  secure: bool,
  #[diesel(sql_type = Nullable<BigInt>)]
  access_control_post_id: Option<i64>,
}

#[derive(QueryableByName)]
struct SourceUploadVariant {
  #[diesel(sql_type = Text)]
  url: String,
}

#[derive(QueryableByName)]
struct SourceAvatar {
  #[diesel(sql_type = Integer)]
  user_id: i32,
  #[diesel(sql_type = Integer)]
  uploaded_avatar_id: i32,
}

#[derive(QueryableByName)]
struct SourceCategoryImages {
  #[diesel(sql_type = Integer)]
  category_id: i32,
  #[diesel(sql_type = Nullable<Integer>)]
  uploaded_logo_id: Option<i32>,
  #[diesel(sql_type = Nullable<Integer>)]
  uploaded_background_id: Option<i32>,
}

#[derive(serde::Deserialize)]
struct PictrsUploadResponse {
  msg: String,
  #[serde(default)]
  files: Vec<PictrsUploadFile>,
}

#[derive(serde::Deserialize)]
struct PictrsUploadFile {
  file: String,
  details: PictrsUploadDetails,
}

#[derive(serde::Deserialize)]
struct PictrsUploadDetails {
  width: u16,
  height: u16,
  content_type: String,
  blurhash: Option<String>,
}

struct ImportedMediaItem {
  media_id: Option<i64>,
  target_url: String,
  replacements: Vec<String>,
}

#[derive(QueryableByName)]
struct TargetFileReference {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Text)]
  public_id: String,
  #[diesel(sql_type = Text)]
  content_hash: String,
}

#[derive(QueryableByName)]
struct TargetMediaObject {
  #[diesel(sql_type = BigInt)]
  id: i64,
  #[diesel(sql_type = Text)]
  canonical_pictrs_alias: String,
}

#[derive(QueryableByName)]
struct SourceMediaPost {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  post_number: i32,
}

#[derive(QueryableByName)]
struct SourcePrivatePostRaw {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Text)]
  raw: String,
}

#[derive(QueryableByName)]
struct TargetMediaContent {
  #[diesel(sql_type = Nullable<Text>)]
  content: Option<String>,
}

async fn rewrite_imported_media_references(
  source: &mut AsyncPgConnection,
  pool: &mut DbPool<'_>,
  run_id: i64,
  site_id: LocalSiteId,
  media_items: &[ImportedMediaItem],
  all_source_tokens: &[String],
  report: &mut MediaImportReport,
) -> Result<()> {
  let source_posts = diesel::sql_query(
    "SELECT p.id, p.topic_id, p.post_number FROM posts p JOIN topics t ON t.id = p.topic_id \
     WHERE t.archetype <> 'private_message' AND p.post_type = 1 ORDER BY p.topic_id, p.post_number",
  )
  .load::<SourceMediaPost>(source)
  .await
  .context("failed to read source post IDs for media URL rewriting")?;
  for source_post in source_posts {
    let Some(target_id) = read_mapping(
      pool,
      run_id,
      "source_discussion_post",
      &source_post.id.to_string(),
    )
    .await
    .map_err(convert_lemmy_error)?
    else {
      continue;
    };
    let root_post_id = read_mapping(pool, run_id, "topic", &source_post.topic_id.to_string())
      .await
      .map_err(convert_lemmy_error)?
      .context("imported reply has no mapped topic starter")?;
    let is_topic = source_post.post_number == 1;
    let read_sql = if is_topic {
      "SELECT body AS content FROM post WHERE id = $1"
    } else {
      "SELECT content FROM comment WHERE id = $1"
    };
    let mut content = diesel::sql_query(read_sql)
      .bind::<Integer, _>(target_id as i32)
      .get_result::<TargetMediaContent>(&mut get_conn(pool).await?)
      .await
      .context("failed to read imported content before media URL rewriting")?
      .content;
    let Some(mut content_value) = content.take() else {
      continue;
    };
    let mut changed = false;
    for item in media_items {
      let matched = item
        .replacements
        .iter()
        .any(|replacement| !replacement.is_empty() && content_value.contains(replacement));
      if !matched {
        continue;
      }
      for replacement in &item.replacements {
        if !replacement.is_empty() && content_value.contains(replacement) {
          content_value = content_value.replace(replacement, &item.target_url);
          changed = true;
        }
      }
      if let Some(media_id) = item.media_id {
        MediaReference::ensure(
          pool,
          &MediaReferenceInsertForm {
            local_site_id: site_id,
            media_id,
            owner_person_id: None,
            post_id: Some(PostId(root_post_id as i32)),
            purpose: "post".to_string(),
          },
        )
        .await
        .map_err(convert_lemmy_error)?;
        report.rewritten_media_reference_count += 1;
      }
    }
    if all_source_tokens
      .iter()
      .any(|token| !token.is_empty() && content_value.contains(token))
    {
      report
        .unresolved_content_source_post_ids
        .push(source_post.id);
    }
    if changed {
      let update_sql = if is_topic {
        "UPDATE post SET body = $1 WHERE id = $2"
      } else {
        "UPDATE comment SET content = $1 WHERE id = $2"
      };
      diesel::sql_query(update_sql)
        .bind::<Text, _>(&content_value)
        .bind::<Integer, _>(target_id as i32)
        .execute(&mut get_conn(pool).await?)
        .await
        .context("failed to update imported content with local media URLs")?;
      report.rewritten_source_post_count += 1;
    }
  }
  let private_posts = diesel::sql_query(
    "SELECT p.id, COALESCE(p.raw, '') AS raw FROM posts p JOIN topics t ON t.id = p.topic_id \
     WHERE t.archetype = 'private_message' AND p.post_type = 1",
  )
  .load::<SourcePrivatePostRaw>(source)
  .await
  .context("failed to scan private messages for media references")?;
  for private_post in private_posts {
    if all_source_tokens
      .iter()
      .any(|token| !token.is_empty() && private_post.raw.contains(token))
    {
      report
        .unresolved_private_message_source_post_ids
        .push(private_post.id);
    }
  }
  Ok(())
}

fn upload_source_reference_tokens(upload: &SourceUpload) -> Vec<String> {
  let mut tokens = vec![upload.url.clone()];
  if let Some(path) = upload_url_path(&upload.url) {
    tokens.push(path);
  }
  if let Some(sha1) = upload_source_sha1(upload)
    && let Ok(base62) = base62_sha1(&sha1)
  {
    let extension = upload
      .extension
      .as_deref()
      .or_else(|| {
        Path::new(&upload.original_filename)
          .extension()
          .and_then(|e| e.to_str())
      })
      .unwrap_or_default()
      .trim_start_matches('.');
    let suffix = if extension.is_empty() {
      String::new()
    } else {
      format!(".{extension}")
    };
    tokens.extend([
      format!("upload://{base62}{suffix}"),
      format!("upload://{base62}"),
      format!("/uploads/short-url/{base62}{suffix}"),
      format!("/uploads/short-url/{base62}"),
    ]);
  }
  tokens.sort_by(|left, right| right.len().cmp(&left.len()).then(left.cmp(right)));
  tokens.dedup();
  tokens
}

fn upload_source_sha1(upload: &SourceUpload) -> Option<String> {
  let valid_sha1 = |value: &str| {
    (value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
      .then(|| value.to_ascii_lowercase())
  };
  upload.sha1.as_deref().and_then(valid_sha1).or_else(|| {
    let path = upload_url_path(&upload.url)?;
    let file_stem = Path::new(&path).file_stem()?.to_str()?;
    valid_sha1(file_stem)
  })
}

fn safe_import_filename(filename: &str) -> String {
  let filename = filename.replace(['/', '\\'], "_");
  let filename = Path::new(&filename)
    .file_name()
    .and_then(|value| value.to_str())
    .unwrap_or("attachment");
  let safe = filename
    .chars()
    .filter(|character| !character.is_control())
    .collect::<String>();
  if safe.trim().is_empty() {
    "attachment".to_string()
  } else {
    safe
  }
}

fn build_s3_client(config: &lemmy_utils::settings::structs::ResolvedS3Config) -> S3Client {
  let sdk_config = aws_sdk_s3::Config::builder()
    .behavior_version_latest()
    .endpoint_url(config.endpoint.clone())
    .region(Region::new(config.region.clone()))
    .credentials_provider(Credentials::new(
      config.access_key.clone(),
      config.secret_key.clone(),
      None,
      None,
      "lemmy-discourse-import",
    ))
    .force_path_style(config.use_path_style)
    .build();
  S3Client::from_conf(sdk_config)
}

async fn ensure_generic_file_object(
  pool: &mut DbPool<'_>,
  storage: &S3Client,
  bucket: &str,
  bytes: Vec<u8>,
  size_bytes: i64,
  content_type: &str,
) -> Result<FileObject> {
  let content_hash = hex::encode(<Sha256 as Sha2Digest>::digest(&bytes));
  let storage_key = format!("files/{content_hash}");
  let object_registered = FileObject::read_by_hash(pool, &content_hash)
    .await
    .map_err(convert_lemmy_error)?
    .is_some();
  let object_present = if object_registered {
    match storage
      .head_object()
      .bucket(bucket)
      .key(&storage_key)
      .send()
      .await
    {
      Ok(_) => true,
      Err(error)
        if error
          .raw_response()
          .is_some_and(|response| response.status().as_u16() == 404) =>
      {
        false
      }
      Err(error) => return Err(anyhow!("could not check generic file in S3: {error}")),
    }
  } else {
    false
  };
  if !object_present {
    storage
      .put_object()
      .bucket(bucket)
      .key(&storage_key)
      .content_type(content_type)
      .body(ByteStream::from(bytes))
      .send()
      .await
      .context("failed to store imported generic attachment")?;
  }
  let (object, _) = FileObject::create_or_read(
    pool,
    &FileObjectInsertForm {
      content_hash,
      storage_key,
      size_bytes,
      content_type: content_type.to_string(),
    },
  )
  .await
  .map_err(convert_lemmy_error)?;
  Ok(object)
}

fn find_archive_upload_member(
  url: &str,
  source_id: i32,
  sha1: Option<&str>,
  extension: Option<&str>,
  original_filename: &str,
  members: &[String],
) -> Option<String> {
  if let Some(path) = upload_url_path(url) {
    let exact = path.trim_start_matches('/');
    if members.iter().any(|member| member == exact) {
      return Some(exact.to_string());
    }
    if let Some((_, suffix)) = path.split_once("/original/") {
      if let Some(member) = unique_upload_member(members, &format!("original/{suffix}")) {
        return Some(member);
      }
    }
  }

  // Short-URL records do not reveal the physical `/original/...` path. Discourse's
  // BaseStore derives that path from the upload ID, SHA-1, and extension.
  let sha1 = sha1?.to_ascii_lowercase();
  if sha1.len() != 40 || !sha1.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return None;
  }
  let extension = extension
    .filter(|value| !value.is_empty())
    .map(str::to_string)
    .or_else(|| {
      Path::new(original_filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_string)
    })
    .unwrap_or_default()
    .trim_start_matches('.')
    .to_string();
  let depth = discourse_upload_depth(source_id);
  let tree = sha1
    .chars()
    .take(depth)
    .map(|character| format!("{character}/"))
    .collect::<String>();
  let extension = if extension.is_empty() {
    String::new()
  } else {
    format!(".{extension}")
  };
  let suffix = format!("original/{}X/{tree}{sha1}{extension}", depth + 1);
  unique_upload_member(members, &suffix)
}

/// Match Discourse `BaseStore#get_depth_for`: ceil(log_16(id / 1000.0)), clamped at zero.
/// Integer thresholds avoid rounding differences around powers of 16.
fn discourse_upload_depth(source_id: i32) -> usize {
  if source_id <= 1_000 {
    return 0;
  }
  let mut depth = 0;
  let mut boundary = 1_000_i64;
  while i64::from(source_id) > boundary {
    depth += 1;
    boundary *= 16;
  }
  depth
}

fn unique_upload_member(members: &[String], suffix: &str) -> Option<String> {
  let mut matches = members
    .iter()
    .filter(|member| member.starts_with("uploads/") && member.ends_with(suffix));
  let first = matches.next()?.clone();
  matches.next().is_none().then_some(first)
}

fn upload_url_path(url: &str) -> Option<String> {
  if let Ok(parsed) = Url::parse(url) {
    return Some(parsed.path().to_string());
  }
  if url.starts_with("//") {
    return Url::parse(&format!("https:{url}"))
      .ok()
      .map(|parsed| parsed.path().to_string());
  }
  let path = url.split(['?', '#']).next()?;
  path.starts_with('/').then(|| path.to_string())
}

fn base62_sha1(sha1_hex: &str) -> Result<String> {
  let mut number = hex::decode(sha1_hex).context("invalid source SHA-1 digest")?;
  if number.len() != 20 {
    bail!("source SHA-1 digest must be 20 bytes");
  }
  const ALPHABET: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
  let mut encoded = Vec::new();
  while number.iter().any(|byte| *byte != 0) {
    let mut remainder = 0_u16;
    for byte in &mut number {
      let value = (remainder << 8) | u16::from(*byte);
      *byte = (value / 62) as u8;
      remainder = value % 62;
    }
    encoded.push(ALPHABET[usize::from(remainder)]);
  }
  if encoded.is_empty() {
    encoded.push(b'0');
  }
  encoded.reverse();
  String::from_utf8(encoded).context("base62 encoding produced invalid UTF-8")
}

async fn delete_pictrs_alias(
  client: &reqwest::Client,
  pictrs_url: &Url,
  api_key: Option<&str>,
  alias: &str,
) {
  let Ok(mut delete_url) = pictrs_url.join("internal/delete") else {
    return;
  };
  delete_url.query_pairs_mut().append_pair("alias", alias);
  let _ = client
    .post(delete_url)
    .header("X-Api-Token", api_key.unwrap_or_default())
    .send()
    .await;
}

#[derive(QueryableByName)]
struct SourceLike {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Integer)]
  source_post_id: i32,
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  post_number: i32,
  #[diesel(sql_type = Integer)]
  user_id: i32,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
}

#[derive(QueryableByName)]
struct SourceBookmark {
  #[diesel(sql_type = BigInt)]
  id: i64,
  #[diesel(sql_type = Integer)]
  user_id: i32,
  #[diesel(sql_type = Nullable<BigInt>)]
  bookmarkable_id: Option<i64>,
  #[diesel(sql_type = Nullable<Text>)]
  bookmarkable_type: Option<String>,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
}

#[derive(QueryableByName)]
struct SourcePostLocation {
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  post_number: i32,
}

#[derive(QueryableByName)]
struct SourceTopicArchetype {
  #[diesel(sql_type = Text)]
  archetype: String,
}

#[derive(QueryableByName)]
struct SourceTopicTag {
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Nullable<Integer>)]
  category_id: Option<i32>,
  #[diesel(sql_type = Integer)]
  tag_id: i32,
  #[diesel(sql_type = Text)]
  name: String,
}

#[derive(QueryableByName)]
struct CountResult {
  #[diesel(sql_type = BigInt)]
  count: i64,
}

#[derive(QueryableByName)]
struct StoredFileKey {
  #[diesel(sql_type = Text)]
  storage_key: String,
}

#[derive(QueryableByName)]
struct TargetSourceMapping {
  #[diesel(sql_type = Text)]
  source_id: String,
  #[diesel(sql_type = BigInt)]
  target_id: i64,
}

#[derive(QueryableByName)]
struct SourceLegacyPost {
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  source_post_id: i32,
  #[diesel(sql_type = Integer)]
  post_number: i32,
}

#[derive(QueryableByName)]
struct SourcePrivateMessagePost {
  #[diesel(sql_type = Integer)]
  id: i32,
  #[diesel(sql_type = Integer)]
  topic_id: i32,
  #[diesel(sql_type = Integer)]
  user_id: i32,
  #[diesel(sql_type = Text)]
  raw: String,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  updated_at: Option<DateTime<Utc>>,
  #[diesel(sql_type = Bool)]
  deleted: bool,
}

#[derive(QueryableByName)]
struct SourceParticipant {
  #[diesel(sql_type = Integer)]
  user_id: i32,
}

#[derive(QueryableByName)]
struct SourceChatTables {
  #[diesel(sql_type = Bool)]
  has_messages: bool,
  #[diesel(sql_type = Bool)]
  has_channels: bool,
  #[diesel(sql_type = Bool)]
  has_dm_channels: bool,
  #[diesel(sql_type = Bool)]
  has_dm_users: bool,
}

#[derive(QueryableByName)]
struct SourceChatDirectMessage {
  #[diesel(sql_type = BigInt)]
  id: i64,
  #[diesel(sql_type = BigInt)]
  direct_message_channel_id: i64,
  #[diesel(sql_type = Bool)]
  is_group: bool,
  #[diesel(sql_type = Nullable<Integer>)]
  user_id: Option<i32>,
  #[diesel(sql_type = Text)]
  raw: String,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  updated_at: Option<DateTime<Utc>>,
  #[diesel(sql_type = Bool)]
  deleted: bool,
  #[diesel(sql_type = Bool)]
  has_attachments: bool,
  #[diesel(sql_type = Bool)]
  has_structured_blocks: bool,
}

#[derive(QueryableByName)]
struct SourceParent {
  #[diesel(sql_type = Integer)]
  id: i32,
}

#[derive(QueryableByName)]
struct SourcePoll {
  #[diesel(sql_type = BigInt)]
  id: i64,
  #[diesel(sql_type = Text)]
  name: String,
  #[diesel(sql_type = Nullable<Text>)]
  title: Option<String>,
  #[diesel(sql_type = Nullable<diesel::sql_types::Timestamptz>)]
  close_at: Option<DateTime<Utc>>,
  #[diesel(sql_type = Integer)]
  status: i32,
  #[diesel(sql_type = Integer)]
  results: i32,
}

#[derive(QueryableByName)]
struct SourcePollOption {
  #[diesel(sql_type = BigInt)]
  id: i64,
  #[diesel(sql_type = Text)]
  html: String,
}

#[derive(QueryableByName)]
struct SourcePollVote {
  #[diesel(sql_type = Nullable<BigInt>)]
  user_id: Option<i64>,
  #[diesel(sql_type = Integer)]
  rank: i32,
  #[diesel(sql_type = diesel::sql_types::Timestamptz)]
  created_at: DateTime<Utc>,
}

async fn read_historical_polls(
  source: &mut AsyncPgConnection,
  source_post_id: i32,
) -> Result<Vec<HistoricalPollReport>> {
  let polls = diesel::sql_query(
    "SELECT id, name, title, status, results, close_at AT TIME ZONE 'UTC' AS close_at \
     FROM polls WHERE post_id = $1 ORDER BY id",
  )
  .bind::<Integer, _>(source_post_id)
  .load::<SourcePoll>(source)
  .await
  .context("failed to read Discourse polls")?;
  let mut reports = Vec::with_capacity(polls.len());
  for poll in polls {
    let options =
      diesel::sql_query("SELECT id, html FROM poll_options WHERE poll_id = $1 ORDER BY id")
        .bind::<BigInt, _>(poll.id)
        .load::<SourcePollOption>(source)
        .await
        .context("failed to read Discourse poll options")?;
    let mut option_reports = Vec::with_capacity(options.len());
    for option in options {
      let votes = diesel::sql_query(
        "SELECT user_id, rank, created_at AT TIME ZONE 'UTC' AS created_at \
         FROM poll_votes WHERE poll_id = $1 AND poll_option_id = $2 \
         ORDER BY created_at, user_id NULLS LAST",
      )
      .bind::<BigInt, _>(poll.id)
      .bind::<BigInt, _>(option.id)
      .load::<SourcePollVote>(source)
      .await
      .context("failed to read Discourse poll votes")?;
      option_reports.push(HistoricalPollOption {
        source_option_id: option.id,
        label: html_text(&option.html),
        votes: votes
          .into_iter()
          .map(|vote| HistoricalPollVote {
            source_user_id: vote.user_id,
            rank: vote.rank,
            created_at: vote.created_at,
          })
          .collect(),
      });
    }
    let is_closed =
      poll.status == 1 || poll.close_at.is_some_and(|close_at| close_at <= Utc::now());
    reports.push(HistoricalPollReport {
      source_poll_id: poll.id,
      source_post_id: i64::from(source_post_id),
      prompt: poll
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| poll.name),
      status: poll.status,
      results_visibility: poll.results,
      is_closed,
      // Discourse `on_vote` results are only visible to voters and the poll
      // author while open, so they cannot be copied into public content. Both
      // `on_vote` and `on_close` become public after close_at/status closes the
      // poll. `staff_only` results remain private even after closing.
      show_public_results: poll.results == 0 || (matches!(poll.results, 1 | 2) && is_closed),
      options: option_reports,
    });
  }
  Ok(reports)
}

fn append_historical_polls(raw: &str, polls: &[HistoricalPollReport]) -> String {
  if polls.is_empty() {
    return raw.to_string();
  }
  let mut content = raw.to_string();
  content
    .push_str("\n\n---\n\n**Historical poll results (frozen at migration; not interactive):**\n");
  for poll in polls {
    content.push_str(&format!("\n{}\n", escape_markdown(&poll.prompt)));
    if !poll.show_public_results {
      content.push_str("*Results were hidden by the source poll settings at migration time.*\n");
      continue;
    }
    for option in &poll.options {
      content.push_str(&format!(
        "- {} — {} vote(s)\n",
        escape_markdown(&option.label),
        option.votes.len()
      ));
    }
  }
  content
}

fn html_text(input: &str) -> String {
  let mut text = String::with_capacity(input.len());
  let mut in_tag = false;
  for character in input.chars() {
    match character {
      '<' => in_tag = true,
      '>' => in_tag = false,
      _ if !in_tag => text.push(character),
      _ => {}
    }
  }
  text
    .replace("&amp;", "&")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
    .trim()
    .to_string()
}

fn escape_markdown(input: &str) -> String {
  input
    .replace('\\', "\\\\")
    .replace('*', "\\*")
    .replace('_', "\\_")
    .replace('[', "\\[")
    .replace(']', "\\]")
    .replace('(', "\\(")
    .replace(')', "\\)")
    .replace('`', "\\`")
}

fn merge_profile_fields(
  bio: Option<String>,
  location: Option<&str>,
  website: Option<&str>,
) -> Option<String> {
  let mut content = bio.unwrap_or_default();
  let mut details = Vec::new();
  if let Some(location) = location.filter(|value| !value.trim().is_empty()) {
    details.push(format!("**Location:** {}", escape_profile_text(location)));
  }
  if let Some(website) = website.filter(|value| !value.trim().is_empty()) {
    details.push(format!("**Website:** {}", escape_profile_text(website)));
  }
  if !details.is_empty() {
    if !content.trim().is_empty() {
      content.push_str("\n\n---\n\n");
    }
    content.push_str(&details.join("\n\n"));
  }
  (!content.trim().is_empty()).then_some(content)
}

fn escape_profile_text(input: &str) -> String {
  let one_line = input.split_whitespace().collect::<Vec<_>>().join(" ");
  let html_escaped = one_line
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;");
  escape_markdown(&html_escaped)
}

fn imported_object_ap_id(
  root: &str,
  fingerprint: &str,
  kind: &str,
  source_id: i64,
) -> Result<DbUrl> {
  Ok(
    Url::parse(&format!(
      "{root}/discourse-import/{fingerprint}/{kind}/{source_id}"
    ))?
    .into(),
  )
}

async fn load_multi_community_creator(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
) -> Result<Creator> {
  let conn = &mut lemmy_diesel_utils::connection::get_conn(pool).await?;
  diesel::sql_query(
    "SELECT person_id FROM local_user WHERE local_site_id = $1 ORDER BY admin DESC, id ASC LIMIT 1",
  )
  .bind::<Integer, _>(site_id.0)
  .get_result(conn)
  .await
  .context("import accounts before categories so a local creator exists for parent categories")
}

async fn multi_entry_exists(
  pool: &mut DbPool<'_>,
  multi_id: MultiCommunityId,
  community_id: CommunityId,
) -> Result<bool> {
  #[derive(QueryableByName)]
  struct Exists {
    #[diesel(sql_type = Bool)]
    found: bool,
  }
  let conn = &mut lemmy_diesel_utils::connection::get_conn(pool).await?;
  let row = diesel::sql_query(
    "SELECT EXISTS (SELECT 1 FROM multi_community_entry WHERE multi_community_id = $1 AND community_id = $2) AS found",
  )
  .bind::<Integer, _>(multi_id.0)
  .bind::<Integer, _>(community_id.0)
  .get_result::<Exists>(conn)
  .await?;
  Ok(row.found)
}

async fn resolve_community_name(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  source_name: &str,
  source_id: i32,
  reserved: &HashSet<String>,
) -> Result<String> {
  let base = normalize_username(source_name, source_id);
  let existing = Community::read_from_name(pool, &base, None, Some(site_id), false)
    .await
    .map_err(convert_lemmy_error)?
    .is_some();
  if !existing && !reserved.contains(&base) {
    return Ok(base);
  }
  let suffix = format!("_c{source_id}");
  let prefix_len = 20_usize.saturating_sub(suffix.len());
  let candidate = format!(
    "{}{suffix}",
    base.chars().take(prefix_len).collect::<String>()
  );
  if Community::read_from_name(pool, &candidate, None, Some(site_id), false)
    .await
    .map_err(convert_lemmy_error)?
    .is_some()
    || reserved.contains(&candidate)
  {
    bail!("deterministic community alias {candidate} is already taken");
  }
  Ok(candidate)
}

async fn resolve_multi_community_name(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  source_name: &str,
  source_id: i32,
  reserved: &HashSet<String>,
) -> Result<String> {
  let base = normalize_username(source_name, source_id);
  if MultiCommunity::read_from_name(pool, &base, None, Some(site_id), false)
    .await
    .map_err(convert_lemmy_error)?
    .is_none()
    && !reserved.contains(&base)
  {
    return Ok(base);
  }
  let suffix = format!("_c{source_id}");
  let prefix_len = 20_usize.saturating_sub(suffix.len());
  let candidate = format!(
    "{}{suffix}",
    base.chars().take(prefix_len).collect::<String>()
  );
  if MultiCommunity::read_from_name(pool, &candidate, None, Some(site_id), false)
    .await
    .map_err(convert_lemmy_error)?
    .is_some()
    || reserved.contains(&candidate)
  {
    bail!("deterministic multi-community alias {candidate} is already taken");
  }
  Ok(candidate)
}

fn descendant_category_ids(root: i32, categories: &[SourceCategory]) -> Vec<i32> {
  let mut descendants = Vec::new();
  let mut frontier = vec![root];
  while let Some(parent) = frontier.pop() {
    for category in categories
      .iter()
      .filter(|category| category.parent_category_id == Some(parent))
    {
      descendants.push(category.id);
      frontier.push(category.id);
    }
  }
  descendants
}

async fn load_target_site(pool: &mut DbPool<'_>, site_id: LocalSiteId) -> Result<TargetSite> {
  let conn = &mut lemmy_diesel_utils::connection::get_conn(pool).await?;
  diesel::sql_query(
    "SELECT s.id AS site_id, s.instance_id, ls.hostname, ls.federation_enabled, \
     ls.file_upload_disabled, ls.file_max_upload_size_bytes, ls.file_allowed_mime_types \
     FROM local_site ls \
     JOIN site s ON s.id = ls.site_id WHERE ls.id = $1",
  )
  .bind::<Integer, _>(site_id.0)
  .get_result(conn)
  .await
  .context("target LocalSite does not exist")
}

async fn email_taken(pool: &mut DbPool<'_>, site_id: LocalSiteId, email: &str) -> Result<bool> {
  #[derive(QueryableByName)]
  struct Exists {
    #[diesel(sql_type = Bool)]
    taken: bool,
  }
  let conn = &mut lemmy_diesel_utils::connection::get_conn(pool).await?;
  let found = diesel::sql_query(
    "SELECT EXISTS (SELECT 1 FROM local_user WHERE local_site_id = $1 AND lower(email) = lower($2)) AS taken",
  )
  .bind::<Integer, _>(site_id.0)
  .bind::<Text, _>(email)
  .get_result::<Exists>(conn)
  .await?;
  Ok(found.taken)
}

async fn resolve_username(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  normalized: &str,
  source_id: i32,
  policy: AccountConflictPolicy,
  reserved: &mut HashSet<String>,
) -> Result<Option<String>> {
  let mut candidate = normalized.to_string();
  if !username_taken(pool, site_id, &candidate, reserved).await? {
    return Ok(Some(candidate));
  }
  match policy {
    AccountConflictPolicy::Report | AccountConflictPolicy::Skip => Ok(None),
    AccountConflictPolicy::Rename => {
      candidate = renamed_username(normalized, source_id)?;
      if username_taken(pool, site_id, &candidate, reserved).await? {
        bail!("deterministic username alias {candidate} is already taken");
      }
      Ok(Some(candidate))
    }
  }
}

fn renamed_username(normalized: &str, source_id: i32) -> Result<String> {
  let suffix = format!("_d{source_id}");
  let prefix_len = 20_usize.saturating_sub(suffix.len());
  let mut candidate = normalized.chars().take(prefix_len).collect::<String>();
  candidate.push_str(&suffix);
  is_valid_local_actor_name(&candidate, false).map_err(convert_lemmy_error)?;
  Ok(candidate)
}

async fn username_taken(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  candidate: &str,
  reserved: &HashSet<String>,
) -> Result<bool> {
  Ok(
    reserved.contains(candidate)
      || Person::read_from_name(pool, candidate, None, Some(site_id), false)
        .await
        .map_err(convert_lemmy_error)?
        .is_some(),
  )
}

async fn read_mapped_person_name(pool: &mut DbPool<'_>, target_id: i64) -> Result<String> {
  diesel::sql_query("SELECT name FROM person WHERE id = $1")
    .bind::<Integer, _>(target_id as i32)
    .get_result::<MappedPersonName>(&mut get_conn(pool).await?)
    .await
    .map(|person| person.name)
    .context("an existing import mapping points to a missing target person")
}

fn convert_lemmy_error(error: lemmy_utils::error::LemmyError) -> anyhow::Error {
  anyhow!(error.to_string())
}

fn normalize_username(source: &str, source_id: i32) -> String {
  let mut result = String::new();
  let mut previous_separator = false;
  for character in source.chars() {
    let supported_script = ('\u{3400}'..='\u{9fff}').contains(&character)
      || ('\u{3040}'..='\u{30ff}').contains(&character)
      || ('\u{ac00}'..='\u{d7af}').contains(&character);
    if character.is_ascii_alphanumeric() || character == '_' || supported_script {
      result.push(if character.is_ascii() {
        character.to_ascii_lowercase()
      } else {
        character
      });
      previous_separator = false;
    } else if !previous_separator && !result.is_empty() {
      result.push('_');
      previous_separator = true;
    }
  }
  let result = result
    .trim_matches('_')
    .chars()
    .take(20)
    .collect::<String>();
  if result.chars().count() >= 2 {
    result
  } else {
    format!("user_{source_id}")
  }
}

fn normalize_tag_name(source: &str) -> String {
  let mut result = String::new();
  let mut separator = false;
  for character in source.chars() {
    if character.is_ascii_alphanumeric() || character == '_' {
      result.push(character.to_ascii_lowercase());
      separator = false;
    } else if !separator && !result.is_empty() {
      result.push('_');
      separator = true;
    }
  }
  result.trim_matches('_').chars().take(200).collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
  value.chars().take(max_chars).collect()
}

fn unique_tag_name(source: &str, source_tag_id: i32, reserved: &HashSet<String>) -> String {
  let mut base = normalize_tag_name(source);
  if base.is_empty() {
    base = format!("tag{source_tag_id}");
  }
  if !reserved.contains(&base) {
    return base;
  }
  let mut suffix = 0_u32;
  loop {
    let extra = if suffix == 0 {
      format!("_d{source_tag_id}")
    } else {
      format!("_d{source_tag_id}_{suffix}")
    };
    let prefix = base
      .chars()
      .take(255_usize.saturating_sub(extra.len()))
      .collect::<String>();
    let candidate = format!("{prefix}{extra}");
    if !reserved.contains(&candidate) {
      return candidate;
    }
    suffix += 1;
  }
}
