## Context

The request Host and `TenantContext` select the site, and JWTs carry a site ID. Local accounts and objects already have site ownership; remote objects can be shared by several sites. Some older handlers still use an `admin` flag and global IDs to access the database.

## Goals / Non-Goals

**Goals:**

- Check the actor, target, action, and current site before any protected operation causes side effects.
- Make audit coverage and findings traceable, and verify fixes with cross-site regression tests.

**Non-Goals:**

- Redesigning federation protocols or giving installation administrators a site-user identity.
- Claiming that one audit proves the absence of all unknown security defects. Confirmed findings from the audit are in scope.

## Decisions

### Authorization boundaries

Derive the site from server-side request context, never from the request body. Add action-specific checks for site accounts, communities, reports, and other objects; compare administrator hierarchy only within the same site. Database reads, updates, and deletes must constrain both resource ID and site ownership to avoid a check-then-write race. Use existing forbidden or not-found semantics without exposing sensitive target data.

The route-level `is_admin` check cannot establish target ownership. Shared authorization helpers therefore need the current site and target, and callers must also scope their queries. An explicitly granted community moderator role remains valid for that community; administrator status on another site does not grant it.

### Local and shared objects

Site administrators may permanently purge only objects owned by their current site. Purge operations must protect shared media and data still referenced by another site. For remote objects and objects owned by another site, a site may change only its own visibility, ban, or moderation state. Federation actions that change an origin object require the corresponding origin authority. Scope report-related bulk operations to the authorized site, and pass and recheck site context in background tasks.

### Site configuration and OAuth

Add non-null site ownership, site-scoped uniqueness constraints, and suitable indexes for OAuth providers, custom emoji, and taglines. Scope all CRUD operations, listings, site responses, random tagline selection, and OAuth login to the current site. Account linking and callbacks must verify provider ownership. Invalidate the affected site's response cache after configuration changes. IDs from another site must not allow reads, writes, or login.

### Existing data migration

On a single-site installation, assign existing global configuration to the only site. On a multi-site installation, ownership cannot be inferred reliably from the old tables. Generate an inventory and require the operator to map every record to one site before migration. Abort without changing data if a mapping is missing or invalid. Do not automatically copy OAuth credentials to other sites. Preserve existing IDs and OAuth account associations, and verify that each linked user and provider belong to the same site. Abort and report conflicts for resolution before retrying.

Explicit mapping prevents credentials or configuration from being assigned to the wrong site. Validate migration integrity transactionally and back up before rollback. Rollback removes the new isolation and must not run while a multi-site installation accepts writes.

### Audit and verification

Inventory registered routes, background tasks, and federation entry points. Record the actor, target, action, site, permission conditions, rejection behavior, and side effects for anonymous users, ordinary users, community moderators, site administrators, and installation administrators, including v3 forwarding paths. Trace queries, caches, and jobs individually; a missing explicit tenant parameter alone is not proof of a vulnerability. Fix confirmed findings and add regression tests before marking their audit items complete.

## Risks / Trade-offs

- [Stricter permissions affect existing administration] Test authorized same-site and rejected cross-site operations and document the behavior change.
- [Purging shared objects damages another site's data] Check origin, site references, and media references; verify database and queue state around rejected actions.
- [Legacy ownership is ambiguous] Require explicit mapping and preflight checks on multi-site installations; leave data unchanged on failure.
- [ID-based reads or background jobs remain vulnerable after list filtering] Include every entry point in the inventory and test direct ID requests, failed requests, queued work, and caches.

## Migration Plan

1. Back up the database, pause relevant writes, and export legacy configuration and OAuth associations. Provide a complete mapping and run preflight checks for multi-site installations.
2. Add ownership, backfill records, and enforce constraints in a transaction. Verify associations, deploy the new code, and invalidate site caches.
3. Verify login and configuration for each site, cross-site rejection, and shared objects. On failure, stop the service and restore the backup. Run a downgrade migration only in a restored single-site or isolated environment.
