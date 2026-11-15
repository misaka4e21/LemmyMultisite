## 1. Registration decisions

- [x] 1.1 Bind the request tenant to the administrator and lock the site-owned application inside the decision transaction; verify cross-site IDs and mismatched request sites fail before writes.
- [x] 1.2 Scope both updates to the verified site and send email after commit; test approval, denial, rollback, and absence of email on rejection or failure.

## 2. Media references

- [x] 2.4 Inventory persisted content and site presence paths that can use local image URLs; document URL forms and verify foreign URLs cannot be mistaken for local aliases.
- [x] 2.5 Track content-specific media references on create, edit, deletion, federation ingestion, and site presence changes; test shared content, removal, and concurrent updates.
- [x] 2.6 Backfill existing content references with an idempotent migration or resumable job, then queue eligible orphans; test multi-site historical data, ambiguous uses, restart, and block final deletion until backfill completes.
- [x] 2.1 Add a site-scoped administrator deletion operation; test own-site, foreign-only, and shared aliases with two sites.
- [x] 2.2 Serialize reference creation and final alias deletion, including durable retry state and any needed migration; test concurrent upload/delete, storage failure, restart recovery, and migration rollback.
- [x] 2.3 Audit upload, user, profile, community, and other low-level alias deletion callers; test that each applicable path preserves another site's references and media.

## 3. Permanent deletion

- [x] 3.1 Inventory foreign keys, presence rows, media references, and federation effects of person, community, post, and comment purge; record the protected dependency checks needed for each path.
- [x] 3.2 Protect dependencies and serialize validation with deletion for community, post, and comment purge; test foreign and shared descendants, presence, media, and concurrent dependency creation.
- [x] 3.3 Pass site ownership into person purge cleanup and scope or reject global mutations; test cross-site content and moderator relationships after the background task completes.
- [x] 3.4 Ensure rejected purges create no modlog, activity, or background task; test foreign targets and protected dependencies as well as successful same-site purges.
- [x] 3.5 Audit other callers of account cleanup and activity generation; verify their existing authorized deletion behavior with focused regression tests.

## 4. Validation

- [x] 4.1 Run relevant database and API tests, including two-site and concurrent scenarios; record commands and results.
- [x] 4.2 Run formatting and applicable Rust checks, then validate the OpenSpec change in strict mode; record any environmental limitations.
