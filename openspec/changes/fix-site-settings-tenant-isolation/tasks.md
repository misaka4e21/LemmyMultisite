## 1. Scope database writes

- [ ] 1.1 Require a local site ID for rate limit updates and filter by its primary key; verify an update to site A leaves site B's row unchanged.
- [ ] 1.2 Include import-user-settings fields in the empty-form check; verify each field can be updated without an `updated_at` value.
- [ ] 1.3 Require a local site ID for both existing-account bulk updates and filter by account ownership; verify only the selected site's accounts change.

## 2. Connect site handlers

- [ ] 2.1 Pass the request tenant ID to rate limit updates in site setup and editing; verify both handlers compile and update the selected site.
- [ ] 2.2 Propagate rate limit update failures from editing and refresh runtime configuration only after persistence; verify a failed write returns an error.
- [ ] 2.3 Pass the request tenant ID to account exemption updates and preserve policy transition checks; verify unrelated edits and unchanged policies do not grant exemptions.

## 3. Verify tenant isolation

- [ ] 3.1 Test site setup and editing with two sites, checking persisted limits, responses, caches, and runtime behavior for both sites.
- [ ] 3.2 Test each policy transition with accounts on two sites, including existing and newly registered accounts; verify only the intended existing accounts receive exemptions.
- [ ] 3.3 Run the relevant database and API tests and record results, including any environment limitations.
