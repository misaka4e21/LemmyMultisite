# Reviewing legacy report ownership

The report-site migration assigns a local reporter's site when the stored resolver is absent or belongs to that same site. Reports with a remote reporter or a resolver from another site remain unassigned. Their original records are retained, but they do not appear in site-scoped report lists until an operator assigns them.

Run `psql -v ON_ERROR_STOP=1 -f scripts/audit-report-site-migration.sql` against the migrated database to list unassigned report IDs. Inspect the relevant activity delivery and moderation records before deciding which sites received each report. Object presence alone does not establish report access.

After review, insert one `report_site` row for each confirmed receiving site in a transaction. Set `resolved`, `resolver_id`, `conclusion`, and `updated_at` only when the reviewed evidence establishes that site's state. The default is unresolved. A report may have more than one receiving site, and each row has an independent state. Do not assign a report to an unrelated site merely to empty the audit list.

```sql
BEGIN;
INSERT INTO report_site (local_site_id, report_combined_id)
VALUES (123, 456);
COMMIT;
```

Replace both IDs with reviewed values. Repeat the audit after assignments. Keep the database backup until all intended access has been verified. Downgrading drops the site-specific report states, so restore from a backup if those states must be recovered after rollback.
