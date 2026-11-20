# Proposal

## Why

Site administrators need to find accounts by a username or email fragment and inspect an account from the current site's user list. The API must keep these reads inside the requesting tenant and preserve the existing admin permission boundary.

## What Changes

- Add case-insensitive partial matching on username and email to the admin user list query.
- Preserve tenant filtering, banned-user filtering, and cursor pagination when search is used.
- Provide an admin user detail read scoped to the current site, including the target account's administrator status.
- Add coverage for search behavior, tenant isolation, pagination, admin authorization, and cross-tenant detail rejection.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `multisite/site-administration`: Specify tenant-scoped admin user search and detail reads, including authorization and cross-tenant behavior.

## Impact

Impacts the admin user list API and local-user database view, the admin user detail route and its site-scoped lookup, and their API/database tests. No schema migration is expected.
