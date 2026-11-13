## Why

Site setup and editing can overwrite rate limits belonging to other sites. Enabling email verification or registration applications can also change every site's existing accounts, bypassing other tenants' policies.

## What Changes

- Scope rate limit updates in both site setup and site editing to the request's local site, and report failed updates.
- Scope the existing-user exemptions applied when enabling email verification or registration applications to accounts owned by that site.
- Verify database state, API responses, and runtime rate limit configuration across two sites.
- Keep historical data recovery outside this change; previous values cannot be inferred reliably from the affected rows.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `multisite/site-routing`: Specify tenant isolation for rate limit changes during site setup and editing.
- `multisite/site-administration`: Specify the scope of existing-account updates when registration policies change.

## Impact

The change affects site setup and edit handlers, rate limit and local user database methods, and tenant isolation tests. It keeps the HTTP request formats and database schema unchanged.
