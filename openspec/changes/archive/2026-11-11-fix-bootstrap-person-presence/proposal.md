## Why

A fresh installation created by `start-for-dev.sh` returns 404 for the setup admin's `/person` request because `setup_local_site` creates the admin and system Person rows before the LocalSite row. `Person::create` cannot resolve a site at that point, so it skips `person_site`, and `PersonView::read` filters every person through that table. The same gap leaves the system account without presence on sites created by `provision_site`. Existing databases with these rows remain broken after a code fix unless the data is repaired.

## What Changes

- Add one helper for local bootstrap actors that sets `origin_local_site_id` and inserts the site's `person_site` row. Use it for the first-site admin and system account, and for the system account on provisioned sites.
- Add a data migration that fills missing `origin_local_site_id` and `person_site` rows for local Person rows that map to a local site. It is idempotent and leaves remote persons alone.
- Add regression tests for first-site setup, provisioned sites, and the admin read path: origin and presence exist, `GetSite.admins` contains the admin, and `GET /api/v4/person?username=<admin>` succeeds.
- Keep the 404 response for people with no site presence. This change only repairs bootstrap and existing local rows.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `multisite/local-identity`: the local actor ownership requirement now covers bootstrap administrators and system accounts, including their origin.
- `multisite/activitypub-objects`: the site-presence requirement now covers actors created during first-site setup and site provisioning.

## Impact

Affected code: `crates/routes/src/utils/setup_local_site.rs`, `crates/routes/src/utils/provision_site.rs`, `crates/db_schema/src/impls/presence.rs` or `person.rs`, a new data migration under `migrations/`, and tests in the routes, person view, schema, and migration crates. No API request or response types change. The frontend `/person` path keeps its current contract; the backend starts returning 200 for the setup admin once presence exists.
