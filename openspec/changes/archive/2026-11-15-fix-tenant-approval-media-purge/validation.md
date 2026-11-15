# Validation

The tests ran against a disposable PostgreSQL 18.6 cluster on `127.0.0.1:55432`. The sandbox denied socket creation, so the cluster and test processes ran with the existing elevated local-execution allowance. The database was dropped and recreated before each suite because several legacy fixtures use a fixed `my_domain.tld` identity.

| Command | Result |
| --- | --- |
| `cargo test -p lemmy_db_schema --features full --lib impls::media::tests -- --test-threads=1` | 5 passed: shared uploads, content/presence reconciliation, site-scoped deletion, concurrent upload/delete, concurrent edits. |
| `cargo test -p lemmy_db_schema --features full --lib impls::purge_guard::tests -- --test-threads=1` | 1 passed: foreign descendants, presence, actions, reports, media references, and concurrent dependency insertion. |
| `cargo test -p lemmy_db_schema --features full --lib impls::person::tests::delete_account_clears_only_the_site_bound_login -- --test-threads=1` | 1 passed. |
| `cargo test -p lemmy_api --features full --lib site::registration_applications::tests -- --test-threads=1` | 1 passed. |
| `cargo test -p lemmy_api --features full --lib site::configuration_tenant_tests -- --test-threads=1` | 4 passed: registration decisions, site settings, full configuration isolation, and scoped site-ban removal. |
| `cargo test -p lemmy_api --features full --lib site::mod_log::tests::test_mod_remove_or_restore_data -- --test-threads=1` | 1 passed. |
| `LEMMY_TEST_PICTRS_PORT=18081 cargo test -p lemmy_api_utils --features full --lib request::tests::queued_media_deletion_survives_storage_failure -- --test-threads=1` | 1 passed with a local mock pict-rs: a 500 response retained the queue and media, and a new context retried successfully. |
| `cargo check --workspace --tests` | Passed. |
| `openspec validate fix-tenant-approval-media-purge --strict` | Valid. |
| `git diff --check` | Passed. |

`cargo test -p lemmy_diesel_utils --features full --lib schema_setup::tests::test_schema_setup -- --test-threads=1` ran every migration in both directions and reached the final schema assertion. That assertion reports three pre-existing foreign keys without supporting indexes: `report_site_resolver_id_fkey`, `report_activity_report_combined_id_fkey`, and `site_configuration_migration_map_local_site_id_fkey`. None belongs to the media reference tables or the new migration. This is recorded as a baseline schema issue rather than hidden by changing the check.

`cargo fmt --all` was run. The repository's rustfmt configuration uses nightly-only import options, so a stable `rustfmt --check` reports ordering differences unrelated to this change. The unrelated formatting-only edits produced by the workspace-wide format were reverted to keep the change reviewable; the files changed for this change were compiled successfully after formatting.
