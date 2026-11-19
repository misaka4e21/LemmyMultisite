use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
  collections::{BTreeMap, BTreeSet},
  fs::File,
  io::{BufRead, BufReader, Read, Write},
  path::{Component, Path, PathBuf},
  process::{Command, Stdio},
};

mod accounts;

const DEFAULT_MAX_EXPANDED_BYTES: u64 = 137_438_953_472;
const MAX_ARCHIVE_MEMBERS: usize = 250_000;
const MAX_MEMBER_NAME_BYTES: usize = 4_096;
const MAX_MEMBER_LIST_BYTES: u64 = 67_108_864;
const IMPORTED_TABLES: &[&str] = &[
  "bookmarks",
  "categories",
  "poll_options",
  "poll_votes",
  "polls",
  "post_actions",
  "posts",
  "tags",
  "topic_allowed_users",
  "topic_tags",
  "topics",
  "uploads",
  "user_emails",
  "user_options",
  "user_passwords",
  "user_profiles",
  "users",
];
const PARTIALLY_IMPORTED_TABLES: &[&str] = &[
  "category_groups",
  "chat_channels",
  "chat_messages",
  "direct_message_channels",
  "direct_message_users",
  "optimized_images",
];
const IGNORED_METADATA_TABLES: &[&str] = &["ar_internal_metadata", "schema_migrations"];

const KNOWN_UNSUPPORTED_TABLES: &[&str] = &[
  "access_control_lists",
  "ad_plugin_house_ads",
  "ad_plugin_house_ads_categories",
  "ad_plugin_house_ads_groups",
  "ad_plugin_house_ads_routes",
  "ad_plugin_impressions",
  "admin_dashboard_reports",
  "admin_dashboard_sections",
  "admin_notices",
  "ai_agent_mcp_servers",
  "ai_agents",
  "ai_api_audit_logs",
  "ai_api_request_stats",
  "ai_artifact_key_values",
  "ai_artifact_versions",
  "ai_artifacts",
  "ai_document_fragments_embeddings",
  "ai_mcp_oauth_tokens",
  "ai_mcp_servers",
  "ai_moderation_settings",
  "ai_post_image_captions",
  "ai_posts_embeddings",
  "ai_secrets",
  "ai_spam_logs",
  "ai_summaries",
  "ai_tool_actions",
  "ai_tool_secret_bindings",
  "ai_tools",
  "ai_topics_embeddings",
  "allowed_pm_users",
  "anonymous_users",
  "api_key_scopes",
  "api_keys",
  "application_requests",
  "assignments",
  "associated_groups",
  "backup_draft_posts",
  "backup_draft_topics",
  "backup_metadata",
  "badge_groupings",
  "badge_types",
  "badges",
  "browser_pageview_country_daily_rollups",
  "browser_pageview_event_scores",
  "browser_pageview_events",
  "browser_pageview_referrer_daily_rollups",
  "browser_pageview_session_engagement_daily_rollups",
  "browser_pageview_session_engagements",
  "calendar_events",
  "categories_web_hooks",
  "category_activity_daily_rollups",
  "category_custom_fields",
  "category_featured_topics",
  "category_users",
  "category_form_templates",
  "category_localizations",
  "category_moderation_groups",
  "category_posting_review_groups",
  "category_required_tag_groups",
  "category_search_data",
  "category_settings",
  "category_tag_groups",
  "category_tag_stats",
  "category_tags",
  "chat_channel_archives",
  "chat_channel_custom_fields",
  "chat_drafts",
  "chat_mention_notifications",
  "chat_mentions",
  "chat_message_custom_fields",
  "chat_message_custom_prompts",
  "chat_message_interactions",
  "chat_message_links",
  "chat_message_reactions",
  "chat_message_revisions",
  "chat_message_search_data",
  "chat_pinned_messages",
  "chat_thread_custom_fields",
  "chat_threads",
  "chat_webhook_events",
  "child_themes",
  "classification_results",
  "color_scheme_colors",
  "color_schemes",
  "completion_prompts",
  "custom_emojis",
  "data_explorer_queries",
  "data_explorer_query_groups",
  "data_explorer_query_stats",
  "developers",
  "directory_columns",
  "directory_items",
  "discourse_ai_ai_bot_conversation_stars",
  "discourse_automation_automations",
  "discourse_automation_fields",
  "discourse_automation_pending_automations",
  "discourse_automation_pending_pms",
  "discourse_automation_stats",
  "discourse_automation_user_global_notices",
  "discourse_calendar_disabled_holidays",
  "discourse_calendar_post_event_dates",
  "discourse_post_event_events",
  "discourse_post_event_invitees",
  "discourse_reactions_reaction_users",
  "discourse_reactions_reactions",
  "discourse_rss_polling_poll_attempts",
  "discourse_rss_polling_rss_feeds",
  "discourse_solved_shared_issues",
  "discourse_solved_solved_topics",
  "discourse_solved_topic_answers",
  "discourse_subscriptions_customers",
  "discourse_subscriptions_products",
  "discourse_subscriptions_subscriptions",
  "discourse_templates_usage_count",
  "discourse_workflows_ai_authoring_sessions",
  "discourse_workflows_credentials",
  "discourse_workflows_data_tables",
  "discourse_workflows_execution_data",
  "discourse_workflows_execution_stats",
  "discourse_workflows_executions",
  "discourse_workflows_tags",
  "discourse_workflows_variables",
  "discourse_workflows_webhooks",
  "discourse_workflows_workflow_call_runs",
  "discourse_workflows_workflow_dependencies",
  "discourse_workflows_workflow_publish_history",
  "discourse_workflows_workflow_tags",
  "discourse_workflows_workflow_versions",
  "discourse_workflows_workflows",
  "dismissed_topic_users",
  "do_not_disturb_timings",
  "draft_sequences",
  "drafts",
  "email_change_requests",
  "email_login_codes",
  "email_logs",
  "email_tokens",
  "embeddable_host_tags",
  "embeddable_hosts",
  "embedding_definitions",
  "external_upload_stubs",
  "flags",
  "form_templates",
  "gamification_leaderboard_scores",
  "gamification_leaderboards",
  "gamification_score_events",
  "gamification_scores",
  "github_commits",
  "github_repos",
  "given_daily_likes",
  "group_archived_messages",
  "group_associated_groups",
  "group_category_notification_defaults",
  "group_custom_fields",
  "group_histories",
  "group_mentions",
  "group_requests",
  "group_users",
  "group_tag_notification_defaults",
  "groups",
  "groups_web_hooks",
  "ignored_users",
  "incoming_chat_webhooks",
  "incoming_domains",
  "incoming_emails",
  "incoming_links",
  "incoming_referers",
  "inferred_concept_posts",
  "inferred_concept_topics",
  "inferred_concepts",
  "invited_groups",
  "invited_users",
  "invites",
  "javascript_caches",
  "linked_topics",
  "livestream_topic_chat_channels",
  "llm_credit_allocations",
  "llm_credit_daily_usages",
  "llm_feature_credit_costs",
  "llm_models",
  "llm_quota_usages",
  "llm_quotas",
  "message_bus",
  "model_accuracies",
  "moved_posts",
  "muted_users",
  "nested_hot_post_scores",
  "nested_hot_score_snapshots",
  "nested_topics",
  "nested_view_post_stats",
  "notifications",
  "oauth2_user_infos",
  "onceoff_logs",
  "optimized_videos",
  "permalinks",
  "plugin_store_rows",
  "policy_users",
  "post_action_types",
  "post_custom_fields",
  "post_custom_prompts",
  "post_details",
  "post_hotlinked_media",
  "post_localizations",
  "post_policies",
  "post_policy_groups",
  "post_replies",
  "post_revisions",
  "post_reply_keys",
  "post_search_data",
  "post_stats",
  "post_timings",
  "post_voting_comment_custom_fields",
  "post_voting_comments",
  "post_voting_votes",
  "problem_check_trackers",
  "published_pages",
  "push_subscriptions",
  "quoted_posts",
  "rag_document_fragments",
  "redelivering_webhook_events",
  "remote_themes",
  "reviewable_claimed_topics",
  "reviewable_histories",
  "reviewable_notes",
  "reviewable_scores",
  "reviewables",
  "scheduler_stats",
  "schema_migration_details",
  "screened_emails",
  "screened_ip_addresses",
  "screened_urls",
  "search_logs",
  "shared_ai_conversations",
  "shared_drafts",
  "shelved_notifications",
  "sidebar_section_links",
  "sidebar_section_localizations",
  "sidebar_sections",
  "sidebar_url_localizations",
  "sidebar_urls",
  "silenced_assignments",
  "single_sign_on_records",
  "site_setting_groups",
  "site_setting_localizations",
  "site_settings",
  "sitemaps",
  "skipped_email_logs",
  "stylesheet_cache",
  "summary_sections",
  "tag_group_memberships",
  "tag_group_permissions",
  "tag_groups",
  "tag_localizations",
  "tag_search_data",
  "tag_users",
  "tags_web_hooks",
  "theme_fields",
  "theme_modifier_sets",
  "theme_settings",
  "theme_settings_migrations",
  "theme_site_settings",
  "theme_svg_sprites",
  "theme_translation_overrides",
  "themes",
  "top_topics",
  "topic_allowed_groups",
  "topic_custom_fields",
  "topic_embeds",
  "topic_groups",
  "topic_hot_scores",
  "topic_invites",
  "topic_link_clicks",
  "topic_links",
  "topic_localizations",
  "topic_search_data",
  "topic_thumbnails",
  "topic_timers",
  "topic_users",
  "topic_view_stats",
  "topic_views",
  "topic_voting_category_settings",
  "topic_voting_topic_vote_count",
  "topic_voting_votes",
  "translation_overrides",
  "unsubscribe_keys",
  "upcoming_change_events",
  "upload_references",
  "user_api_key_client_scopes",
  "user_api_key_clients",
  "user_api_key_scopes",
  "user_api_keys",
  "user_archived_messages",
  "user_associated_accounts",
  "user_associated_groups",
  "user_auth_token_logs",
  "user_auth_tokens",
  "user_avatars",
  "user_badges",
  "user_chat_channel_memberships",
  "user_chat_thread_memberships",
  "user_custom_fields",
  "user_exports",
  "user_field_options",
  "user_fields",
  "user_histories",
  "user_ip_address_histories",
  "user_notification_schedules",
  "user_open_ids",
  "user_profile_views",
  "user_required_fields_versions",
  "user_search_data",
  "user_second_factors",
  "user_security_keys",
  "user_stats",
  "user_statuses",
  "user_actions",
  "user_uploads",
  "user_visit_daily_rollups",
  "user_visits",
  "user_warnings",
  "watched_word_groups",
  "watched_words",
  "web_crawler_requests",
  "web_hook_event_types",
  "web_hook_event_types_hooks",
  "web_hook_events",
  "web_hook_events_daily_aggregates",
  "web_hooks",
];

#[derive(Parser)]
#[command(about = "Inspect a Discourse backup before importing it into LemmyMultisite")]
struct Cli {
  #[command(subcommand)]
  command: CommandLine,
}

#[derive(Subcommand)]
enum CommandLine {
  /// Inspect archive members and count rows in the SQL dump. Does not connect to or write the target.
  Preview {
    archive: PathBuf,
    /// Target LocalSite ID for which a later import plan/report is being prepared.
    #[arg(long)]
    site_id: i32,
    /// How to handle username/email collisions with existing local accounts.
    #[arg(long, value_enum, default_value_t = ConflictPolicy::Report)]
    account_conflicts: ConflictPolicy,
    /// JSON file path for the machine-readable preview report. Defaults to stdout.
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Compare source account identities with a target site without creating the import ledger or accounts.
  PreviewAccounts {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long, value_enum, default_value_t = ConflictPolicy::Report)]
    account_conflicts: ConflictPolicy,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Restore a supported dump into an operator-provisioned, isolated, empty PostgreSQL database.
  RestoreSource {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_RESTORE_DATABASE_URL")]
    database_url: String,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Restore an archive and run every supported import phase in one staged extraction.
  ImportAll {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_RESTORE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long, value_enum, default_value_t = ConflictPolicy::Report)]
    account_conflicts: ConflictPolicy,
    /// Optionally write the generated Nginx legacy URL map.
    #[arg(long)]
    legacy_urls_output: Option<PathBuf>,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import local accounts from a restored Discourse database into one selected site.
  ImportAccounts {
    archive: PathBuf,
    /// Source PostgreSQL URL. Prefer DISCOURSE_DATABASE_URL to avoid exposing credentials in process arguments.
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    /// Target LocalSite ID.
    #[arg(long)]
    site_id: i32,
    #[arg(long, value_enum, default_value_t = ConflictPolicy::Report)]
    account_conflicts: ConflictPolicy,
    /// JSON file path for the result report. Defaults to stdout.
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import Discourse categories as Lemmy communities and parent multi-communities.
  ImportCategories {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import public Discourse topics and replies into posts and comments.
  ImportDiscussions {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import Discourse private-message posts for mapped participants.
  ImportPrivateMessages {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Export source Discourse topic/reply paths as an Nginx map include.
  ExportLegacyUrls {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    /// Nginx map entries output file; old hostname is configured separately in the proxy.
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import supported likes and bookmarks for mapped public discussions.
  ImportInteractions {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import Discourse topic tags into the owning Lemmy community and assign them to posts.
  ImportTags {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Import public image uploads from the archive into the configured pict-rs service.
  ImportMedia {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
  /// Validate source-to-target counts, site ownership, federation state, and media coverage.
  Validate {
    archive: PathBuf,
    #[arg(long, env = "DISCOURSE_DATABASE_URL")]
    source_database_url: String,
    #[arg(long)]
    site_id: i32,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = DEFAULT_MAX_EXPANDED_BYTES)]
    max_expanded_bytes: u64,
  },
}

#[derive(Clone, Copy, Debug, PartialEq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConflictPolicy {
  Report,
  Rename,
  Skip,
}

#[derive(Serialize)]
struct Preview {
  archive: String,
  target_site_id: i32,
  account_conflict_policy: ConflictPolicy,
  source_fingerprint_sha256: String,
  discourse_version: Option<String>,
  database_version_from_filename: Option<i64>,
  database_version_from_dump: Option<i64>,
  supported: bool,
  required_extensions: Vec<String>,
  required_schemas: Vec<String>,
  archive_member_count: usize,
  upload_file_count: usize,
  expanded_sql_bytes: u64,
  table_row_counts: BTreeMap<String, u64>,
  restricted_category_ids: Vec<i32>,
  partially_imported_tables: Vec<String>,
  unsupported_tables: Vec<String>,
  unclassified_tables: Vec<String>,
  warnings: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();
  match cli.command {
    CommandLine::Preview {
      archive,
      site_id,
      account_conflicts,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(&archive, site_id, account_conflicts, max_expanded_bytes)?;
      let json = serde_json::to_string_pretty(&preview_report)?;
      if let Some(path) = report {
        std::fs::write(&path, json)
          .with_context(|| format!("failed to write report {}", path.display()))?;
      } else {
        println!("{json}");
      }
      if !preview_report.supported {
        std::process::exit(2);
      }
    }
    CommandLine::PreviewAccounts {
      archive,
      source_database_url,
      site_id,
      account_conflicts,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let archive_report = preview(&archive, site_id, account_conflicts, max_expanded_bytes)?;
      if !archive_report.supported {
        bail!("unsupported Discourse archive; account conflict preview was not run");
      }
      let conflict_report = accounts::preview_account_conflicts(
        &source_database_url,
        site_id,
        &archive_report.source_fingerprint_sha256,
        account_conflicts.into(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&serde_json::json!({
        "archive": archive_report,
        "account_conflicts": conflict_report,
        "target_database_modified": false
      }))?;
      if let Some(path) = report {
        std::fs::write(&path, json)
          .with_context(|| format!("failed to write report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::RestoreSource {
      archive,
      database_url,
      max_expanded_bytes,
    } => {
      let preview_report = preview(&archive, 1, ConflictPolicy::Report, max_expanded_bytes)?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; source database was not modified");
      }
      let expanded_bytes = restore_source_dump(
        &archive,
        &database_url,
        &preview_report.required_extensions,
        max_expanded_bytes,
      )?;
      println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
          "source_fingerprint_sha256": preview_report.source_fingerprint_sha256,
          "database_version": preview_report.database_version_from_dump,
          "required_extensions": preview_report.required_extensions,
          "restored_sql_bytes": expanded_bytes,
          "source_database_modified": true
        }))?
      );
    }
    CommandLine::ImportAll {
      archive,
      source_database_url,
      site_id,
      account_conflicts,
      legacy_urls_output,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let result = run_import_all(
        &archive,
        &source_database_url,
        site_id,
        account_conflicts,
        legacy_urls_output.as_deref(),
        max_expanded_bytes,
      )
      .await?;
      let json = serde_json::to_string_pretty(&result)?;
      if let Some(path) = report {
        std::fs::write(&path, json)
          .with_context(|| format!("failed to write import report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ImportAccounts {
      archive,
      source_database_url,
      site_id,
      account_conflicts,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(&archive, site_id, account_conflicts, max_expanded_bytes)?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let imported = accounts::import_accounts(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
        account_conflicts.into(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&imported)?;
      if let Some(path) = report {
        std::fs::write(&path, json)
          .with_context(|| format!("failed to write import report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ImportCategories {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let imported = accounts::import_categories(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&imported)?;
      if let Some(path) = report {
        std::fs::write(&path, json)
          .with_context(|| format!("failed to write category import report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ImportDiscussions {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let imported = accounts::import_discussions(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&imported)?;
      if let Some(path) = report {
        std::fs::write(&path, json).with_context(|| {
          format!(
            "failed to write discussion import report {}",
            path.display()
          )
        })?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ImportPrivateMessages {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let imported = accounts::import_private_messages(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&imported)?;
      if let Some(path) = report {
        std::fs::write(&path, json).with_context(|| {
          format!(
            "failed to write private-message import report {}",
            path.display()
          )
        })?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ExportLegacyUrls {
      archive,
      source_database_url,
      site_id,
      output,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let (entries, mapping_report) = accounts::export_legacy_urls(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
      )
      .await?;
      let mut map = String::from(
        "map $uri $discourse_legacy_key {\n  default \"\";\n  ~^/t/(?:[^/]+/)?(?<discourse_legacy_root_topic>[0-9]+)$ \"$discourse_legacy_root_topic:1\";\n  ~^/t/(?:[^/]+/)?(?<discourse_legacy_reply_topic>[0-9]+)/(?<discourse_legacy_post_number>[0-9]+)$ \"$discourse_legacy_reply_topic:$discourse_legacy_post_number\";\n  ~^/t/(?:[^/]+/)?(?<discourse_legacy_last_topic>[0-9]+)/last$ \"$discourse_legacy_last_topic:last\";\n}\n\nmap $discourse_legacy_key $discourse_legacy_path {\n  default \"\";\n",
      );
      for (source_path, target_path) in entries {
        map.push_str(&format!("  \"{source_path}\" \"{target_path}\";\n"));
      }
      map.push_str("}\n");
      std::fs::write(&output, map)
        .with_context(|| format!("failed to write legacy URL map {}", output.display()))?;
      let json = serde_json::to_string_pretty(&mapping_report)?;
      if let Some(path) = report {
        std::fs::write(&path, json)
          .with_context(|| format!("failed to write legacy URL report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ImportInteractions {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let imported = accounts::import_interactions(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&imported)?;
      if let Some(path) = report {
        std::fs::write(&path, json).with_context(|| {
          format!(
            "failed to write interaction import report {}",
            path.display()
          )
        })?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::Validate {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; validation was not run");
      }
      let validation = accounts::validate_import(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
        preview_report.upload_file_count,
      )
      .await?;
      let json = serde_json::to_string_pretty(&validation)?;
      if let Some(path) = report {
        std::fs::write(&path, &json)
          .with_context(|| format!("failed to write validation report {}", path.display()))?;
      } else {
        println!("{json}");
      }
      if !validation.blocking_reasons.is_empty() {
        bail!(
          "validation found {} blocking reason(s)",
          validation.blocking_reasons.len()
        );
      }
    }
    CommandLine::ImportTags {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let imported = accounts::import_tags(
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
      )
      .await?;
      let json = serde_json::to_string_pretty(&imported)?;
      if let Some(path) = report {
        std::fs::write(&path, &json)
          .with_context(|| format!("failed to write tag import report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
    CommandLine::ImportMedia {
      archive,
      source_database_url,
      site_id,
      report,
      max_expanded_bytes,
    } => {
      anyhow::ensure!(site_id > 0, "--site-id must be a positive LocalSite ID");
      let preview_report = preview(
        &archive,
        site_id,
        ConflictPolicy::Report,
        max_expanded_bytes,
      )?;
      if !preview_report.supported {
        bail!("unsupported Discourse archive; no target writes were made");
      }
      let media_report = accounts::import_media(
        &archive,
        &source_database_url,
        site_id,
        &preview_report.source_fingerprint_sha256,
        preview_report
          .discourse_version
          .as_deref()
          .unwrap_or_default(),
        preview_report
          .database_version_from_dump
          .unwrap_or_default(),
        preview_report.upload_file_count,
      )
      .await?;
      let json = serde_json::to_string_pretty(&media_report)?;
      if let Some(path) = report {
        std::fs::write(&path, &json)
          .with_context(|| format!("failed to write media import report {}", path.display()))?;
      } else {
        println!("{json}");
      }
    }
  }
  Ok(())
}

impl From<ConflictPolicy> for accounts::AccountConflictPolicy {
  fn from(value: ConflictPolicy) -> Self {
    match value {
      ConflictPolicy::Report => Self::Report,
      ConflictPolicy::Rename => Self::Rename,
      ConflictPolicy::Skip => Self::Skip,
    }
  }
}

async fn run_import_all(
  archive: &Path,
  source_database_url: &str,
  site_id: i32,
  account_conflicts: ConflictPolicy,
  legacy_urls_output: Option<&Path>,
  max_expanded_bytes: u64,
) -> Result<serde_json::Value> {
  let archive_name = archive
    .file_name()
    .and_then(|name| name.to_str())
    .context("archive filename must be valid UTF-8")?;
  eprintln!("[import-all] Extracting the archive once...");
  let staging = extract_archive_once(archive, max_expanded_bytes)?;
  eprintln!("[import-all] Archive extraction complete.");
  eprintln!("[import-all] Checking source database state...");
  let restore_dump = prompt_clear_source_database_if_needed(source_database_url)?;
  eprintln!("[import-all] Reading archive metadata...");
  let (preview_report, restored_bytes) = if restore_dump {
    let preview_report = preview_named(
      &staging.path,
      Some(archive_name),
      site_id,
      account_conflicts,
      max_expanded_bytes,
    )?;
    eprintln!("[import-all] Restoring the Discourse database with psql...");
    let restored_bytes = restore_source_dump(
      &staging.path,
      source_database_url,
      &preview_report.required_extensions,
      max_expanded_bytes,
    )?;
    eprintln!("[import-all] Discourse database restore complete.");
    (preview_report, Some(restored_bytes))
  } else {
    // The existing source database is already restored. Preview archive metadata
    // for the fingerprint and media count, then continue with the Lemmy imports.
    eprintln!("[import-all] Skipping database restore; continuing Lemmy migration.");
    (
      preview_named(
        &staging.path,
        Some(archive_name),
        site_id,
        account_conflicts,
        max_expanded_bytes,
      )?,
      None,
    )
  };

  let discourse_version = preview_report
    .discourse_version
    .as_deref()
    .unwrap_or_default();
  let schema_version = preview_report
    .database_version_from_dump
    .unwrap_or_default();
  let fingerprint = &preview_report.source_fingerprint_sha256;
  eprintln!("[import-all] Migrating accounts...");
  let account_report = accounts::import_accounts(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
    account_conflicts.into(),
  )
  .await?;
  if account_report.skipped_user_count > 0 && account_conflicts != ConflictPolicy::Skip {
    let examples = account_report
      .conflicts
      .iter()
      .take(5)
      .cloned()
      .collect::<Vec<_>>()
      .join("; ");
    bail!(
      "account migration has {} unresolved conflicts; categories and later phases require every source account to be mapped. Re-run import-all with --account-conflicts rename to assign conflict-free names. Examples: {}",
      account_report.skipped_user_count,
      examples
    );
  }
  if account_report.skipped_user_count > 0 {
    eprintln!(
      "[import-all] Skipped {} conflicting accounts by request; dependent content for those accounts may also be skipped.",
      account_report.skipped_user_count
    );
  }
  eprintln!("[import-all] Migrating categories...");
  let category_report = accounts::import_categories(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
  )
  .await?;
  eprintln!("[import-all] Migrating discussions...");
  let discussion_report = accounts::import_discussions(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
  )
  .await?;
  eprintln!("[import-all] Migrating private messages...");
  let private_message_report = accounts::import_private_messages(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
  )
  .await?;
  eprintln!("[import-all] Migrating interactions...");
  let interaction_report = accounts::import_interactions(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
  )
  .await?;
  eprintln!("[import-all] Migrating tags...");
  let tag_report = accounts::import_tags(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
  )
  .await?;
  eprintln!("[import-all] Migrating media...");
  let media_report = accounts::import_media(
    &staging.path,
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
    preview_report.upload_file_count,
  )
  .await?;

  let legacy_urls_report = if let Some(output) = legacy_urls_output {
    eprintln!("[import-all] Exporting legacy URLs...");
    let (entries, report) = accounts::export_legacy_urls(
      source_database_url,
      site_id,
      fingerprint,
      discourse_version,
      schema_version,
    )
    .await?;
    std::fs::write(output, render_legacy_url_map(&entries))
      .with_context(|| format!("failed to write legacy URL map {}", output.display()))?;
    Some(serde_json::json!({ "output": output.display().to_string(), "report": report }))
  } else {
    None
  };
  eprintln!("[import-all] Validating migration...");
  let validation_report = accounts::validate_import(
    source_database_url,
    site_id,
    fingerprint,
    discourse_version,
    schema_version,
    preview_report.upload_file_count,
  )
  .await?;
  eprintln!("[import-all] All migration phases complete.");

  Ok(serde_json::json!({
    "preview": &preview_report,
    "source_restore": {
      "restored_sql_bytes": restored_bytes,
      "skipped": restored_bytes.is_none(),
      "required_extensions": &preview_report.required_extensions,
      "required_schemas": &preview_report.required_schemas,
      "source_database_modified": restored_bytes.is_some()
    },
    "accounts": account_report,
    "categories": category_report,
    "discussions": discussion_report,
    "private_messages": private_message_report,
    "interactions": interaction_report,
    "tags": tag_report,
    "media": media_report,
    "legacy_urls": legacy_urls_report,
    "validation": validation_report
  }))
}

fn render_legacy_url_map(entries: &[(String, String)]) -> String {
  let mut map = String::from(
    "map $uri $discourse_legacy_key {\n  default \"\";\n  ~^/t/(?:[^/]+/)?(?<discourse_legacy_root_topic>[0-9]+)$ \"$discourse_legacy_root_topic:1\";\n  ~^/t/(?:[^/]+/)?(?<discourse_legacy_reply_topic>[0-9]+)/(?<discourse_legacy_post_number>[0-9]+)$ \"$discourse_legacy_reply_topic:$discourse_legacy_post_number\";\n  ~^/t/(?:[^/]+/)?(?<discourse_legacy_last_topic>[0-9]+)/last$ \"$discourse_legacy_last_topic:last\";\n}\n\nmap $discourse_legacy_key $discourse_legacy_path {\n  default \"\";\n",
  );
  for (source_path, target_path) in entries {
    map.push_str(&format!("  \"{source_path}\" \"{target_path}\";\n"));
  }
  map.push_str("}\n");
  map
}

struct ImportStagingDirectory {
  path: PathBuf,
}

impl Drop for ImportStagingDirectory {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.path);
  }
}

fn extract_archive_once(archive: &Path, max_bytes: u64) -> Result<ImportStagingDirectory> {
  let filename = archive
    .file_name()
    .and_then(|name| name.to_str())
    .context("archive filename must be valid UTF-8")?;
  if !filename.ends_with(".tar.gz") {
    bail!("import-all requires a .tar.gz Discourse archive");
  }
  let staging_path = std::env::temp_dir().join(format!(
    "lemmy-discourse-import-{}-{}",
    std::process::id(),
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .context("system clock is before Unix epoch")?
      .as_nanos()
  ));
  std::fs::create_dir(&staging_path)
    .context("failed to create a temporary archive extraction directory")?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o700))?;
  }
  let staging = ImportStagingDirectory { path: staging_path };
  let mut tar = Command::new("tar");
  tar
    .args(["--extract", "--file"])
    .arg(archive)
    .args(["--directory"])
    .arg(&staging.path)
    .args(["--no-same-owner", "--no-same-permissions"]);
  if pigz_available() {
    tar.arg("--use-compress-program=pigz");
  } else {
    tar.arg("--gzip");
  }
  let status = tar
    .status()
    .context("failed to run tar to extract the archive once")?;
  if !status.success() {
    bail!("tar could not extract the backup archive");
  }

  let members = list_extracted_members(&staging.path)?;
  let dumps = ["dump.sql", "dump.sql.gz"]
    .iter()
    .filter(|dump| members.iter().any(|member| member.as_str() == **dump))
    .copied()
    .collect::<Vec<_>>();
  if dumps.len() != 1 {
    bail!("archive must contain exactly one dump.sql.gz or dump.sql member");
  }
  if dumps[0] == "dump.sql.gz" {
    let mut input = open_gzip_reader(&staging.path.join("dump.sql.gz"))?;
    let mut output = File::create(staging.path.join("dump.sql"))
      .context("failed to create the expanded SQL dump")?;
    let mut expanded_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
      let count = input.read(&mut buffer)?;
      if count == 0 {
        break;
      }
      expanded_bytes = expanded_bytes
        .checked_add(count as u64)
        .context("expanded SQL size overflow")?;
      if expanded_bytes > max_bytes {
        bail!("expanded SQL dump exceeds --max-expanded-bytes");
      }
      output.write_all(&buffer[..count])?;
    }
    output.flush()?;
    std::fs::remove_file(staging.path.join("dump.sql.gz"))?;
  }
  list_extracted_members(&staging.path)?;
  Ok(staging)
}

fn list_extracted_members(root: &Path) -> Result<Vec<String>> {
  fn visit(
    root: &Path,
    directory: &Path,
    members: &mut Vec<String>,
    listing_bytes: &mut u64,
    expanded_bytes: &mut u64,
  ) -> Result<()> {
    let mut entries = std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
      let path = entry.path();
      let metadata = std::fs::symlink_metadata(&path)?;
      if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
        bail!("extracted archive contains a symlink or special file");
      }
      let relative = path
        .strip_prefix(root)
        .context("extracted archive path escaped its staging directory")?;
      let name = relative
        .to_str()
        .context("archive member path is not UTF-8")?
        .replace(std::path::MAIN_SEPARATOR, "/");
      if name.is_empty()
        || name.len() > MAX_MEMBER_NAME_BYTES
        || name.contains('\\')
        || relative.is_absolute()
        || relative
          .components()
          .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
      {
        bail!("archive contains an unsafe or overlong member path");
      }
      *listing_bytes = listing_bytes
        .checked_add(name.len() as u64 + 1)
        .context("archive member listing size overflow")?;
      if *listing_bytes > MAX_MEMBER_LIST_BYTES || members.len() >= MAX_ARCHIVE_MEMBERS {
        bail!("archive member listing exceeds configured safety limits");
      }
      if metadata.is_dir() {
        members.push(format!("{name}/"));
        visit(root, &path, members, listing_bytes, expanded_bytes)?;
      } else {
        *expanded_bytes = expanded_bytes
          .checked_add(metadata.len())
          .context("expanded archive size overflow")?;
        if *expanded_bytes > DEFAULT_MAX_EXPANDED_BYTES {
          bail!("expanded archive exceeds configured safety limits");
        }
        members.push(name);
      }
    }
    Ok(())
  }

  let mut members = Vec::new();
  let mut listing_bytes = 0;
  let mut expanded_bytes = 0;
  visit(
    root,
    root,
    &mut members,
    &mut listing_bytes,
    &mut expanded_bytes,
  )?;
  Ok(members)
}

fn pigz_available() -> bool {
  Command::new("pigz")
    .arg("--version")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .is_ok_and(|status| status.success())
}

fn open_gzip_reader(path: &Path) -> Result<Box<dyn Read>> {
  if pigz_available() {
    let mut child = Command::new("pigz")
      .args(["--decompress", "--stdout", "--"])
      .arg(path)
      .stdout(Stdio::piped())
      .spawn()
      .context("failed to launch pigz for SQL decompression")?;
    let stdout = child
      .stdout
      .take()
      .context("pigz did not provide decompressed output")?;
    Ok(Box::new(ChildReader {
      reader: Some(Box::new(stdout)),
      child: Some(child),
    }))
  } else {
    Ok(Box::new(GzDecoder::new(
      File::open(path).context("failed to open compressed SQL dump")?,
    )))
  }
}

fn database_connection_details(database_url: &str) -> Result<(String, Option<String>)> {
  let mut database_url = url::Url::parse(database_url).context("invalid source database URL")?;
  if !matches!(database_url.scheme(), "postgres" | "postgresql") {
    bail!("source database URL must use PostgreSQL");
  }
  let userinfo_password = database_url.password().map(|value| {
    percent_encoding::percent_decode_str(value)
      .decode_utf8_lossy()
      .into_owned()
  });
  let query_pairs = database_url
    .query_pairs()
    .map(|(key, value)| (key.into_owned(), value.into_owned()))
    .collect::<Vec<_>>();
  let query_password = query_pairs
    .iter()
    .find(|(key, _)| key == "password")
    .map(|(_, value)| value.clone());
  let password = userinfo_password.or(query_password);
  if password.is_some() {
    database_url
      .set_password(None)
      .map_err(|_| anyhow::anyhow!("could not remove database password from connection URL"))?;
    database_url
      .query_pairs_mut()
      .clear()
      .extend_pairs(query_pairs.into_iter().filter(|(key, _)| key != "password"));
  }
  Ok((database_url.to_string(), password))
}

fn restore_source_dump(
  archive: &Path,
  database_url: &str,
  required_extensions: &[String],
  max_bytes: u64,
) -> Result<u64> {
  let (database_url, password) = database_connection_details(database_url)?;
  ensure_restore_database_empty(&database_url, password.as_deref())?;
  create_restore_extensions(&database_url, password.as_deref(), required_extensions)?;
  let filename = archive
    .file_name()
    .and_then(|name| name.to_str())
    .context("archive filename must be valid UTF-8")?;
  let dump_member = if archive.is_dir() {
    Some("dump.sql".to_string())
  } else if filename.ends_with(".tar.gz") {
    let members = list_tar_members(archive)?;
    let dump_members = members
      .iter()
      .filter(|name| name.as_str() == "dump.sql.gz" || name.as_str() == "dump.sql")
      .collect::<Vec<_>>();
    if dump_members.len() != 1 {
      bail!("archive must contain exactly one dump.sql.gz or dump.sql member");
    }
    Some(dump_members[0].clone())
  } else if filename.ends_with(".sql.gz") {
    None
  } else {
    bail!("unsupported backup suffix; expected .tar.gz or .sql.gz");
  };
  let extracted_dump = archive.is_dir().then(|| archive.join("dump.sql"));
  let mut dump = if extracted_dump.is_none() {
    Some(open_dump_reader(archive, dump_member.as_deref())?)
  } else {
    None
  };
  let mut command = Command::new("psql");
  command.args([
    "--no-psqlrc",
    "--set=ON_ERROR_STOP=1",
    "--single-transaction",
    "--quiet",
    "--dbname",
    &database_url,
  ]);
  if let Some(path) = &extracted_dump {
    command.args(["--file"]).arg(path);
  } else {
    command.stdin(Stdio::piped());
  }
  command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
  if let Some(password) = password {
    command.env("PGPASSWORD", password);
  }
  let mut child = command
    .spawn()
    .context("failed to launch psql; install PostgreSQL client tools")?;
  let expanded_bytes = if let Some(path) = extracted_dump {
    let expanded_bytes = std::fs::metadata(path)
      .context("failed to inspect the extracted SQL dump")?
      .len();
    if expanded_bytes > max_bytes {
      let _ = child.kill();
      let _ = child.wait();
      bail!("expanded SQL dump exceeds --max-expanded-bytes");
    }
    expanded_bytes
  } else {
    let Some(mut stdin) = child.stdin.take() else {
      let _ = child.kill();
      let _ = child.wait();
      bail!("psql did not provide standard input");
    };
    let mut expanded_bytes = 0_u64;
    let copy_result = copy_dump_to_psql(
      dump
        .as_mut()
        .context("SQL dump stream is missing")?
        .as_mut(),
      &mut stdin,
      max_bytes,
      &mut expanded_bytes,
    );
    drop(stdin);
    if copy_result.is_err() {
      let _ = child.kill();
    }
    copy_result?;
    expanded_bytes
  };
  let status = child.wait().context("failed waiting for psql restore")?;
  if !status.success() {
    bail!("psql failed to restore the archive into the source database");
  }
  Ok(expanded_bytes)
}

fn create_restore_extensions(
  database_url: &str,
  password: Option<&str>,
  extensions: &[String],
) -> Result<()> {
  for extension in extensions {
    let statement = if extension == "vector" {
      "CREATE EXTENSION IF NOT EXISTS \"vector\" WITH SCHEMA public".to_string()
    } else {
      format!("CREATE EXTENSION IF NOT EXISTS \"{extension}\"")
    };
    let mut command = Command::new("psql");
    command
      .args([
        "--no-psqlrc",
        "--set=ON_ERROR_STOP=1",
        "--dbname",
        database_url,
        "--command",
      ])
      .arg(statement)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());
    if let Some(password) = password {
      command.env("PGPASSWORD", password);
    }
    let output = command
      .output()
      .context("failed to launch psql while creating required source extensions")?;
    if !output.status.success() {
      bail!(
        "could not create required PostgreSQL extension {extension} before restore: {}",
        String::from_utf8_lossy(&output.stderr).trim()
      );
    }
  }
  Ok(())
}

fn ensure_restore_database_empty(database_url: &str, password: Option<&str>) -> Result<()> {
  if !restore_database_is_empty(database_url, password)? {
    bail!(
      "restore database contains user tables, views, sequences, or custom schemas; use a fresh empty database"
    );
  }
  Ok(())
}

fn restore_database_is_empty(database_url: &str, password: Option<&str>) -> Result<bool> {
  const EMPTY_DATABASE_QUERY: &str = "SELECT \
    NOT EXISTS (SELECT 1 FROM pg_catalog.pg_class c \
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
      WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') \
        AND n.nspname NOT LIKE 'pg_toast%' AND c.relkind IN ('r', 'p', 'v', 'm', 'S', 'f')) \
    AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_namespace \
      WHERE nspname NOT IN ('pg_catalog', 'information_schema', 'public') \
        AND nspname NOT LIKE 'pg_toast%')";
  let mut command = Command::new("psql");
  command
    .args([
      "--no-psqlrc",
      "--set=ON_ERROR_STOP=1",
      "--tuples-only",
      "--no-align",
      "--quiet",
      "--dbname",
      database_url,
      "--command",
      EMPTY_DATABASE_QUERY,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  if let Some(password) = password {
    command.env("PGPASSWORD", password);
  }
  let output = command
    .output()
    .context("failed to check the source database before restore")?;
  if !output.status.success() {
    bail!(
      "could not verify that the restore database is empty: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(String::from_utf8_lossy(&output.stdout).trim() == "t")
}

fn prompt_clear_source_database_if_needed(database_url: &str) -> Result<bool> {
  let (safe_url, password) = database_connection_details(database_url)?;
  if restore_database_is_empty(&safe_url, password.as_deref())? {
    return Ok(true);
  }
  eprintln!(
    "The source database is not empty. Choose an action:\n  1) Clear the database and continue\n  2) Skip import\n  3) Cancel"
  );
  let mut answer = String::new();
  std::io::stdin()
    .read_line(&mut answer)
    .context("failed to read the source database action")?;
  match answer.trim().to_ascii_lowercase().as_str() {
    "1" | "clear" => {
      clear_source_database(&safe_url, password.as_deref())?;
      ensure_restore_database_empty(&safe_url, password.as_deref())?;
      Ok(true)
    }
    "2" | "skip" => Ok(false),
    "3" | "cancel" | "" => bail!("restore cancelled; the source database was not modified"),
    _ => bail!("invalid choice; the source database was not modified"),
  }
}

fn clear_source_database(database_url: &str, password: Option<&str>) -> Result<()> {
  const CLEAR_DATABASE_SQL: &str = "DO $$ DECLARE schema_to_drop text; BEGIN \
    FOR schema_to_drop IN SELECT nspname FROM pg_catalog.pg_namespace \
      WHERE nspname NOT IN ('pg_catalog', 'information_schema') \
        AND nspname NOT LIKE 'pg_toast%' LOOP \
      EXECUTE format('DROP SCHEMA %I CASCADE', schema_to_drop); \
    END LOOP; END $$; \
    CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO public";
  let mut command = Command::new("psql");
  command
    .args([
      "--no-psqlrc",
      "--set=ON_ERROR_STOP=1",
      "--single-transaction",
      "--dbname",
      database_url,
      "--command",
      CLEAR_DATABASE_SQL,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  if let Some(password) = password {
    command.env("PGPASSWORD", password);
  }
  let output = command
    .output()
    .context("failed to launch psql while clearing the source database")?;
  if !output.status.success() {
    bail!(
      "could not clear the source database: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(())
}

fn copy_dump_to_psql(
  dump: &mut dyn Read,
  destination: &mut dyn Write,
  max_bytes: u64,
  expanded_bytes: &mut u64,
) -> Result<()> {
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let count = dump.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    *expanded_bytes = expanded_bytes
      .checked_add(count as u64)
      .context("expanded SQL size overflow")?;
    if *expanded_bytes > max_bytes {
      bail!("expanded SQL dump exceeds --max-expanded-bytes");
    }
    destination.write_all(&buffer[..count])?;
  }
  Ok(())
}

fn preview(
  archive: &Path,
  target_site_id: i32,
  account_conflict_policy: ConflictPolicy,
  max_expanded_bytes: u64,
) -> Result<Preview> {
  preview_named(
    archive,
    None,
    target_site_id,
    account_conflict_policy,
    max_expanded_bytes,
  )
}

fn preview_named(
  archive: &Path,
  archive_name: Option<&str>,
  target_site_id: i32,
  account_conflict_policy: ConflictPolicy,
  max_expanded_bytes: u64,
) -> Result<Preview> {
  let filename = archive_name
    .map(str::to_string)
    .or_else(|| {
      archive
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
    })
    .context("archive filename must be valid UTF-8")?;
  let fingerprint = fingerprint(archive)?;
  let (members, dump_member, upload_file_count) =
    if archive.is_dir() || filename.ends_with(".tar.gz") {
      let members = list_tar_members(archive)?;
      let mut dump_members = members
        .iter()
        .filter(|name| name.as_str() == "dump.sql.gz" || name.as_str() == "dump.sql")
        .cloned()
        .collect::<Vec<_>>();
      if dump_members.len() != 1 {
        bail!("archive must contain exactly one dump.sql.gz or dump.sql member");
      }
      let dump_member = dump_members.remove(0);
      let upload_file_count = members
        .iter()
        .filter(|name| name.starts_with("uploads/") && !name.ends_with('/'))
        .count();
      (members, Some(dump_member), upload_file_count)
    } else if filename.ends_with(".sql.gz") {
      (vec![filename.to_string()], None, 0)
    } else {
      bail!("unsupported backup suffix; expected .tar.gz or .sql.gz");
    };

  let mut reader = open_dump_reader(archive, dump_member.as_deref())?;
  let (
    expanded_sql_bytes,
    table_row_counts,
    database_version_from_dump,
    restricted_category_ids,
    required_extensions,
    required_schemas,
  ) = inspect_dump(&mut reader, max_expanded_bytes)?;
  let (discourse_version, database_version_from_filename) = parse_backup_version(&filename);
  let supported = true;
  let mut warnings = Vec::new();
  if upload_file_count == 0 {
    warnings.push("no archived uploads were found; S3-only or database-only uploads are not available in this archive".into());
  }
  if !restricted_category_ids.is_empty() {
    warnings.push(format!(
      "{} categories have read restrictions or category group permissions; those ACLs are reported but not enforced",
      restricted_category_ids.len()
    ));
  }
  let unclassified_tables = table_row_counts
    .keys()
    .filter(|table| {
      table.as_str() != "schema_migrations"
        && table.as_str() != "ar_internal_metadata"
        && !IGNORED_METADATA_TABLES.contains(&table.as_str())
        && !IMPORTED_TABLES.contains(&table.as_str())
        && !PARTIALLY_IMPORTED_TABLES.contains(&table.as_str())
        && !KNOWN_UNSUPPORTED_TABLES.contains(&table.as_str())
    })
    .cloned()
    .collect::<Vec<_>>();
  let unsupported_tables = table_row_counts
    .keys()
    .filter(|table| KNOWN_UNSUPPORTED_TABLES.contains(&table.as_str()))
    .cloned()
    .collect::<Vec<_>>();
  let partially_imported_tables = table_row_counts
    .keys()
    .filter(|table| PARTIALLY_IMPORTED_TABLES.contains(&table.as_str()))
    .cloned()
    .collect::<Vec<_>>();
  if !unclassified_tables.is_empty() {
    warnings.push(format!(
      "{} source tables are not classified by this importer adapter; review them as plugin or unsupported data",
      unclassified_tables.len()
    ));
  }
  if !unsupported_tables.is_empty() {
    warnings.push(format!(
      "{} known source tables contain data not imported by this adapter; review unsupported_tables and retain the source archive",
      unsupported_tables.len()
    ));
  }
  Ok(Preview {
    archive: filename,
    target_site_id,
    account_conflict_policy,
    source_fingerprint_sha256: fingerprint,
    discourse_version,
    database_version_from_filename,
    database_version_from_dump,
    supported,
    required_extensions,
    required_schemas,
    archive_member_count: members.len(),
    upload_file_count,
    expanded_sql_bytes,
    table_row_counts,
    restricted_category_ids,
    partially_imported_tables,
    unsupported_tables,
    unclassified_tables,
    warnings,
  })
}

fn list_tar_members(archive: &Path) -> Result<Vec<String>> {
  if archive.is_dir() {
    return list_extracted_members(archive);
  }
  let mut child = Command::new("tar")
    .args(["-tzf"])
    .arg(archive)
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .context("failed to run tar; install GNU tar or provide a supported archive")?;
  let stdout = child
    .stdout
    .take()
    .context("tar did not provide archive listing output")?;
  let mut listing = BufReader::new(stdout);
  let mut members = Vec::new();
  let mut line = Vec::new();
  let mut total_bytes = 0_u64;
  let result = (|| -> Result<()> {
    loop {
      line.clear();
      let count = listing.read_until(b'\n', &mut line)?;
      if count == 0 {
        break;
      }
      total_bytes = total_bytes
        .checked_add(count as u64)
        .context("archive member listing size overflow")?;
      if total_bytes > MAX_MEMBER_LIST_BYTES || members.len() >= MAX_ARCHIVE_MEMBERS {
        bail!("archive member listing exceeds configured safety limits");
      }
      let raw = line.strip_suffix(b"\n").unwrap_or(&line);
      let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
      if raw.len() > MAX_MEMBER_NAME_BYTES {
        bail!("archive contains an overlong member path");
      }
      let raw = std::str::from_utf8(raw).context("archive member path is not UTF-8")?;
      let name = raw.strip_prefix("./").unwrap_or(raw);
      let path = Path::new(name);
      if name.is_empty()
        || name.contains('\\')
        || path.is_absolute()
        || path
          .components()
          .any(|part| matches!(part, Component::ParentDir | Component::Prefix(_)))
      {
        bail!("archive contains an unsafe member path");
      }
      members.push(name.to_string());
    }
    Ok(())
  })();
  if result.is_err() {
    let _ = child.kill();
  }
  drop(listing);
  let status = child.wait().context("failed to wait for tar listing")?;
  result?;
  if !status.success() {
    bail!("tar could not read the backup archive");
  }
  Ok(members)
}

fn open_dump_reader(archive: &Path, member: Option<&str>) -> Result<Box<dyn Read>> {
  if let Some(member) = member {
    if archive.is_dir() {
      return Ok(Box::new(
        File::open(archive.join(member)).context("failed to open the extracted SQL dump")?,
      ));
    }
    let mut child = Command::new("tar")
      .args(["-xOzf"])
      .arg(archive)
      .arg("--")
      .arg(member)
      .stdout(Stdio::piped())
      .spawn()
      .context("failed to stream the database dump from the archive")?;
    let stdout = child
      .stdout
      .take()
      .context("tar did not provide dump output")?;
    let reader: Box<dyn Read> = if member.ends_with(".gz") {
      Box::new(GzDecoder::new(stdout))
    } else {
      Box::new(stdout)
    };
    // tar's process is held open by its stdout reader; dropping the reader closes the pipe.
    Ok(Box::new(ChildReader {
      reader: Some(reader),
      child: Some(child),
    }))
  } else {
    let file = File::open(archive).context("failed to open SQL backup")?;
    Ok(Box::new(GzDecoder::new(file)))
  }
}

fn read_archive_member(archive: &Path, member: &str, max_bytes: u64) -> Result<Vec<u8>> {
  if archive.is_dir() {
    let mut file = File::open(archive.join(member))
      .with_context(|| format!("failed to open extracted archive member {member}"))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
      .take(max_bytes.saturating_add(1))
      .read_to_end(&mut bytes)
      .with_context(|| format!("failed to read extracted archive member {member}"))?;
    if bytes.len() as u64 > max_bytes {
      bail!("archived upload exceeds the configured per-file size limit");
    }
    return Ok(bytes);
  }
  let mut child = Command::new("tar")
    .args(["-xOzf"])
    .arg(archive)
    .arg("--")
    .arg(member)
    .stdout(Stdio::piped())
    .spawn()
    .context("failed to stream an upload from the archive")?;
  let stdout = child
    .stdout
    .take()
    .context("tar did not provide upload output")?;
  let reader = ChildReader {
    reader: Some(Box::new(stdout)),
    child: Some(child),
  };
  let mut bytes = Vec::new();
  reader
    .take(max_bytes.saturating_add(1))
    .read_to_end(&mut bytes)
    .context("failed to read an archived upload")?;
  if bytes.len() as u64 > max_bytes {
    bail!("archived upload exceeds the configured per-file size limit");
  }
  Ok(bytes)
}

struct ChildReader {
  reader: Option<Box<dyn Read>>,
  child: Option<std::process::Child>,
}

impl Read for ChildReader {
  fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
    let Some(reader) = self.reader.as_mut() else {
      return Ok(0);
    };
    let bytes_read = reader.read(buffer)?;
    if bytes_read == 0 {
      self.reader.take();
      let Some(mut child) = self.child.take() else {
        return Ok(0);
      };
      let status = child.wait()?;
      if !status.success() {
        return Err(std::io::Error::other("tar failed to stream the SQL dump"));
      }
    }
    Ok(bytes_read)
  }
}

impl Drop for ChildReader {
  fn drop(&mut self) {
    self.reader.take();
    if let Some(mut child) = self.child.take() {
      let _ = child.wait();
    }
  }
}

fn inspect_dump(
  reader: &mut dyn Read,
  max_bytes: u64,
) -> Result<(
  u64,
  BTreeMap<String, u64>,
  Option<i64>,
  Vec<i32>,
  Vec<String>,
  Vec<String>,
)> {
  let mut reader = BufReader::new(reader);
  let mut line = Vec::new();
  let mut expanded_bytes = 0_u64;
  let mut table_counts = BTreeMap::new();
  let mut active_table: Option<String> = None;
  let mut active_category_id_index: Option<usize> = None;
  let mut active_restricted_index: Option<usize> = None;
  let mut database_version = None;
  let mut restricted_category_ids = BTreeSet::new();
  let mut required_extensions = BTreeSet::new();
  let mut declared_extensions = BTreeSet::new();
  let mut required_schemas = BTreeSet::from(["discourse_functions".to_string()]);
  loop {
    line.clear();
    let bytes_read = reader.read_until(b'\n', &mut line)?;
    if bytes_read == 0 {
      break;
    }
    expanded_bytes = expanded_bytes
      .checked_add(bytes_read as u64)
      .context("expanded SQL size overflow")?;
    if expanded_bytes > max_bytes {
      bail!("expanded SQL dump exceeds --max-expanded-bytes");
    }
    if let Some(table) = &active_table {
      if line.trim_ascii() == b"\\." {
        active_table = None;
        active_category_id_index = None;
        active_restricted_index = None;
      } else {
        *table_counts.entry(table.clone()).or_insert(0) += 1;
        let fields = line.strip_suffix(b"\n").unwrap_or(&line);
        let fields = fields
          .strip_suffix(b"\r")
          .unwrap_or(fields)
          .split(|byte| *byte == b'\t')
          .collect::<Vec<_>>();
        if let Some(index) = active_restricted_index
          && fields.get(index).is_some_and(|value| *value == b"t")
          && let Some(id) = fields
            .first()
            .and_then(|value| std::str::from_utf8(value).ok())
            .and_then(|value| value.parse::<i32>().ok())
        {
          restricted_category_ids.insert(id);
        }
        if let Some(index) = active_category_id_index
          && let Some(id) = fields
            .as_slice()
            .get(index)
            .and_then(|value| std::str::from_utf8(*value).ok())
            .and_then(|value| value.parse::<i32>().ok())
        {
          restricted_category_ids.insert(id);
        }
        if table == "schema_migrations"
          && let Ok(version) = std::str::from_utf8(line.trim_ascii())
            .unwrap_or_default()
            .parse::<i64>()
        {
          database_version =
            Some(database_version.map_or(version, |previous: i64| previous.max(version)));
        }
      }
      continue;
    }
    if let Some(extension) = parse_create_extension_name(&line) {
      declared_extensions.insert(extension);
    }
    if let Some(schema) = parse_create_schema_name(&line) {
      required_schemas.insert(schema);
    }
    if [
      b"public.halfvec".as_slice(),
      b"public.sparsevec",
      b"public.vector",
      b"\"public\".\"halfvec\"",
      b"\"public\".\"sparsevec\"",
      b"\"public\".\"vector\"",
    ]
    .iter()
    .any(|type_name| contains_ascii_case_insensitive(&line, type_name))
    {
      required_extensions.insert("vector".to_string());
    }
    if let Some(rest) = line.strip_prefix(b"COPY public.")
      && let Some((table, tail)) = copy_section_parts(rest)
      && copy_section_ends_with_stdin(tail)
    {
      if let Ok(table) = std::str::from_utf8(table) {
        let table = table.trim_matches('"').to_string();
        let columns = tail
          .split(|byte| *byte == b'(')
          .nth(1)
          .and_then(|columns| columns.split(|byte| *byte == b')').next())
          .map(|columns| {
            columns
              .split(|byte| *byte == b',')
              .filter_map(|column| std::str::from_utf8(column).ok())
              .map(|column| column.trim().trim_matches('"').to_string())
              .collect::<Vec<_>>()
          })
          .unwrap_or_default();
        if table == "categories" {
          active_restricted_index = columns
            .iter()
            .position(|column| column == "read_restricted");
        } else if table == "category_groups" {
          active_category_id_index = columns.iter().position(|column| column == "category_id");
        }
        active_table = Some(table);
      }
    }
  }
  if table_counts.is_empty() {
    bail!("SQL dump contains no PostgreSQL COPY data sections");
  }
  Ok((
    expanded_bytes,
    table_counts,
    database_version,
    restricted_category_ids.into_iter().collect(),
    required_extensions
      .difference(&declared_extensions)
      .cloned()
      .collect(),
    required_schemas.into_iter().collect(),
  ))
}

fn parse_create_extension_name(line: &[u8]) -> Option<String> {
  let trimmed = line.trim_ascii();
  let prefix = b"CREATE EXTENSION";
  if trimmed.get(..prefix.len())?.eq_ignore_ascii_case(prefix) {
    let mut remainder = trimmed[prefix.len()..].trim_ascii_start();
    let conditional = b"IF NOT EXISTS";
    if remainder
      .get(..conditional.len())
      .is_some_and(|value| value.eq_ignore_ascii_case(conditional))
    {
      remainder = remainder[conditional.len()..].trim_ascii_start();
    }
    let token = remainder.split(|byte| byte.is_ascii_whitespace()).next()?;
    let token = token.strip_suffix(b";").unwrap_or(token);
    let name = token.strip_prefix(b"\"").unwrap_or(token);
    let name = name.strip_suffix(b"\"").unwrap_or(name);
    if !name.is_empty()
      && name
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
    {
      return std::str::from_utf8(name).ok().map(str::to_string);
    }
  }
  None
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
  !needle.is_empty()
    && haystack
      .windows(needle.len())
      .any(|window| window.eq_ignore_ascii_case(needle))
}

fn parse_create_schema_name(line: &[u8]) -> Option<String> {
  let trimmed = line.trim_ascii();
  let prefix = b"CREATE SCHEMA";
  if !trimmed.get(..prefix.len())?.eq_ignore_ascii_case(prefix) {
    return None;
  }
  let mut remainder = trimmed[prefix.len()..].trim_ascii_start();
  let conditional = b"IF NOT EXISTS";
  if remainder
    .get(..conditional.len())
    .is_some_and(|value| value.eq_ignore_ascii_case(conditional))
  {
    remainder = remainder[conditional.len()..].trim_ascii_start();
  }
  let token = remainder.split(|byte| byte.is_ascii_whitespace()).next()?;
  let token = token.strip_suffix(b";").unwrap_or(token);
  let name = token.strip_prefix(b"\"").unwrap_or(token);
  let name = name.strip_suffix(b"\"").unwrap_or(name);
  if !name.is_empty()
    && name
      .iter()
      .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
  {
    return std::str::from_utf8(name).ok().map(str::to_string);
  }
  None
}

fn copy_section_parts(rest: &[u8]) -> Option<(&[u8], &[u8])> {
  let mut parts = rest.splitn(2, |byte| *byte == b' ');
  Some((parts.next()?, parts.next()?))
}

fn copy_section_ends_with_stdin(tail: &[u8]) -> bool {
  tail
    .split(|byte| *byte == b'\n')
    .any(|line| line.trim_ascii().ends_with(b"FROM stdin;"))
}

fn fingerprint(path: &Path) -> Result<String> {
  if path.is_dir() {
    let mut hasher = Sha256::new();
    for member in list_extracted_members(path)? {
      if member.ends_with('/') {
        continue;
      }
      let member_path = path.join(&member);
      hasher.update((member.len() as u64).to_be_bytes());
      hasher.update(member.as_bytes());
      let mut file = File::open(member_path).context("failed to open extracted archive member")?;
      let mut buffer = [0_u8; 64 * 1024];
      loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
          break;
        }
        hasher.update(&buffer[..bytes_read]);
      }
    }
    return Ok(hex::encode(hasher.finalize()));
  }
  let mut file = File::open(path).context("failed to open backup archive")?;
  let mut hasher = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let bytes_read = file.read(&mut buffer)?;
    if bytes_read == 0 {
      break;
    }
    hasher.update(&buffer[..bytes_read]);
  }
  Ok(hex::encode(hasher.finalize()))
}

fn parse_backup_version(filename: &str) -> (Option<String>, Option<i64>) {
  let stem = filename
    .strip_suffix(".tar.gz")
    .or_else(|| filename.strip_suffix(".sql.gz"))
    .unwrap_or(filename);
  let Some((_, encoded)) = stem.rsplit_once("-v") else {
    return (None, None);
  };
  let Some((version, db_version)) = encoded.rsplit_once('-') else {
    return (None, None);
  };
  let discourse_version = version.replace('-', ".");
  let database_version = db_version.parse::<i64>().ok();
  (Some(discourse_version), database_version)
}
