## 1. Registration decisions

- [ ] 1.1 Bind the request tenant to the administrator and lock the site-owned application inside the decision transaction; verify cross-site IDs and mismatched request sites fail before writes.
- [ ] 1.2 Scope both updates to the verified site and send email after commit; test approval, denial, rollback, and absence of email on rejection or failure.

## 2. Media references

- [ ] 2.1 Add a site-scoped administrator deletion operation; test own-site, foreign-only, and shared aliases with two sites.
- [ ] 2.2 Serialize reference creation and final alias deletion, including durable retry state and any needed migration; test concurrent upload/delete, storage failure, restart recovery, and migration rollback.
- [ ] 2.3 Audit upload, user, profile, community, and other low-level alias deletion callers; test that each applicable path preserves another site's references and media.

## 3. Permanent deletion

- [ ] 3.1 Inventory foreign keys, presence rows, media references, and federation effects of person, community, post, and comment purge; record the protected dependency checks needed for each path.
- [ ] 3.2 Protect dependencies and serialize validation with deletion for community, post, and comment purge; test foreign and shared descendants, presence, media, and concurrent dependency creation.
- [ ] 3.3 Pass site ownership into person purge cleanup and scope or reject global mutations; test cross-site content and moderator relationships after the background task completes.
- [ ] 3.4 Ensure rejected purges create no modlog, activity, or background task; test foreign targets and protected dependencies as well as successful same-site purges.
- [ ] 3.5 Audit other callers of account cleanup and activity generation; verify their existing authorized deletion behavior with focused regression tests.

## 4. Validation

- [ ] 4.1 Run relevant database and API tests, including two-site and concurrent scenarios; record commands and results.
- [ ] 4.2 Run formatting and applicable Rust checks, then validate the OpenSpec change in strict mode; record any environmental limitations.
