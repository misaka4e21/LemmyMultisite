# Tenant Authorization Audit Inventory

This is the reviewable record for the "Complete authorization audit" requirement. Each entry lists
the entry point, how site scope is derived, the authorization rule, side effects, and regression
evidence. Tests that need a database were run against a disposable PostgreSQL 18 instance.

## Scope derivation

- Public reads derive the site from the request host through `TenantContext`. Handlers that change
  protected data also require `LocalUserView`, so the acting identity and the target site come from
  server state, never from a request body field.
- Installation administrator tokens are separate claims. They authorize installation routes only
  and are rejected by normal site-user claims validation
  (`installation_admin::tests::installation_token_is_not_a_site_session`).
- Target-aware helpers: `check_community_global_mod_action` requires site ownership or an explicit
  community moderator role. `check_community_mod_action` keeps the existing moderator and site
  administrator rules for objects owned by the current site.

## v4 route groups (`crates/api/routes/src/lib.rs`)

| Area | Representative routes | Site and target checks | Side effects | Evidence |
| --- | --- | --- | --- | --- |
| Site configuration | `/site`, `/site/icon`, `/site/banner`, `/modlog` | Current site from tenant context; OAuth, emoji and tagline rows carry `local_site_id`; responses are cached per site | Cache invalidation is scoped to the affected site | `site::configuration_tenant_tests::site_configuration_is_isolated_and_cache_is_invalidated` |
| Installation | `/installation/login`, `/installation/sites` | Installation claims only; no site-user authority | Site lifecycle and hostname changes | `installation_admin` unit test; build of `lemmy_routes` |
| OAuth | Site response, create, update, delete, login, register/link | Provider read and mutation by site; login and callback compare the provider site before token exchange | Provider rows, OAuth accounts and site cache | `site::oauth_tenant_tests::oauth_provider_is_bound_to_request_site` |
| Custom emoji and tagline | List, create, update, delete, random site response | Every query filters `local_site_id`; update and delete read the row with the same site | Site-scoped configuration and cache | Configuration tenant test above |
| Community management | Create, edit, delete, remove, transfer, mod, ban, pending follows, tag | `check_community_global_mod_action` for shared state; community ownership or explicit moderator role | Community rows, follower approvals, modlog and federation | `utils::tests::global_community_actions_require_site_ownership_or_mod_role` |
| Posts and comments | Create, edit, delete, remove, lock, distinguish, feature, save, vote | Existing community/user checks; shared moderation paths use target-aware checks | Content rows, votes and federation | `cargo test -p lemmy_api --lib` |
| Reports | Create, resolve, combined list and count | `report_site` binds each receiving site; resolution and reads require a row for the caller's site; per-site resolution state is overlaid on direct reads | Report state, federation activity and counts | `lemmy_db_views_report_combined` suite (9 tests) |
| Private messages and notifications | Create, edit, delete, notification list and reads | Message ownership plus `private_message_site` removal state; notification reads filter removed site state | Notification rows and read marks | `lemmy_db_views_notification` suite (4 tests) |
| Media | Upload, proxy, download, delete, list all media | Upload validates the acting account; media references and aliases are deleted per site and owner | Pictrs aliases, media references, S3 objects | `cargo build --workspace`; media code review |
| Accounts and registration | Register, authenticate, password reset, email verification, login list, registration application | Local user and site from server context; installation admins are rejected as site users | Accounts, tokens, applications and email jobs | `cargo test -p lemmy_api --lib` |
| Invitations | Create, list, revoke | Acting local user's site; invitation rows carry the site | Invitation validity and email jobs | Code review of `api_crud/src/invite` |
| Purge | Person, community, post, comment | `is_admin` plus current-site match and target `origin_local_site_id`; person purge reads the target by site before scheduling | Deletion, modlog, federation and asynchronous cleanup | `site::configuration_tenant_tests` purge assertions |

## v3 adapters

`crates/api/routes_v3` and `crates/api/api_crud` route to the same v4 handlers and shared helpers.
The v3 adapter tests were not run against a live server in this workspace; the shared handler tests
above cover the authorization code both adapters call.

## Federation, queues and background work

- Federation ingest handlers resolve the receiving site from the request host and keep the same
  target checks as local handlers.
- `ActivityChannel` now carries the request `Data<LemmyContext>` with each queued activity, so a
  cross-site request cannot reuse another site's context. Regression test:
  `send_activity::tests::outgoing_queue_keeps_request_context`.
- Rejected cross-site operations return before modlog creation, queue submission or cache
  invalidation. The purge tenant test asserts unchanged database rows, unchanged modlog counts and
  no queued activity.
- Media cleanup resolves the site from context and deletes an alias only after
  `MediaReference::delete_for_site_and_user` reports that the site and user owned it.

## Data migration coverage

- `scripts/site-configuration-migration.md` and `scripts/prepare-site-configuration-migration.sql`
  describe operator mapping, validation and rollback for OAuth, emoji and taglines.
- `scripts/report-site-migration.md` and `scripts/audit-report-site-migration.sql` describe the
  per-site report association and the audit of unassigned legacy reports.
- Migration tests were exercised through the disposable database schema setup used by the test runs
  above. A production rollback requires the documented backup restore when the old schema cannot
  represent site-specific rows.

## Verification notes

- `cargo build --workspace` and `cargo check --workspace --tests` pass.
- `cargo test -p lemmy_api --lib`, the report view suite, the notification suite and the new helper
  tests pass when each crate runs against a clean disposable database. Several older test modules
  share fixed person names and fail if the whole workspace runs in one database without per-test
  cleanup; that is test isolation, not an authorization result.
