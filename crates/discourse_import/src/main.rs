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

const SUPPORTED_DISCOURSE_VERSION: &str = "2026.7.4";
const SUPPORTED_DATABASE_VERSION: i64 = 20261014065735;
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

#[derive(Clone, Copy, Debug, clap::ValueEnum, Serialize)]
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
      let expanded_bytes = restore_source_dump(&archive, &database_url, max_expanded_bytes)?;
      println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
          "source_fingerprint_sha256": preview_report.source_fingerprint_sha256,
          "database_version": preview_report.database_version_from_dump,
          "restored_sql_bytes": expanded_bytes,
          "source_database_modified": true
        }))?
      );
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

fn restore_source_dump(archive: &Path, database_url: &str, max_bytes: u64) -> Result<u64> {
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
    let safe_query = query_pairs
      .into_iter()
      .filter(|(key, _)| key != "password")
      .collect::<Vec<_>>();
    database_url
      .query_pairs_mut()
      .clear()
      .extend_pairs(safe_query);
  }
  ensure_restore_database_empty(database_url.as_str(), password.as_deref())?;
  let filename = archive
    .file_name()
    .and_then(|name| name.to_str())
    .context("archive filename must be valid UTF-8")?;
  let dump_member = if filename.ends_with(".tar.gz") {
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
  let mut dump = open_dump_reader(archive, dump_member.as_deref())?;
  let mut nonce = [0_u8; 32];
  File::open("/dev/urandom")
    .context("secure randomness is required to protect the psql restore session")?
    .read_exact(&mut nonce)
    .context("failed to generate a psql restore restriction token")?;
  let nonce = hex::encode(nonce);
  let mut command = Command::new("psql");
  command
    .args([
      "--no-psqlrc",
      "--set=ON_ERROR_STOP=1",
      "--single-transaction",
      "--quiet",
      "--dbname",
      database_url.as_str(),
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit());
  if let Some(password) = password {
    command.env("PGPASSWORD", password);
  }
  let mut child = command
    .spawn()
    .context("failed to launch psql; install PostgreSQL client tools")?;
  let Some(mut stdin) = child.stdin.take() else {
    let _ = child.kill();
    let _ = child.wait();
    bail!("psql did not provide standard input");
  };
  let mut expanded_bytes = 0_u64;
  let copy_result = (|| -> Result<()> {
    writeln!(stdin, "\\restrict {nonce}")?;
    stream_sanitized_dump(&mut dump, &mut stdin, max_bytes, &mut expanded_bytes)?;
    writeln!(stdin, "\\unrestrict {nonce}")?;
    Ok(())
  })();
  drop(stdin);
  if copy_result.is_err() {
    let _ = child.kill();
  }
  let status = child.wait().context("failed waiting for psql restore")?;
  copy_result?;
  if !status.success() {
    bail!("psql failed to restore the archive into the source database");
  }
  Ok(expanded_bytes)
}

fn ensure_restore_database_empty(database_url: &str, password: Option<&str>) -> Result<()> {
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
  if String::from_utf8_lossy(&output.stdout).trim() != "t" {
    bail!(
      "restore database contains user tables, views, sequences, or custom schemas; use a fresh empty database"
    );
  }
  Ok(())
}

fn stream_sanitized_dump(
  dump: &mut dyn Read,
  destination: &mut dyn Write,
  max_bytes: u64,
  expanded_bytes: &mut u64,
) -> Result<()> {
  let mut dump = BufReader::new(dump);
  let mut line = Vec::new();
  let mut in_copy_data = false;
  let mut skip_until: Option<&[u8]> = None;
  loop {
    line.clear();
    let count = dump.read_until(b'\n', &mut line)?;
    if count == 0 {
      break;
    }
    *expanded_bytes = expanded_bytes
      .checked_add(count as u64)
      .context("expanded SQL size overflow")?;
    if *expanded_bytes > max_bytes {
      bail!("expanded SQL dump exceeds --max-expanded-bytes");
    }
    if in_copy_data {
      destination.write_all(&line)?;
      if line.as_slice() == b"\\.\n" || line.as_slice() == b"\\.\r\n" || line == b"\\." {
        in_copy_data = false;
      }
      continue;
    }
    if let Some(end_marker) = skip_until {
      if line.trim_ascii() == end_marker {
        skip_until = None;
      }
      continue;
    }
    let trimmed = line.trim_ascii();
    if trimmed.starts_with(b"CREATE FUNCTION discourse_functions") {
      skip_until = Some(b"$$;");
      continue;
    }
    if trimmed.starts_with(b"CREATE SERVER")
      || trimmed.starts_with(b"CREATE USER MAPPING")
      || trimmed.starts_with(b"CREATE FOREIGN TABLE")
    {
      skip_until = Some(b");");
      continue;
    }
    if trimmed.starts_with(b"DROP SCHEMA")
      || trimmed.starts_with(b"CREATE SCHEMA")
      || trimmed.starts_with(b"COMMENT ON SCHEMA")
      || trimmed.starts_with(b"SET default_table_access_method")
      || trimmed.starts_with(b"SET transaction_timeout")
      || trimmed.starts_with(b"CREATE EXTENSION")
      || trimmed.starts_with(b"COMMENT ON EXTENSION")
      || trimmed.starts_with(b"\\restrict")
      || trimmed.starts_with(b"\\unrestrict")
    {
      continue;
    }
    destination.write_all(&line)?;
    if is_copy_data_start(&line) {
      in_copy_data = true;
    }
  }
  if in_copy_data || skip_until.is_some() {
    bail!("SQL dump ended inside a COPY section or an unsupported DDL block");
  }
  Ok(())
}

fn is_copy_data_start(line: &[u8]) -> bool {
  line.strip_prefix(b"COPY public.").is_some_and(|rest| {
    rest
      .windows(b"FROM stdin;".len())
      .any(|window| window == b"FROM stdin;")
  })
}

fn preview(
  archive: &Path,
  target_site_id: i32,
  account_conflict_policy: ConflictPolicy,
  max_expanded_bytes: u64,
) -> Result<Preview> {
  let filename = archive
    .file_name()
    .and_then(|name| name.to_str())
    .context("archive filename must be valid UTF-8")?;
  let fingerprint = fingerprint(archive)?;
  let (members, dump_member, upload_file_count) = if filename.ends_with(".tar.gz") {
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
  let (expanded_sql_bytes, table_row_counts, database_version_from_dump, restricted_category_ids) =
    inspect_dump(&mut reader, max_expanded_bytes)?;
  let (discourse_version, database_version_from_filename) = parse_backup_version(filename);
  let supported = discourse_version.as_deref() == Some(SUPPORTED_DISCOURSE_VERSION)
    && database_version_from_filename == Some(SUPPORTED_DATABASE_VERSION)
    && database_version_from_dump == Some(SUPPORTED_DATABASE_VERSION);
  let mut warnings = Vec::new();
  if !supported {
    warnings.push(format!(
      "only Discourse {SUPPORTED_DISCOURSE_VERSION} with database schema {SUPPORTED_DATABASE_VERSION} is supported by this importer build"
    ));
  }
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
    archive: filename.to_string(),
    target_site_id,
    account_conflict_policy,
    source_fingerprint_sha256: fingerprint,
    discourse_version,
    database_version_from_filename,
    database_version_from_dump,
    supported,
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
) -> Result<(u64, BTreeMap<String, u64>, Option<i64>, Vec<i32>)> {
  let mut reader = BufReader::new(reader);
  let mut line = Vec::new();
  let mut expanded_bytes = 0_u64;
  let mut table_counts = BTreeMap::new();
  let mut active_table: Option<String> = None;
  let mut active_category_id_index: Option<usize> = None;
  let mut active_restricted_index: Option<usize> = None;
  let mut database_version = None;
  let mut restricted_category_ids = BTreeSet::new();
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
      if line.as_slice() == b"\\.\n" || line.as_slice() == b"\\.\r\n" {
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
    if let Some(rest) = line.strip_prefix(b"COPY public.")
      && let Some((table, tail)) = copy_section_parts(rest)
      && tail
        .windows(b"FROM stdin;\n".len())
        .any(|window| window == b"FROM stdin;\n")
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
  ))
}

fn copy_section_parts(rest: &[u8]) -> Option<(&[u8], &[u8])> {
  let mut parts = rest.splitn(2, |byte| *byte == b' ');
  Some((parts.next()?, parts.next()?))
}

fn fingerprint(path: &Path) -> Result<String> {
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
