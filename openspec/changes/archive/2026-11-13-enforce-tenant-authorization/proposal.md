## Why

Some endpoints still rely on single-site administrator privileges and global object IDs. In a multi-site installation, they can allow one site to modify another site's accounts, configuration, reports, or content. The existing investigation did not cover every entry point, so the known defects need to be fixed alongside a complete authorization audit.

## What Changes

- Scope administrator permissions, account and community management, reports, bans, purges, and their side effects to the authorized site. Distinguish locally owned objects from shared federated objects.
- Assign OAuth providers, custom emoji, and taglines to individual sites. Scope configuration, listings, site responses, and OAuth login, and migrate existing records.
- Audit v4 and v3 APIs, media, private messages, federation, and background work for identity, target permissions, and tenant scope. Fix confirmed gaps discovered by the audit.
- Add regression tests for cross-site rejection, authorized same-site operations, shared objects, migration, and asynchronous side effects.
- **BREAKING**: Site administrators can no longer operate on another site's data. Previously global site configuration becomes site-specific.

## Capabilities

### New Capabilities

- `multisite/tenant-authorization`: Authorization boundaries across entry points, audit coverage, and side effects of rejected actions.

### Modified Capabilities

- `multisite/site-administration`: Tenant scope for administrative targets and site configuration.
- `multisite/local-identity`: OAuth provider ownership and account authorization scope.
- `multisite/activitypub-objects`: Isolation when purging and moderating shared objects.

## Impact

This affects API handlers, database queries and migrations, OAuth login, moderation and federation tasks, caches, and tests. Frontends must handle rejected cross-site IDs. Operators must review the migration of existing global configuration.
