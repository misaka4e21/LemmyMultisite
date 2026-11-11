## Why

Lemmy can only upload images and videos through pict-rs. Users cannot attach PDFs, office documents, archives, or other files. The `migrate-media-storage-to-s3` change gives the deployment an S3-compatible object store, but no Lemmy API writes documents to it. This change adds a generic document upload API backed by that storage.

This change depends on `migrate-media-storage-to-s3` for the RustFS service and S3 configuration. It can be implemented after that change is applied.

## What Changes

- Add `POST /api/v4/file` for authenticated multipart document uploads, with per-site size limits, SHA-256 hashing, and deduplication.
- Add `GET /api/v4/file/{public_id}/{filename}`, `DELETE /api/v4/file/{public_id}`, and `GET /api/v4/file/list` for download, deletion, and listing.
- Add `file_object` and `file_reference` tables, plus per-site file upload policy fields.
- Add a Lemmy S3 client configured with endpoint, bucket, region, access key, secret key, and path-style settings. Use a separate bucket from the pict-rs image store.
- Serve documents through Lemmy with `Content-Disposition: attachment`, `X-Content-Type-Options: nosniff`, and safe filename encoding. Keep the bucket private.
- Reject image and video content types in the document endpoint; those continue through the existing pict-rs endpoints.
- Initialize the document bucket in the Docker stack and document provider overrides.

Out of scope: frontend upload controls, per-file ACLs, virus scanning, and ActivityPub attachment federation.

## Capabilities

### New Capabilities

- `multisite/file-storage`: generic document upload, S3-compatible object storage, per-site file references, download and deletion, limits, and content safety.

### Modified Capabilities

None.

## Impact

- New database tables, indexes, and migrations for file objects and per-site references.
- New API routes and handlers for upload, download, deletion, and listing.
- New Rust dependency on an S3 client and configuration fields for installation-level storage.
- Docker Compose: second bucket, credentials, health checks, and startup ordering on top of `migrate-media-storage-to-s3`.
- Tests: upload, download, deduplication, cross-site isolation, private-site access, size limits, unsafe content types, and S3 failure behavior.
- Existing image and video APIs remain unchanged.
