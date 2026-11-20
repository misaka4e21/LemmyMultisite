# Discourse archive import

This document records the source-to-target mapping and operational limits of the offline importer. The import reports identify imported and transformed data, retained-only uploads, skipped conflicts, unresolved references, and unsupported source records.

The current CLI provides read-only archive preview:

```sh
cargo run -p lemmy_discourse_import -- preview discourse-backup.tar.gz --site-id 1 --account-conflicts report
cargo run -p lemmy_discourse_import -- preview discourse-backup.sql.gz --site-id 1 --account-conflicts rename --report preview.json --max-expanded-bytes 137438953472
```

After restoring the dump, preview the source accounts against the selected target site before writing accounts. This read-only command reports existing username/email collisions, the deterministic username alias and email action for `rename`, and whether conflicts block `report` or `skip` policies:

```sh
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- preview-accounts discourse-backup.tar.gz \
  --site-id 1 --account-conflicts report --report account-conflicts.json
```

The report sets `target_database_modified` to `false`. A `rename` policy appends `_d<source-user-id>` to a colliding normalized username and omits a conflicting primary email; if that deterministic alias is already taken, the report sets `username_alias_collision` and `blocks_import`. Review the report before running `import-accounts` with the same policy.

Preview reports table row counts, supported source tables, partially imported tables (`partially_imported_tables`), known tables with data that this adapter does not import (`unsupported_tables`), and tables absent from the checked-out source schema (`unclassified_tables`). “Imported” means the importer reads records from that table for a target conversion; it does not imply every source row is eligible or that every field survives. `category_groups` is partial because its records only trigger access-restriction warnings, and `optimized_images` is partial because source paths are rewritten to the imported original image without retaining rendition sizing. `groups`, `group_users`, `category_users`, `post_revisions`, and `user_actions` are reported as unsupported rather than imported: their rows are counted, but their records are not migrated. The adapter's imported, partial, unsupported, and metadata classifications have been compared against the supported Discourse schema; every table in that schema has a classification. Archive-specific table counts and plugin tables still need to be checked against the actual backup in preview. The partial list includes Chat messages and channels: direct-message channel records are imported as Lemmy private messages, while public Chat channels, threads, reactions, and other Chat semantics are not. Other enabled-plugin tables such as reactions and voting plugins remain unsupported; this inventory is not a claim that those records have been migrated. Retain the source archive for unsupported data. Rails metadata tables such as `schema_migrations` are excluded from both data lists.

`restore-source` executes the archive's SQL against the database URL supplied in `DISCOURSE_RESTORE_DATABASE_URL`; it is a destructive restore operation. Use a dedicated, disposable, empty PostgreSQL database that is isolated from production and from the LemmyMultisite target. Before restore, the command checks that the database has no user tables, views, sequences, foreign tables, or custom schemas and refuses to continue otherwise. Do not point it at the target database or any database containing data you need to keep. The command does not create, truncate, or clean up the database for you. After restoring, expose it to the import commands through a read-only PostgreSQL role where possible.

The first write phase imports accounts from that restored, isolated Discourse PostgreSQL database:

```sh
DISCOURSE_RESTORE_DATABASE_URL='postgres://restore_user@127.0.0.1/discourse_import' \
cargo run -p lemmy_discourse_import -- restore-source discourse-backup.tar.gz

DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-accounts discourse-backup.tar.gz --site-id 1 --account-conflicts report --report account-import.json
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-categories discourse-backup.tar.gz --site-id 1 --report category-import.json
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-discussions discourse-backup.tar.gz --site-id 1 --report discussion-import.json
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-private-messages discourse-backup.tar.gz --site-id 1 --report private-message-import.json
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-interactions discourse-backup.tar.gz --site-id 1 --report interaction-import.json
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-tags discourse-backup.tar.gz --site-id 1 --report tag-import.json
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- import-media discourse-backup.tar.gz --site-id 1 --report media-import.json
```

The source database URL is read from the environment so credentials need not appear in the command arguments. The target connection uses LemmyMultisite's normal configuration. Import commands validate the archive/version before writing, reject a target whose federation is enabled, use the durable run and source-ID mapping tables, and print a JSON result report. Import accounts first; expired Discourse password rows are treated as accounts without a usable imported verifier and require account recovery. The category phase requires a local user to own parent multi-communities. Parent categories become multi-communities containing their descendant imported communities; leaves and parents with topics become communities. Because Lemmy limits a multi-community to fewer than 50 entries, a parent with 50 or more mapped descendant communities is reported unresolved and blocks completion. Preview and the category report list category IDs with `read_restricted` enabled or with `category_groups` permissions, but those ACLs are not enforced. Import categories before discussions. Public ordinary topics become posts and their regular replies become comments in parent-first post-number order; the discussion phase excludes private-message topics and non-regular system/whisper posts. Run `import-private-messages` after accounts; it imports ordinary PM posts and direct-message Chat messages for mapped participants, creates target-site presence rows, and preserves timestamps. Group conversations are represented as one-to-one copies per recipient and listed in the report because Lemmy has no group chat model. Public Chat channels, threads, reactions, and Chat-message attachment references are not converted; preview identifies the Chat tables as partially imported and the report warns if the Chat tables are absent. Run `import-interactions` after public discussions; it maps Discourse's ordinary like action to an upvote and imports `Post` and `Topic` bookmarks where their target mapping exists. Other action types, plugin bookmark types, and PM bookmarks are not converted to votes or public bookmarks. Missing authors, participants, categories, topic starters, or reply parents are reported as unresolved and leave the relevant phase failed. Completing a phase does not mean the site migration is complete.

The account report exposes source-state and unsupported-password notices in a `warnings` array, separate from username/email `conflicts`; it maps Discourse `approved` to Lemmy `accepted_application` and only marks an email verified when the source account is active. Accounts that were inactive or unapproved are also imported with Lemmy's `person.deleted` gate so login remains blocked independently of target-site registration settings. The report includes inactive or unapproved, currently suspended, currently silenced, staged, Discourse admin/moderator, and profile-visibility restriction counts with source user IDs. Lemmy cannot represent Discourse suspension dates, silencing, staged-account state, or profile-visibility restrictions; those are reported for operator review and are not enforced by import. Discourse admin and moderator roles never grant Lemmy privileges automatically.

After all content phases, run the validation command. It compares source counts to the durable mapping ledger, verifies topic/reply authorship, community and parent relationships, private-message participants, vote/bookmark actors, descendant community membership, and selected-site ownership, confirms federation remains disabled, and blocks completion on mismatches or failed phases. For imported generic attachments, validation also checks each distinct S3 object key is present. The tag phase imports public topic tags as community tags and attaches them to the corresponding Lemmy posts. Run `import-private-messages` before `import-media`. Media import stores checksum-verified raster images in pict-rs and policy-eligible documents in generic S3 storage. Public references retain the existing site-level behavior. Private-message references are bound to each imported target message copy; a download checks that the current requester is a participant who can still read that message. Post-restricted references are bound to the mapped target post and checked against current post visibility on every request. Group messages use pairwise copies, each with its own binding. An unmappable protected upload stays inaccessible, is listed in `unresolved_protected_upload_ids`, and leaves the media phase failed. Avatar and category images are rewritten only when their source and target mappings resolve. Image and file requests must pass through the authorization-aware Lemmy routes; direct pict-rs access must not be exposed for protected aliases. Media import and validation also require access to configured pict-rs and, for non-image attachments, generic S3 storage.

```sh
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- validate discourse-backup.tar.gz \
  --site-id 1 --report validation.json
```

`restore-source` first runs the read-only preview/version gate, then streams the archive SQL into `psql` in a single transaction; it does not unpack an intermediate SQL file or require a temporary extraction directory. The archive must remain readable for the full command, and PostgreSQL needs enough storage for the restored source database. It requires PostgreSQL client tools and an operator-provisioned **empty, isolated source database**; use credentials with restore privileges only on that database. Import commands need read access to the restored source and write access to the target, including its import-ledger tables; media import also needs access to the configured pict-rs service. Database passwords are passed to the `psql` child through its environment, not command arguments. Import commands then connect to that restored source database and the separate Lemmy target. Never point the restore URL at the Lemmy database.

Preview requires an explicit positive `--site-id` and prints JSON (or writes it to `--report`) containing the archive SHA-256 fingerprint, target site, account conflict policy, detected versions, table row counts, upload-file count, and warnings. `--account-conflicts` accepts `report`, `rename`, or `skip` and controls account import collision behavior. Its supported-schema gate is intentionally strict until additional source-version adapters and fixtures are added. Preview never connects to the target Lemmy database, so the selected conflict policy is an operator decision rather than a precomputed list of target collisions. Actual collisions are detected by `import-accounts` and included in its result report.

The report also lists source tables not classified by the active adapter, so plugin data and newly added schema tables remain visible for operator review. Tags are created separately for each target community because Lemmy tags are community-scoped; source tag names are converted to safe ASCII identifiers while their original spelling is retained as the display name.

## Cutover runbook

1. Take and verify a restorable backup of the LemmyMultisite database and media store. Record the selected LocalSite ID and confirm federation is disabled for that site.
2. Pause writes to the target site and keep them paused through import and validation. Retain the original Discourse service and archive unchanged.
3. Restore the archive into a separate empty PostgreSQL database with `restore-source`, then run `preview`. Choose the account conflict policy before the account import: `rename` assigns a deterministic `_d<source-id>` username suffix and omits a colliding email; `report` and `skip` leave conflicting accounts unmapped and block later phases.
4. Run account, category, public discussion, private-message, interaction, tag, and media imports in that order. Keep federation disabled. A failed phase must be resolved and rerun before moving on.
5. Run `validate`. Do not enable redirects or resume writes unless it exits successfully and its report has no blocking reasons. Media import must complete when the archive contains upload rows or files; unsupported, inaccessible, or unresolved references are reported and block completion.
6. Generate the legacy URL map, validate the proxy configuration, and activate redirects only after validation succeeds. Keep the old Discourse host under operator control.
7. Resume target writes after checking representative accounts, discussions, private messages, and redirects. Retain the source archive and source database until the migration has been accepted.
8. After acceptance and the required retention period, drop the isolated restored-source database and remove temporary reports that are no longer needed. Keep the original archive according to the archive-retention policy, and keep the generated proxy map while redirects remain active.

If rollback is needed, pause target writes, restore the pre-import database and media backup as a matched snapshot, remove the generated proxy include, and route the old hostname back to Discourse. Do not restore only the database while retaining post-import media state, or vice versa.

## Archive and version data

Discourse backup archives are gzip-compressed tar files containing `dump.sql.gz` and sometimes an `uploads/` tree. A database-only backup is `.sql.gz`. The inspected Discourse source is version `2026.7.4`; its backup filename encodes that version and the database schema version, and the SQL dump's `schema_migrations` rows provide an independent schema-version check. The source's `backup_metadata` table stores site settings such as base URLs, not these version values. The importer uses the filename and schema migration version to select its declared source adapter and reject unknown schema shapes during preview. The source version alone is not sufficient because the database schema can advance independently.

The source database restore is isolated from the Lemmy target. Archive entries must be validated before extraction; source user content and file names are untrusted input. Preview must not write to the target.

Imported credential rows use an encoded verifier through `create_imported_account`, which creates the `Person`, `LocalUser`, site presence, language assignments, and source mapping in one transaction. It calls the migration-only `LocalUser::create_with_password_verifier` path; ordinary account creation and password reset continue to hash plaintext with Argon2id. Login upgrades supported bcrypt and Discourse PBKDF2 verifiers to Argon2id after successful authentication.

Discourse stores `password_algorithm`, `password_salt`, and `password_hash` separately. The supported PBKDF2-SHA256 encoding uses a 32-byte output and a source salt string passed as UTF-8 bytes; the salt happens to be 32 hexadecimal characters in supported source rows and is not hex-decoded before PBKDF2. The verifier accepts 1 through 5,000,000 iterations to bound imported credential cost; unsupported algorithms, malformed rows, and higher iteration counts are imported without a usable password and require account recovery. New and reset passwords use Argon2id. Argon2id verifiers must use version 19, at most 65,536 KiB, four iterations, and parallelism four; bcrypt verifiers must use a cost no higher than 16. These bounds apply both when importing encoded verifiers and before login verification. Existing bcrypt verifiers within those limits remain accepted and are upgraded after successful login. Password comparison and hashing run in blocking workers; a failed login does not rewrite the stored verifier.

## Mapping inventory

| Discourse data | LemmyMultisite target | Initial handling |
| --- | --- | --- |
| `users`, `user_emails`, `user_profiles`, `user_options`, `user_passwords` | `person`, `local_user`, profile fields | Create one site-owned identity per source account. Map `approved` to `accepted_application`, `active` to email verification, and preserve PBKDF2 inputs for compatibility; map `allow_private_messages` and `external_links_in_new_tab` to the matching local-user preferences; append source location and website to the imported biography because the target profile has no separate fields; mark accounts without a usable credential for recovery. Emails and usernames require explicit conflict handling. |
| `categories` | `community` | Every leaf category maps to one community. A parent that contains child categories and its own topics also receives a community. |
| Parent/child category structure | `multi_community`, `multi_community_entry` | Each parent category maps to a multi-community containing its descendant leaf communities. |
| Category permissions and `category_user` policy | No direct equivalent | Report source visibility and posting restrictions; do not claim they remain enforced. |
| `topics`, first ordinary `posts` row | `post` | One topic starter maps to one post. Preserve topic title, body, author, timestamps, status, and source URL mapping where supported. |
| Remaining ordinary `posts` rows and `reply_to_post_number` | `comment` | Map replies to comments and reconstruct the reply tree from source topic/post numbers. |
| `post_actions` where action type is Discourse `like` | `post_actions`, `comment_actions` | Convert the active Discourse like (source type ID 2) to a positive Lemmy vote on its imported post/comment. Preserve the source action timestamp; report all other action types as unsupported. |
| `bookmarks` with `bookmarkable_type` `Post` or `Topic` | `post_actions.saved_at`, `comment_actions.saved_at` | Import when the bookmarked public source object resolves to an imported post or comment. Unsupported plugin types and private-message objects are reported. |
| `user_actions`, reactions, badges, edit history, wiki revisions | No equivalent or partial equivalent | Retain counts/metadata in the report or a source manifest; no semantic equivalence is implied. |
| `posts` in PM topics, `topic_allowed_users` | `private_message`, `private_message_site`, read `notification` rows | Import ordinary PM posts with sender/recipient visibility, chronology, and the notification rows Lemmy requires to list message history. Imported PM notifications are marked read because per-message unread state is not represented consistently by the source. Group conversations are fanned out as one-to-one copies per recipient and listed in the report because Lemmy has no group chat model. |
| `chat_channels` with `DirectMessage` chatables, `direct_message_channels`, `direct_message_users`, and matching `chat_messages` | `private_message`, `private_message_site`, read `notification` rows | Import direct-message Chat history for mapped participants, preserving message timestamps and deleted state; group-channel messages are copied once per current recipient and channel IDs are reported. Imported PM notifications are marked read. Discourse stores current channel membership, so participants who previously left cannot be reconstructed. Public Chat channels, threads, reactions, and chat upload references remain unsupported. The channel/message tables are listed as partially imported in preview. |
| `polls`, `poll_options`, `poll_votes` | No native poll model | Render a clearly labeled frozen prompt/options/vote-count summary only when Discourse's result mode and close state (including an elapsed `close_at`) made results visible to all users at migration time. Open `on_vote` results are voter/author-specific and are not made public; `staff_only` results remain hidden even after closing. The durable report retains option IDs, source voter IDs (including anonymous/null voters), vote rank, timestamps, close state, and the visibility decision. Do not create a fake interactive poll. |
| `uploads`, `upload_references`, `optimized_images`, and archive `uploads/` files | pict-rs media, generic site file storage, or retained-only report | Resolve normal upload paths and Discourse `upload://` short URLs from the source SHA-1 and upload ID; when the `sha1` column is empty, derive it from the original URL's SHA-1 filename. Derive fallback archive paths with Discourse's 16-way BaseStore depth rule and verify the resulting SHA-1. Import public raster images into pict-rs, and import non-image files only when generic S3 storage and target upload policy permit them and the owner maps. Rewrite public discussion references, attach resolved user-avatar uploads, and apply category logo/background uploads to mapped communities. Bind PM and supported Chat direct-message uploads to their target private-message copies; bind secure or `access_control_post_id` uploads to a target post only when its current visibility can enforce access. Every request checks current target permissions. Unmappable protected uploads remain inaccessible and block media completion. Optimized variants resolve to the imported original image, so source thumbnail sizing is not preserved. |
| Discourse tags and topic tags | `community_tag`, `post_community_tag` | Import public topic tags into the topic's community, preserving original spelling as display text and recording community-scoped source tag mappings. |
| Groups, trust levels, moderation logs, plugin tables | No direct equivalent | Report as unsupported unless a later adapter defines an explicit target behavior. |

## Required invariants

- Every imported `person`, `community`, post, comment, private message, and multi-community is owned by the selected LocalSite and has the necessary site-presence records.
- The `discourse_import_run` ledger is keyed by target LocalSite and archive SHA-256; `discourse_import_mapping` stores stable source-to-target IDs; `discourse_import_phase` stores resumable phase cursors. Replaying a source mapping is idempotent only when its target ID agrees; conflicting mappings stop the phase. Source IDs must not be forced into target primary keys.
- Replies are imported only after their topic starter and parent reply mappings exist. Private messages are imported only after both account mappings exist.
- The import path may persist read private-message notification rows required for message history, but must not invoke notification hooks, send emails, submit activities, or create outbound federation queue records.
- Unsupported or unresolved relationships remain visible in the final report and prevent completion when the target object would otherwise be orphaned.

## Legacy URL mapping

After the public discussion phase is complete, export an Nginx `map` include:

```sh
DISCOURSE_DATABASE_URL='postgres://migration_reader@127.0.0.1/discourse_import' \
LEMMY_DATABASE_URL='postgres://lemmy_migrator@127.0.0.1/lemmy' \
cargo run -p lemmy_discourse_import -- export-legacy-urls discourse-backup.tar.gz \
  --site-id 1 --output discourse-legacy-map.conf --report legacy-url-report.json
```

The generated file contains two Nginx `map` directives: a constant-size set of regular expressions extracts a numeric topic/post key from slugged or ID-only routes, then exact map entries resolve that key to an imported Lemmy path. This keeps regex matching constant-size even for large archives. Include the generated file in the `http` context and configure the old hostname and LemmyMultisite origin in the deployment's proxy config, for example:

```nginx
http {
  include /path/configured/by/operator/discourse-legacy-map.conf;
}

server {
  # This server block is attached to the operator's old hostname.
  set $lemmyms_origin https://target-host-configured-by-operator;
  if ($discourse_legacy_path != "") {
    return 301 $lemmyms_origin$discourse_legacy_path;
  }
}
```

Topic paths map to `/post/{post_id}` and reply paths map to `/post/{post_id}/{comment_id}`, matching current frontend routes. The extractor handles both ID-only and slugged Discourse `/t/` paths, including stale slugs, and `/last` routes map to the last imported post or reply. No Discourse hostname is embedded in frontend assets or the generated map.
