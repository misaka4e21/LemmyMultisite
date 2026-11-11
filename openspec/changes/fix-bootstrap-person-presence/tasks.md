## 1. Shared local-person helper

- [ ] 1.1 Add a helper that sets a Person's NULL `origin_local_site_id` and inserts the matching `person_site` row with `ON CONFLICT DO NOTHING`. Verify the helper's unit test covers a NULL origin, an existing origin, and a repeated call.
- [ ] 1.2 Keep remote federation on the presence-only path so remote People retain a NULL origin. Verify an existing federation or person test still passes and add an assertion if none covers the origin.

## 2. Bootstrap call sites

- [ ] 2.1 In `setup_local_site`, run the helper for the admin and system Person rows on the transaction connection after the LocalSite is created. Verify the fresh-setup regression test from task 4.1 passes.
- [ ] 2.2 In `provision_site`, run the helper for the system account and keep the admin's existing origin and presence path. Verify the provision test asserts origin and presence for both accounts.

## 3. Existing data repair

- [ ] 3.1 Add a data migration that fills NULL origins for local People through `person.instance_id = site.instance_id` and inserts missing `person_site` rows. Use the lowest LocalSite id when a Site has more than one row, and add a no-op down migration. Verify the migration test seeds a broken local Person, runs the repair, and reruns it without duplicates.
- [ ] 3.2 Run the migration harness against the test database and confirm it completes without a schema diff. Verify with `cargo test -p lemmy_diesel_utils` and the project's normal migration command.

## 4. Regression and integration checks

- [ ] 4.1 Add a first-site setup test on an empty test database. Assert that the admin and system Person rows have the new LocalSite as origin, both have `person_site` rows, `GetSite.admins` includes the admin, and `GET /api/v4/person?username=<admin>` returns 200. Confirm the test fails on the current code before applying the fix.
- [ ] 4.2 Extend the provision test to assert the system account's origin and `person_site` row, and keep the admin assertions. Verify the test passes.
- [ ] 4.3 Run formatting and the focused suites: `cargo +nightly fmt --all --check`, then `cargo test -p lemmy_routes -p lemmy_db_views_person -p lemmy_db_schema -p lemmy_diesel_utils`. Record the exact result.
- [ ] 4.4 Start a fresh server with `LEMMY_TEST_KEEP_TMP=1 ./start-for-dev.sh` and verify `curl -H 'Host: localhost' 'http://127.0.0.1:8536/api/v4/person?username=admin'` returns 200. Also verify `/api/v4/site` lists the admin.
- [ ] 4.5 Run `openspec validate fix-bootstrap-person-presence --strict` and confirm the change has no artifact or scenario errors.
