mod diff_check;
use anyhow::{Context, anyhow};
use chrono::TimeDelta;
use diesel::{
  BoolExpressionMethods,
  Connection,
  ExpressionMethods,
  PgConnection,
  QueryDsl,
  RunQueryDsl,
  connection::SimpleConnection,
  dsl::exists,
  migration::{Migration, MigrationVersion},
  pg::Pg,
  select,
  update,
};
use diesel_migrations::MigrationHarness;
use std::time::Instant;
use tracing::debug;

diesel::table! {
  pg_namespace (nspname) {
    nspname -> Text,
  }
}

diesel::table! {
  previously_run_sql (id) {
    id -> Bool,
    content -> Text,
  }
}

fn migrations() -> diesel_migrations::EmbeddedMigrations {
  // Using `const` here is required by the borrow checker
  const MIGRATIONS: diesel_migrations::EmbeddedMigrations = diesel_migrations::embed_migrations!();
  MIGRATIONS
}

/// This SQL code sets up the `r` schema, which contains things that can be safely dropped and
/// replaced instead of being changed using migrations. It may not create or modify things outside
/// of the `r` schema (indicated by `r.` before the name), unless a comment says otherwise.
fn replaceable_schema() -> String {
  [
    "CREATE SCHEMA r;",
    include_str!("../../replaceable_schema/utils.sql"),
    include_str!("../../replaceable_schema/triggers.sql"),
  ]
  .join("\n")
}

const REPLACEABLE_SCHEMA_PATH: &str = "crates/diesel_utils/replaceable_schema";

struct MigrationHarnessWrapper<'a> {
  conn: &'a mut PgConnection,
  #[cfg(test)]
  enable_diff_check: bool,
  options: &'a Options,
}

impl MigrationHarnessWrapper<'_> {
  fn run_migration_inner(
    &mut self,
    migration: &dyn Migration<Pg>,
  ) -> diesel::migration::Result<MigrationVersion<'static>> {
    let start_time = Instant::now();

    let result = self.conn.run_migration(migration);

    let duration = TimeDelta::from_std(start_time.elapsed())
      .map(|d| d.to_string())
      .unwrap_or_default();
    let name = migration.name();
    self.options.print(&format!("{duration} run {name}"));

    result
  }
}

impl MigrationHarness<Pg> for MigrationHarnessWrapper<'_> {
  fn run_migration(
    &mut self,
    migration: &dyn Migration<Pg>,
  ) -> diesel::migration::Result<MigrationVersion<'static>> {
    #[cfg(test)]
    if self.enable_diff_check {
      let before = diff_check::get_dump();

      self.run_migration_inner(migration)?;
      self.revert_migration(migration)?;

      let after = diff_check::get_dump();

      diff_check::check_dump_diff(
        [&after, &before],
        &format!(
          "These changes need to be applied in migrations/{}/down.sql:",
          migration.name()
        ),
      );
    }

    self.run_migration_inner(migration)
  }

  fn revert_migration(
    &mut self,
    migration: &dyn Migration<Pg>,
  ) -> diesel::migration::Result<MigrationVersion<'static>> {
    let start_time = Instant::now();

    let result = self.conn.revert_migration(migration);

    let duration = TimeDelta::from_std(start_time.elapsed())
      .map(|d| d.to_string())
      .unwrap_or_default();
    let name = migration.name();
    self.options.print(&format!("{duration} revert {name}"));

    result
  }

  fn applied_migrations(&mut self) -> diesel::migration::Result<Vec<MigrationVersion<'static>>> {
    self.conn.applied_migrations()
  }
}

#[derive(Default, Clone, Copy)]
pub struct Options {
  #[cfg(test)]
  enable_diff_check: bool,
  revert: bool,
  run: bool,
  print_output: bool,
  limit: Option<u64>,
}

impl Options {
  #[cfg(test)]
  fn enable_diff_check(mut self) -> Self {
    self.enable_diff_check = true;
    self
  }

  pub fn run(mut self) -> Self {
    self.run = true;
    self
  }

  pub fn revert(mut self) -> Self {
    self.revert = true;
    self
  }

  pub fn limit(mut self, limit: u64) -> Self {
    self.limit = Some(limit);
    self
  }

  /// If print_output is true, use println!.
  /// Otherwise, use debug!
  pub fn print_output(mut self) -> Self {
    self.print_output = true;
    self
  }

  fn print(&self, text: &str) {
    if self.print_output {
      println!("{text}");
    } else {
      debug!("{text}");
    }
  }
}

/// Checked by tests
#[derive(PartialEq, Eq, Debug)]
pub enum Branch {
  EarlyReturn,
  ReplaceableSchemaRebuilt,
  ReplaceableSchemaNotRebuilt,
}

pub fn run(options: Options, db_url: &str) -> anyhow::Result<Branch> {
  // Migrations don't support async connection, and this function doesn't need to be async
  let conn = &mut PgConnection::establish(db_url)?;

  // If possible, skip getting a lock and recreating the "r" schema, so
  // lemmy_server processes in a horizontally scaled setup can start without causing locks
  if !options.revert
    && options.run
    && options.limit.is_none()
    && !conn
      .has_pending_migration(migrations())
      .map_err(convert_err)?
  {
    // The condition above implies that the migration that creates the previously_run_sql table was
    // already run
    let sql_unchanged = exists(
      previously_run_sql::table.filter(previously_run_sql::content.eq(replaceable_schema())),
    );

    let schema_exists = exists(pg_namespace::table.find("r"));

    if select(sql_unchanged.and(schema_exists)).get_result(conn)? {
      return Ok(Branch::EarlyReturn);
    }
  }

  // Block concurrent attempts to run migrations until `conn` is closed, and disable the
  // trigger that prevents the Diesel CLI from running migrations
  options.print("Waiting for lock...");
  conn.batch_execute("SELECT pg_advisory_lock(0);")?;
  options.print("Running Database migrations (This may take a long time)...");

  // Drop `r` schema, so migrations don't need to be made to work both with and without things in
  // it existing
  revert_replaceable_schema(conn)?;

  run_selected_migrations(conn, &options).map_err(convert_err)?;

  // Only run replaceable_schema if newest migration was applied
  let output = if (options.run && options.limit.is_none())
    || !conn
      .has_pending_migration(migrations())
      .map_err(convert_err)?
  {
    #[cfg(test)]
    if options.enable_diff_check {
      let before = diff_check::get_dump();

      run_replaceable_schema(conn)?;
      revert_replaceable_schema(conn)?;

      let after = diff_check::get_dump();

      diff_check::check_dump_diff(
        [&before, &after],
        "The code in crates/diesel_utils/replaceable_schema incorrectly created or modified things outside of the `r` schema, causing these changes to be left behind after dropping the schema:",
      );

      diff_check::deferr_constraint_check(&after);
    }

    run_replaceable_schema(conn)?;

    Branch::ReplaceableSchemaRebuilt
  } else {
    Branch::ReplaceableSchemaNotRebuilt
  };

  options.print("Database migrations complete.");

  Ok(output)
}

fn run_replaceable_schema(conn: &mut PgConnection) -> anyhow::Result<()> {
  conn.transaction(|conn| {
    conn
      .batch_execute(&replaceable_schema())
      .with_context(|| format!("Failed to run SQL files in {REPLACEABLE_SCHEMA_PATH}"))?;

    let num_rows_updated = update(previously_run_sql::table)
      .set(previously_run_sql::content.eq(replaceable_schema()))
      .execute(conn)?;

    debug_assert_eq!(num_rows_updated, 1);

    Ok(())
  })
}

fn revert_replaceable_schema(conn: &mut PgConnection) -> anyhow::Result<()> {
  conn
    .batch_execute("DROP SCHEMA IF EXISTS r CASCADE;")
    .with_context(|| format!("Failed to revert SQL files in {REPLACEABLE_SCHEMA_PATH}"))?;

  // Value in `previously_run_sql` table is not set here because the table might not exist,
  // and that's fine because the existence of the `r` schema is also checked

  Ok(())
}

fn run_selected_migrations(
  conn: &mut PgConnection,
  options: &Options,
) -> diesel::migration::Result<()> {
  let mut wrapper = MigrationHarnessWrapper {
    conn,
    options,
    #[cfg(test)]
    enable_diff_check: options.enable_diff_check,
  };

  if options.revert {
    if let Some(limit) = options.limit {
      for _ in 0..limit {
        wrapper.revert_last_migration(migrations())?;
      }
    } else {
      wrapper.revert_all_migrations(migrations())?;
    }
  }

  if options.run {
    if let Some(limit) = options.limit {
      for _ in 0..limit {
        wrapper.run_next_migration(migrations())?;
      }
    } else {
      wrapper.run_pending_migrations(migrations())?;
    }
  }

  Ok(())
}

/// Makes `diesel::migration::Result` work with `anyhow` and `LemmyError`
fn convert_err(e: Box<dyn std::error::Error + Send + Sync>) -> anyhow::Error {
  anyhow!(e)
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
  use super::{
    Branch::{EarlyReturn, ReplaceableSchemaNotRebuilt, ReplaceableSchemaRebuilt},
    *,
  };
  use diesel::{
    dsl::{not, sql},
    sql_types,
  };
  use diesel_ltree::Ltree;
  use lemmy_utils::{error::LemmyResult, settings::SETTINGS};
  use serial_test::serial;
  // The number of migrations that should be run to set up some test data.
  // Currently, this includes migrations until
  // 2020-04-07-135912_add_user_community_apub_constraints, since there are some mandatory apub
  // fields need to be added.

  const INITIAL_MIGRATIONS_COUNT: u64 = 40;

  // Migrations after the initial set until the table needed to seed the site registry exists.
  // The last migration in this range predates `local_site` but already added `site.actor_id`.
  const MIGRATIONS_UNTIL_SITE_ACTOR: u64 = 86;

  // Migrations from after `local_site` creation until the migration that starts site-scoped
  // federation. The scoped tables must contain test rows before that migration runs.
  const MIGRATIONS_UNTIL_SITE_SCOPED_FEDERATION: u64 = 222;

  const TEST_SITE_NAME: &str = "test_site";
  const TEST_SITE_ACTOR_ID: &str = "https://test.example/site";
  const TEST_SITE_INBOX_URL: &str = "https://test.example/site/inbox";
  const TEST_SITE_HOSTNAME: &str = "test.example";
  const REMOTE_INSTANCE_DOMAIN: &str = "remote.example";
  const LEGACY_PICTRS_ALIAS: &str = "legacy_upload_alias";

  // Test data IDs
  const TEST_USER_ID_1: i32 = 101;
  const USER1_NAME: &str = "test_user_1";
  const USER1_ACTOR_ID: &str = "test_user_1@fedi.example";
  const USER1_PREFERRED_NAME: &str = "preferred_1";
  const USER1_EMAIL: &str = "email1@example.com";
  const USER1_PASSWORD: &str = "test_password_1";
  const USER1_PUBLIC_KEY: &str = "test_public_key_1";

  const TEST_USER_ID_2: i32 = 102;
  const USER2_NAME: &str = "test_user_2";
  const USER2_ACTOR_ID: &str = "test_user_2@fedi.example";
  const USER2_PREFERRED_NAME: &str = "preferred2";
  const USER2_EMAIL: &str = "email2@example.com";
  const USER2_PASSWORD: &str = "test_password_2";
  const USER2_PUBLIC_KEY: &str = "test_public_key_2";

  const TEST_COMMUNITY_ID_1: i32 = 101;
  const COMMUNITY_NAME: &str = "test_community_1";
  const COMMUNITY_TITLE: &str = "Test Community 1";
  const COMMUNITY_DESCRIPTION: &str = "This is a test community.";
  const CATEGORY_ID: i32 = 4; // Should be a valid category "Movies"
  const COMMUNITY_ACTOR_ID: &str = "https://fedi.example/community/12345";
  const COMMUNITY_PUBLIC_KEY: &str = "test_public_key_community_1";

  const TEST_POST_ID_1: i32 = 101;
  const POST_NAME: &str = "Post Title";
  const POST_URL: &str = "https://fedi.example/post/12345";
  const POST_BODY: &str = "Post Body.";
  const POST_AP_ID: &str = "https://fedi.example/post/12345";

  const TEST_COMMENT_ID_1: i32 = 101;
  const COMMENT1_CONTENT: &str = "Comment";
  const COMMENT1_AP_ID: &str = "https://fedi.example/comment/12345";

  const TEST_COMMENT_ID_2: i32 = 102;
  const COMMENT2_CONTENT: &str = "Reply";
  const COMMENT2_AP_ID: &str = "https://fedi.example/comment/12346";

  #[test]
  #[serial]
  fn test_schema_setup() -> LemmyResult<()> {
    let o = Options::default();
    // Migrations use session variables such as `lemmy.protocol_and_hostname`, so connect with the
    // same options that `lemmy_server` injects.
    let db_url = SETTINGS.get_database_url_with_options()?;
    let conn = &mut PgConnection::establish(&db_url)?;

    // Start with consistent state by dropping everything
    conn.batch_execute("DROP OWNED BY CURRENT_USER;")?;

    // Run initial migrations to prepare basic tables
    assert_eq!(
      run(o.run().limit(INITIAL_MIGRATIONS_COUNT), &db_url)?,
      ReplaceableSchemaNotRebuilt
    );

    // Insert the test data
    insert_test_data(conn)?;

    // The site registry predates `local_site`. Insert the site once the actor columns exist; the
    // migration that creates `local_site` copies the site row into it. Inserting the site any
    // earlier would make the per-migration down scripts drop the seeded row again.
    assert_eq!(
      run(o.run().limit(MIGRATIONS_UNTIL_SITE_ACTOR), &db_url)?,
      ReplaceableSchemaNotRebuilt
    );
    conn.batch_execute(&format!(
      "INSERT INTO site (name, actor_id, inbox_url) VALUES ('{}', '{}', '{}')",
      TEST_SITE_NAME, TEST_SITE_ACTOR_ID, TEST_SITE_INBOX_URL
    ))?;
    assert_eq!(
      run(o.run().limit(1).enable_diff_check(), &db_url)?,
      ReplaceableSchemaNotRebuilt
    );
    assert_eq!(
      run(
        o.run()
          .limit(MIGRATIONS_UNTIL_SITE_SCOPED_FEDERATION)
          .enable_diff_check(),
        &db_url
      )?,
      ReplaceableSchemaNotRebuilt
    );
    insert_site_scoped_test_data(conn)?;

    // Run all migrations, and make sure that changes can be correctly reverted
    assert_eq!(
      run(o.run().enable_diff_check(), &db_url)?,
      ReplaceableSchemaRebuilt
    );

    // Check the test data we inserted before after running migrations
    check_test_data(conn)?;
    check_multisite_backfill(conn)?;

    // Check the current schema
    assert_eq!(
      get_foreign_keys_with_missing_indexes(conn)?,
      Vec::<String>::new(),
      "each foreign key needs an index so that deleting the referenced row does not scan the whole referencing table"
    );

    // Check for early return
    assert_eq!(run(o.run(), &db_url)?, EarlyReturn);

    // Test `limit`
    assert_eq!(
      run(o.revert().limit(1), &db_url)?,
      ReplaceableSchemaNotRebuilt
    );
    assert_eq!(
      conn
        .pending_migrations(migrations())
        .map_err(convert_err)?
        .len(),
      1
    );
    assert_eq!(run(o.run().limit(1), &db_url)?, ReplaceableSchemaRebuilt);

    // Get a new connection, workaround for error `cache lookup failed for function 26633`
    // on `migrations/2025-10-15-114811-0000_merge-modlog-tables/down.sql`.
    let conn = &mut PgConnection::establish(&db_url)?;

    // This should throw an error saying to use lemmy_server instead of diesel CLI
    conn.batch_execute("DROP OWNED BY CURRENT_USER;")?;
    assert!(matches!(
      conn.run_pending_migrations(migrations()),
      Err(e) if e.to_string().contains("lemmy_server")
    ));

    // Diesel CLI's way of running migrations shouldn't break the custom migration runner
    assert_eq!(run(o.run(), &db_url)?, ReplaceableSchemaRebuilt);

    Ok(())
  }

  fn insert_test_data(conn: &mut PgConnection) -> LemmyResult<()> {
    // Users
    conn.batch_execute(&format!(
      "INSERT INTO user_ (id, name, actor_id, preferred_username, password_encrypted, email, public_key) \
          VALUES ({}, '{}', '{}', '{}', '{}', '{}', '{}')",
      TEST_USER_ID_1,
      USER1_NAME,
      USER1_ACTOR_ID,
      USER1_PREFERRED_NAME,
      USER1_PASSWORD,
      USER1_EMAIL,
      USER1_PUBLIC_KEY
    ))?;

    conn.batch_execute(&format!(
      "INSERT INTO user_ (id, name, actor_id, preferred_username, password_encrypted, email, public_key) \
          VALUES ({}, '{}', '{}', '{}', '{}', '{}', '{}')",
      TEST_USER_ID_2,
      USER2_NAME,
      USER2_ACTOR_ID,
      USER2_PREFERRED_NAME,
      USER2_PASSWORD,
      USER2_EMAIL,
      USER2_PUBLIC_KEY
    ))?;

    // Community
    conn.batch_execute(&format!(
      "INSERT INTO community (id, actor_id, public_key, name, title, description, category_id, creator_id) \
          VALUES ({}, '{}', '{}', '{}', '{}', '{}', {}, {})",
      TEST_COMMUNITY_ID_1,
      COMMUNITY_ACTOR_ID,
      COMMUNITY_PUBLIC_KEY,
      COMMUNITY_NAME,
      COMMUNITY_TITLE,
      COMMUNITY_DESCRIPTION,
      CATEGORY_ID,
      TEST_USER_ID_1
    ))?;

    conn.batch_execute(&format!(
      "INSERT INTO community_moderator (community_id, user_id) \
          VALUES ({}, {})",
      TEST_COMMUNITY_ID_1, TEST_USER_ID_1
    ))?;

    // Post
    conn.batch_execute(&format!(
      "INSERT INTO post (id, name, url, body, creator_id, community_id, ap_id) \
          VALUES ({}, '{}', '{}', '{}', {}, {}, '{}')",
      TEST_POST_ID_1,
      POST_NAME,
      POST_URL,
      POST_BODY,
      TEST_USER_ID_1,
      TEST_COMMUNITY_ID_1,
      POST_AP_ID
    ))?;

    // Comment
    conn.batch_execute(&format!(
      "INSERT INTO comment (id, creator_id, post_id, parent_id, content, ap_id) \
           VALUES ({}, {}, {}, NULL, '{}', '{}')",
      TEST_COMMENT_ID_1, TEST_USER_ID_2, TEST_POST_ID_1, COMMENT1_CONTENT, COMMENT1_AP_ID
    ))?;

    conn.batch_execute(&format!(
      "INSERT INTO comment (id, creator_id, post_id, parent_id, content, ap_id) \
           VALUES ({}, {}, {}, {}, '{}', '{}')",
      TEST_COMMENT_ID_2,
      TEST_USER_ID_1,
      TEST_POST_ID_1,
      TEST_COMMENT_ID_1,
      COMMENT2_CONTENT,
      COMMENT2_AP_ID
    ))?;

    conn.batch_execute(&format!(
      "INSERT INTO comment_like (user_id, comment_id, post_id, score) \
           VALUES ({}, {}, {}, {})",
      TEST_USER_ID_1, TEST_COMMENT_ID_1, TEST_POST_ID_1, 1
    ))?;

    Ok(())
  }

  /// Rows that exist before the site-scoped federation migration, so the test can check that the
  /// migration backfills their tenant columns instead of only accepting empty tables.
  fn insert_site_scoped_test_data(conn: &mut PgConnection) -> LemmyResult<()> {
    conn.batch_execute(&format!(
      "INSERT INTO instance (domain) VALUES ('{REMOTE_INSTANCE_DOMAIN}') ON CONFLICT (domain) DO NOTHING;
       INSERT INTO federation_queue_state (instance_id, fail_count)
       SELECT id, 0 FROM instance WHERE domain = '{REMOTE_INSTANCE_DOMAIN}';
       INSERT INTO federation_allowlist (instance_id)
       SELECT id FROM instance WHERE domain = '{REMOTE_INSTANCE_DOMAIN}';
       INSERT INTO sent_activity (ap_id, data, sensitive, send_inboxes, send_all_instances, actor_type)
       VALUES ('https://{TEST_SITE_HOSTNAME}/activities/1', '{{}}'::json, false, '{{}}', false, 'site');
       INSERT INTO received_activity (ap_id)
       VALUES ('https://{REMOTE_INSTANCE_DOMAIN}/activities/1');
       INSERT INTO local_image (pictrs_alias, person_id, thumbnail_for_post_id)
       VALUES ('{LEGACY_PICTRS_ALIAS}', {TEST_USER_ID_1}, {TEST_POST_ID_1});"
    ))?;
    Ok(())
  }

  fn check_multisite_backfill(conn: &mut PgConnection) -> LemmyResult<()> {
    use lemmy_db_schema_file::schema::{
      comment,
      comment_site,
      community,
      community_site,
      federation_allowlist,
      federation_queue_state,
      local_site,
      local_user,
      media_object,
      media_reference,
      person,
      person_site,
      post,
      post_site,
      received_activity,
      sent_activity,
    };

    let (site_id, hostname): (i32, String) = local_site::table
      .select((local_site::id, local_site::hostname))
      .first(conn)?;
    assert_eq!(hostname, TEST_SITE_HOSTNAME);

    let person_origins: Vec<(i32, Option<i32>)> = person::table
      .select((person::id, person::origin_local_site_id))
      .order_by(person::id)
      .load(conn)?;
    assert!(person_origins.len() >= 2);
    assert!(
      person_origins
        .iter()
        .all(|(_, origin)| *origin == Some(site_id))
    );

    let community_origins: Vec<Option<i32>> = community::table
      .select(community::origin_local_site_id)
      .load(conn)?;
    assert_eq!(community_origins, vec![Some(site_id)]);

    let post_origins: Vec<Option<i32>> =
      post::table.select(post::origin_local_site_id).load(conn)?;
    assert_eq!(post_origins, vec![Some(site_id)]);

    let comment_origins: Vec<Option<i32>> = comment::table
      .select(comment::origin_local_site_id)
      .load(conn)?;
    assert_eq!(comment_origins, vec![Some(site_id), Some(site_id)]);

    let local_user_sites: Vec<i32> = local_user::table
      .select(local_user::local_site_id)
      .order_by(local_user::local_site_id)
      .load(conn)?;
    assert_eq!(local_user_sites, vec![site_id, site_id]);

    let person_count: i64 = person::table.count().get_result(conn)?;
    let community_count: i64 = community::table.count().get_result(conn)?;
    let post_count: i64 = post::table.count().get_result(conn)?;
    let comment_count: i64 = comment::table.count().get_result(conn)?;
    let person_presence: i64 = person_site::table.count().get_result(conn)?;
    let community_presence: i64 = community_site::table.count().get_result(conn)?;
    let post_presence: i64 = post_site::table.count().get_result(conn)?;
    let comment_presence: i64 = comment_site::table.count().get_result(conn)?;
    assert_eq!(person_presence, person_count);
    assert_eq!(community_presence, community_count);
    assert_eq!(post_presence, post_count);
    assert_eq!(comment_presence, comment_count);

    let sent_sites: Vec<i32> = sent_activity::table
      .select(sent_activity::local_site_id)
      .load(conn)?;
    assert_eq!(sent_sites, vec![site_id]);

    let received_sites: Vec<i32> = received_activity::table
      .select(received_activity::local_site_id)
      .load(conn)?;
    assert_eq!(received_sites, vec![site_id]);

    let queue_sites: Vec<i32> = federation_queue_state::table
      .select(federation_queue_state::local_site_id)
      .load(conn)?;
    assert_eq!(queue_sites, vec![site_id]);

    let allowlist_sites: Vec<i32> = federation_allowlist::table
      .select(federation_allowlist::local_site_id)
      .load(conn)?;
    assert_eq!(allowlist_sites, vec![site_id]);

    let media_aliases: Vec<String> = media_object::table
      .select(media_object::canonical_pictrs_alias)
      .load(conn)?;
    assert_eq!(media_aliases, vec![LEGACY_PICTRS_ALIAS.to_string()]);

    let media_references: Vec<(i32, Option<i32>, Option<i32>)> = media_reference::table
      .select((
        media_reference::local_site_id,
        media_reference::owner_person_id,
        media_reference::post_id,
      ))
      .load(conn)?;
    assert_eq!(
      media_references,
      vec![(site_id, Some(TEST_USER_ID_1), Some(TEST_POST_ID_1))]
    );

    Ok(())
  }

  fn check_test_data(conn: &mut PgConnection) -> LemmyResult<()> {
    use lemmy_db_schema_file::schema::{comment, community, notification, person, post};

    // Check users
    let users: Vec<(i32, String, Option<String>, String, String)> = person::table
      .filter(person::bot_account.eq(false))
      .select((
        person::id,
        person::name,
        person::display_name,
        person::ap_id,
        person::public_key,
      ))
      .order_by(person::id)
      .load(conn)
      .map_err(|e| anyhow!("Failed to read users: {}", e))?;

    assert_eq!(users.len(), 2);
    assert_eq!(users[0].0, TEST_USER_ID_1);
    assert_eq!(users[0].1, USER1_NAME);
    assert_eq!(users[0].2.clone().unwrap(), USER1_PREFERRED_NAME);
    assert_eq!(users[0].3, USER1_ACTOR_ID);
    assert_eq!(users[0].4, USER1_PUBLIC_KEY);

    assert_eq!(users[1].0, TEST_USER_ID_2);
    assert_eq!(users[1].1, USER2_NAME);
    assert_eq!(users[1].2.clone().unwrap(), USER2_PREFERRED_NAME);
    assert_eq!(users[1].3, USER2_ACTOR_ID);
    assert_eq!(users[1].4, USER2_PUBLIC_KEY);

    // Check communities
    let communities: Vec<(i32, String, String, String)> = community::table
      .select((
        community::id,
        community::name,
        community::ap_id,
        community::public_key,
      ))
      .load(conn)
      .map_err(|e| anyhow!("Failed to read communities: {}", e))?;

    assert_eq!(communities.len(), 1);
    assert_eq!(communities[0].0, TEST_COMMUNITY_ID_1);
    assert_eq!(communities[0].1, COMMUNITY_NAME);
    assert_eq!(communities[0].2, COMMUNITY_ACTOR_ID);
    assert_eq!(communities[0].3, COMMUNITY_PUBLIC_KEY);

    let posts: Vec<(i32, String, String, Option<String>, i32, i32)> = post::table
      .select((
        post::id,
        post::name,
        post::ap_id,
        post::body,
        post::community_id,
        post::creator_id,
      ))
      .load(conn)
      .map_err(|e| anyhow!("Failed to read posts: {}", e))?;

    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].0, TEST_POST_ID_1);
    assert_eq!(posts[0].1, POST_NAME);
    assert_eq!(posts[0].2, POST_AP_ID);
    assert_eq!(posts[0].3.clone().unwrap(), POST_BODY);
    assert_eq!(posts[0].4, TEST_COMMUNITY_ID_1);
    assert_eq!(posts[0].5, TEST_USER_ID_1);

    let comments: Vec<(i32, String, String, i32, i32, Ltree, i32)> = comment::table
      .select((
        comment::id,
        comment::content,
        comment::ap_id,
        comment::post_id,
        comment::creator_id,
        comment::path,
        comment::upvotes,
      ))
      .order_by(comment::id)
      .load(conn)
      .map_err(|e| anyhow!("Failed to read comments: {}", e))?;

    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].0, TEST_COMMENT_ID_1);
    assert_eq!(comments[0].1, COMMENT1_CONTENT);
    assert_eq!(comments[0].2, COMMENT1_AP_ID);
    assert_eq!(comments[0].3, TEST_POST_ID_1);
    assert_eq!(comments[0].4, TEST_USER_ID_2);
    assert_eq!(
      comments[0].5,
      Ltree(format!("0.{}", TEST_COMMENT_ID_1).to_string())
    );
    assert_eq!(comments[0].6, 1); // One upvote

    assert_eq!(comments[1].0, TEST_COMMENT_ID_2);
    assert_eq!(comments[1].1, COMMENT2_CONTENT);
    assert_eq!(comments[1].2, COMMENT2_AP_ID);
    assert_eq!(comments[1].3, TEST_POST_ID_1);
    assert_eq!(comments[1].4, TEST_USER_ID_1);
    assert_eq!(
      comments[1].5,
      Ltree(format!("0.{}.{}", TEST_COMMENT_ID_1, TEST_COMMENT_ID_2).to_string())
    );
    assert_eq!(comments[1].6, 0); // Zero upvotes

    // Check comment replies
    let replies: Vec<(Option<i32>, i32)> = notification::table
      .select((notification::comment_id, notification::recipient_id))
      .order_by(notification::comment_id)
      .load(conn)
      .map_err(|e| anyhow!("Failed to read comment replies: {}", e))?;

    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].0, Some(TEST_COMMENT_ID_1));
    assert_eq!(replies[0].1, TEST_USER_ID_1);
    assert_eq!(replies[1].0, Some(TEST_COMMENT_ID_2));
    assert_eq!(replies[1].1, TEST_USER_ID_2);

    Ok(())
  }

  const FOREIGN_KEY: &str = "f";

  fn get_foreign_keys_with_missing_indexes(conn: &mut PgConnection) -> LemmyResult<Vec<String>> {
    diesel::table! {
      pg_constraint (table_oid, name, kind, column_numbers) {
        #[sql_name = "conrelid"]
        table_oid -> Oid,
        #[sql_name = "conname"]
        name -> Text,
        #[sql_name = "contype"]
        kind -> Text,
        #[sql_name = "conkey"]
        column_numbers -> Array<Int2>,
      }
    }

    diesel::table! {
      pg_index (table_oid, key_length, column_numbers) {
        #[sql_name = "indrelid"]
        table_oid -> Oid,
        #[sql_name = "indnkeyatts"]
        key_length -> Int2,
        #[sql_name = "indkey"]
        column_numbers -> Array<Int2>,
      }
    }

    diesel::allow_tables_to_appear_in_same_query!(pg_constraint, pg_index);

    let matching_index = pg_index::table
      .filter(pg_index::table_oid.eq(pg_constraint::table_oid))
      // Check if the index's key (not columns listed with `INCLUDE`) starts with the foreign key.
      // TODO: use Diesel array slice function when it's added.
      .filter(sql::<sql_types::Bool>(
        "((pg_index.indkey[:pg_index.indnkeyatts])[:array_length(pg_constraint.conkey, 1)] = pg_constraint.conkey)"
      ));

    let res = pg_constraint::table
      .select(pg_constraint::name)
      .filter(pg_constraint::kind.eq(FOREIGN_KEY))
      .filter(not(exists(matching_index)))
      .load(conn)?;

    Ok(res)
  }
}
