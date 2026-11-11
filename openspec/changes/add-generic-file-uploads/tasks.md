## 1. Prerequisite, configuration, and bucket

- [ ] 1.1 Confirm `migrate-media-storage-to-s3` is applied, or add the RustFS service and S3 settings it defines; verify `docker compose -f docker/docker-compose.yml config` includes the shared S3 endpoint and credentials.
- [ ] 1.2 Add an optional installation-level `s3` settings block with endpoint, bucket, region, access key, secret key, and path-style fields, plus `LEMMY_S3_*` environment overrides; verify settings parsing tests cover a complete block, a partial block, and no block.
- [ ] 1.3 Extend the bucket initializer to create the document bucket idempotently alongside the pict-rs bucket; verify a first run creates both and a second run exits successfully without deleting either.
- [ ] 1.4 Add a deployment template or `.env.example` for the document bucket and credentials; verify no real secret is committed and the documented defaults match the compose file.

## 2. Database model

- [ ] 2.1 Add a migration for `file_object`, `file_reference`, `local_site.file_upload_disabled`, and `local_site.file_max_upload_size_bytes`; verify the migration applies and rolls back against the test database.
- [ ] 2.2 Add Diesel schema entries, models, insert/update forms, and CRUD helpers for both tables; verify focused database tests cover create, read by public ID, list by site and owner, and reference counting.
- [ ] 2.3 Add indexes for `content_hash`, `file_reference.file_id`, and `(local_site_id, owner_person_id, created_at)`; verify the migration includes them and the query plans use them.
- [ ] 2.4 Expose the file upload policy fields in the site settings API and restrict changes to site administrators; verify an administrator can disable uploads and change the size limit while a non-admin receives 403.

## 3. S3 client

- [ ] 3.1 Add a file-storage module or crate with the pinned `aws-sdk-s3` dependency and rustls/Tokio features; verify the workspace builds without adding the SDK to unrelated crates.
- [ ] 3.2 Build the S3 client from explicit configuration without `aws-config`, including endpoint, region, static credentials, path-style, timeouts, and retries; verify a `PutObject`, `HeadObject`, `GetObject`, and `DeleteObject` round trip against RustFS.
- [ ] 3.3 Add storage error mapping for unavailable, unauthorized, missing object, and timeout cases; verify each maps to a stable API error and never falls back to local disk.

## 4. Upload API

- [ ] 4.1 Add `POST /api/v4/file` with authentication, site resolution, and the per-site upload-enabled check; verify unauthenticated requests return 401 and disabled sites return the documented error.
- [ ] 4.2 Stream the multipart body to a temp file while enforcing `file_max_upload_size_bytes` and computing SHA-256; verify an oversize upload returns 413 with no object or reference.
- [ ] 4.3 Detect the content type from bytes and reject image and video types with an error that points to the pict-rs endpoints; verify a PNG renamed to `.pdf` is rejected.
- [ ] 4.4 Reuse an existing `file_object` by hash, verify it with `HeadObject`, repair a missing object, and insert the new UUID reference; verify identical bytes produce one object and two references.
- [ ] 4.5 Return `public_id`, site-scoped URL, filename, content type, and size; verify the URL uses the tenant hostname and contains neither the S3 endpoint nor credentials.
- [ ] 4.6 Apply a rate limit to document uploads and verify a burst is rejected without creating objects or references.
- [ ] 4.7 Add TypeScript type exports for the upload response, download/list response, and error variants; verify generated bindings compile with the existing API types.

## 5. Download, listing, and deletion

- [ ] 5.1 Add `GET /api/v4/file/{public_id}/{filename}` with tenant reference lookup and private-site authentication; verify a reference from site A returns 404 through site B and anonymous access to a private site returns 401.
- [ ] 5.2 Stream the S3 object with `Content-Disposition: attachment`, `X-Content-Type-Options: nosniff`, `Content-Length`, and an RFC 5987 filename; verify HTML bytes and CR/LF filenames cannot produce inline content or header injection.
- [ ] 5.3 Add `DELETE /api/v4/file/{public_id}` for the owner or a site administrator; verify another user gets 403, a shared object survives one site's deletion, and the last reference removes the S3 object and object row.
- [ ] 5.4 Add `GET /api/v4/file/list`; verify it returns only the authenticated user's references for the current site and never another site's files.

## 6. Tests

- [ ] 6.1 Add focused unit tests for hashing, content-type detection, filename sanitization, and reference-count transition logic; verify all pass with the workspace test command.
- [ ] 6.2 Add a RustFS-backed integration test for upload, download, deduplication, deletion, cross-site isolation, private-site access, and oversize rejection; verify it passes against the compose stack.
- [ ] 6.3 Add failure tests for S3 unavailable and missing S3 configuration; verify uploads fail without a local fallback and the server still starts with image APIs available.

## 7. Documentation and validation

- [ ] 7.1 Document the API shape, request limits, content-type rules, site visibility, bucket isolation, and the fact that frontend controls are not part of this change; verify examples contain no real credentials.
- [ ] 7.2 Run `openspec validate add-generic-file-uploads --strict`, `docker compose -f docker/docker-compose.yml config`, and the focused Rust tests; fix any validation, compose, or test failure.
- [ ] 7.3 Update deployment docs with the document bucket, environment variables, private-site behavior, and the backup requirement for both the database and S3 objects.
