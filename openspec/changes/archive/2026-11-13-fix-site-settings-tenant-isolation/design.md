## Context

The rate limit table uses `local_site_id` as its primary key, and local accounts carry their owning `local_site_id`. Both site handlers receive a tenant context. The rate limit update and the two existing-account bulk updates currently omit tenant filters. The edit handler also discards rate limit update errors.

## Goals / Non-Goals

**Goals:** Keep tenant ownership explicit at the database method boundary and preserve the existing policy transition behavior for the current site's accounts.

**Non-Goals:** Recover previously overwritten data, redesign registration policy or rate limiting, or audit unrelated site administration operations.

## Decisions

1. Require `LocalSiteId` in the rate limit update method and select its row by primary key. Pass the ID from the request tenant in both setup and edit handlers. This makes an unscoped update difficult to call accidentally; filtering only at the handlers would leave the unsafe database method available.
2. Require `LocalSiteId` in both bulk account methods and filter on account ownership, not `person_site` presence. Presence can include actors associated with a site, whereas the local account's owner determines its registration policy.
3. Propagate rate limit update errors from editing. Keep the existing per-site runtime configuration refresh and cache invalidation after successful persistence. The handlers currently perform multiple writes without one encompassing transaction; an error can leave earlier writes committed. Full transactional site editing is outside this change.
4. Include both import-user-settings fields in the empty-form check. The helper must recognize every writable rate limit field, including calls that omit the timestamp.
5. Retain the transition check for existing-account exemptions. Apply it only after a successful site policy update, and leave new registrations subject to the current policy.

## Risks / Trade-offs

- Previously overwritten values and account flags cannot be reconstructed from their current values. Operators need backups or audit records for any recovery.
- An error after earlier site writes can leave a partial edit. Return the error and avoid publishing an unpersisted rate limit configuration; transactional changes can be addressed separately.
- Runtime rate limits and cached responses can hide database errors or stale values. Test both sites through their handlers and inspect persisted and runtime state.

## Migration Plan

Deploy the application change without a schema migration. Roll back by redeploying the prior version if needed; rollback restores the prior cross-tenant risk. Review backups separately when historical recovery is required.
