# Design

## Context

The admin user list is backed by `LocalUserQuery`, which already applies `local_site_id` before listing and uses `LocalUserView::paginate` for cursor paging. Search has been added to that query in the working tree. The detail route is also present and calls `read_person_for_site`, which filters by both person ID and local site ID and returns `LocalUserView`, including `local_user.admin`.

See proposal.md for the motivation and the spec delta for the observable behavior.

## Goals / Non-Goals

**Goals:**
- Keep all user-list search results within the active tenant while preserving the existing list filters and cursor order.
- Keep detail reads behind `is_admin` and return only current-site local accounts.
- Add regression coverage at the database query and API handler boundaries.

**Non-Goals:**
- Change admin list sort options, cursor format, or response shape.
- Add an installation-wide user search or expose remote Person records as local user details.
- Add database migrations or alter account ownership.

## Decisions

- Apply username and email matching as a combined OR predicate inside `LocalUserQuery`, after the site scope is established and before pagination. This keeps tenant and search predicates in one SQL query. Filtering results after loading a page would produce incomplete pages and could leak cross-site matches.
- Normalize both the search term and database text with lowercase matching, and use a contains pattern for partial matches. Keep nullable emails as an empty string in the predicate so users without email remain searchable by username.
- Continue using the existing keyset pagination and stable published-at/person-ID ordering. Search changes the candidate set, while the cursor mechanism and deterministic ordering stay the same.
- Use the site-scoped detail lookup and return the existing `LocalUserView`. A user from another tenant and a nonexistent user both resolve to NotFound, avoiding account existence disclosure. The existing `is_admin` check remains the authorization gate and produces the standard not-admin error.
- Test the query with matching users in multiple tenants and enough results to traverse a cursor. Test the handler for admin success, non-admin rejection, and cross-tenant NotFound, asserting that the returned view exposes the target's `local_user.admin` value.

Alternatives considered: a separate search endpoint would duplicate the existing list contract; a global detail lookup followed by an ownership check would fetch an out-of-tenant account before rejecting it. Both add complexity without improving the requested behavior.

## Risks / Trade-offs

- Case-folded contains matching can require a scan as the local account count grows. Keep this query scoped to the tenant and consider an appropriate functional/trigram index only if measured performance warrants a separate change.
- SQL `LIKE` treats `%` and `_` as pattern characters. Tests should clarify escaping if literal matching for these characters is required; the current user-facing requirement does not define that edge case.
- Cursor tests can become brittle if they depend on timestamps being distinct. Use deterministic ordering and explicit test data when validating page boundaries.

## Migration Plan

No data migration is needed. Deploy the API and query changes together. Rollback consists of reverting the code and route changes; no stored data needs repair.
