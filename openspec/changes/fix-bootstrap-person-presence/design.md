## Context

See `proposal.md` for the failure and its user-visible effect. The relevant constraint is that first-site setup and site provisioning both run inside a database transaction, and they create Person rows before the LocalSite row that owns those people. `Person::create` can only write `person_site` when it receives an origin site or can find exactly one existing LocalSite. Neither condition holds during bootstrap. Registration and federation already write presence explicitly, so the gap is limited to bootstrap plus databases that were created while the gap existed.

## Goals / Non-Goals

**Goals:**

- Make the first-site administrator and system account site-local in both origin and presence.
- Make the provisioned site's system account site-local in both origin and presence.
- Repair local Person rows in existing databases without manual SQL.
- Cover the bootstrap paths with tests that fail on the current code.

**Non-Goals:**

- Change `PersonView::read` or remove its presence filter.
- Repair wrong non-null origins. The migration only fills NULL origins.
- Change person IDs assigned during first-site setup.
- Add frontend behavior or alter the `/person` API shape.

## Decisions

### Keep the existing Person creation order and repair after LocalSite creation

`setup_local_site` currently creates the admin Person, then the system Person, then the LocalSite. Moving the admin creation after the LocalSite would make the insert simpler, but it changes which Person gets the first ID and requires restructuring the deferred `admin_setup` tuple. The smaller change keeps the order and runs one repair step for both People after `LocalSite::create`.

The repair step runs on the transaction's connection. Calling a pool-based helper could take a second connection before the LocalSite row is committed, which risks a foreign-key failure or a deadlock. The helper should accept a `DbPool` created from the transaction connection, matching the existing `Person::create(&mut conn.into(), ...)` pattern.

### Use one idempotent helper for origin and presence

A new helper sets `person.origin_local_site_id` when it is NULL and inserts the matching `person_site` row with `ON CONFLICT DO NOTHING`. Both bootstrap paths end with the helper applied to the administrator and system account, either because the insert already set the values or because the repair step fills them. Registration and federation may keep their current explicit calls; the helper is safe to call there later.

Setting only presence would make the profile read work, but the Person would still fail the site-local origin check and would not satisfy the existing local-identity requirement. Setting only origin would leave the 404 in place. The helper keeps the two values together.

### Repair existing rows in a data migration

Existing installations that already ran the broken bootstrap have Person rows with NULL origins and no presence. A code-only fix would leave those installations broken until someone recreated the database. A data migration runs once during the normal upgrade, and it can be written to be idempotent.

The migration maps a local Person to its site through `person.instance_id = site.instance_id`. A local site owns a Site row whose instance is the local hostname, while remote People point at remote instances, so the join selects only local People. When a Site has more than one LocalSite row, choose the lowest LocalSite id to keep the result deterministic. The presence insert then uses each repaired or existing non-null origin.

Alternatives considered:

- Run the repair on every server start. This adds a table scan to every boot and still leaves the setup path to fix separately.
- Ask operators to run manual SQL. This would fix a test database but not other installations, and it is easy to miss.
- Add a trigger on Person insert. This would hide the missing origin from callers and make the site choice ambiguous when a Person is inserted before any LocalSite exists.

### Test the bootstrap paths, not only the helper

The current database tests create a LocalSite before their users or call `ensure_person_site` directly, so they cannot catch this regression. The new tests should start from an empty schema and run the real bootstrap code:

- First-site setup with a configured admin, then assert the admin and system Person origins, the two `person_site` rows, `GetSite.admins`, and a person read for the admin.
- Provisioned site creation, then assert the system account's origin and presence. The admin path is already covered by the existing provision test and should remain covered.

The setup test needs an empty database because `setup_local_site` does nothing when any LocalSite exists. Use the serial test suite and clean the relevant rows before the test, or reset the test database through the existing test workflow.

## Risks / Trade-offs

- The backfill could attach a local Person to the wrong site when an instance maps to an unexpected Site. The join uses the local Site's instance, and the migration skips People with no matching Site. The failure mode is a missing row, not a cross-site attachment.
- A large Person table makes the repair migration slower. The update and insert are set-based, use the existing origin and presence indexes, and run during the normal migration window.
- The migration cannot detect a wrong non-null origin. This is a narrower repair than a full reconstruction, and it avoids overwriting data that an operator may have set deliberately.
- The helper adds a write to the bootstrap transaction. The transaction already writes the Person, Site, LocalSite, and LocalUser rows, so one update and two inserts do not change the failure model.
- A test that depends on a truly empty test database can interfere with other serial tests. Cleanup must remove the bootstrap rows, and the test should not assume a fixed Person id.

## Migration Plan

1. Add the helper and call it from first-site setup and site provisioning.
2. Add the data migration. It fills NULL origins for local People and inserts missing presence rows. The down migration is a no-op because deleting repaired origins or presence could remove valid data.
3. Deploy the code and migration together. The migration is safe to rerun through the normal migration harness and uses `ON CONFLICT DO NOTHING`.
4. Roll back the code if needed. The repaired rows stay valid, and the old code can still read them. Do not run a destructive down migration.

## Open Questions

None.
