## 1. Inventory and authorization model

- [x] 1.1 Inventory v4 routes and v3 adapters with actor, action, target, site, permission checks, and side effects; verify each registered route has an audit entry.
- [x] 1.2 Inventory federation handlers, media routes, scheduled jobs, queues, and background tasks with the same fields; verify every entry point has an owner and audit outcome.
- [x] 1.3 Define target-aware site and community authorization helpers; verify same-site, cross-site, moderator, and installation-admin cases with tests.
- [x] 1.4 Scope administrator hierarchy and account role changes to the current site; verify cross-site grants and removals fail and same-site hierarchy remains intact.

## 2. Configuration ownership and migration

- [x] 2.1 Add site ownership and site-scoped constraints for OAuth providers, emoji, and taglines; verify schema and per-site uniqueness with database tests.
- [x] 2.2 Implement legacy record inventory and explicit multi-site mapping preflight; verify missing, invalid, and conflicting OAuth associations fail without mutation.
- [x] 2.3 Backfill ownership transactionally, preserving IDs and OAuth associations; test single-site and mapped multi-site migrations and failure rollback.
- [x] 2.4 Scope OAuth provider CRUD, site responses, login, callbacks, and account linking; test same-site login and cross-site provider rejection before token exchange.
- [x] 2.5 Scope emoji and tagline CRUD, lists, and random selection; test cross-site IDs, list isolation, and site-specific cache invalidation.

## 3. Moderation and destructive operations

- [x] 3.1 Restrict person, community, post, and comment purge to eligible site-owned targets; test authorized and cross-site IDs before deletion or queuing.
- [x] 3.2 Protect shared objects, other sites' presence, and referenced media during purge; test shared references and remote objects remain available.
- [x] 3.3 Scope site bans, unbans, and bulk content cleanup to the acting site; test profile, votes, posts, comments, messages, and asynchronous effects across two sites.
- [x] 3.4 Apply target-aware authorization to community moderation, transfer, bans, and content controls; test explicit moderators and unrelated site administrators.
- [x] 3.5 Scope report reads, resolution, bulk resolution, and counts, including private messages; test cross-site IDs, returned data, and count isolation.

## 4. Audit and integration

- [x] 4.1 Audit authentication, account, registration, invitations, and administrative routes; fix each confirmed gap and record its regression test in the inventory.
- [x] 4.2 Audit community, post, comment, voting, report, and moderation routes; fix each confirmed gap and record its regression test in the inventory.
- [x] 4.3 Audit private messages, notifications, media, downloads, and related direct-ID reads and writes; fix each confirmed gap and record its regression test in the inventory.
- [x] 4.4 Audit federation, internal delivery, caches, queues, and asynchronous operations; fix each confirmed gap and verify authorization and site scope survive indirect execution.
- [x] 4.5 Exercise v3 adapters and v4 endpoints with anonymous users, ordinary users, moderators, site admins, and installation admins; verify equivalent authorization for shared handlers.
- [x] 4.6 Run multi-site integration tests for same-site success, cross-site rejection, shared remote objects, and unchanged database, media, cache, notification, and queue state after rejection.
- [x] 4.7 Document operator mapping, backup, migration, rollback, API behavior changes, and audit coverage; verify the instructions against migration tests and the completed inventory.
