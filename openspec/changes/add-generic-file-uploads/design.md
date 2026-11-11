## Context

See `proposal.md` for motivation. Images and videos already have a complete path through pict-rs, `media_object`, `media_reference`, and `/api/v4/image/{alias}`. `media_object` is image-shaped: it requires width, height, a pict-rs alias, and an image content type. Reusing it for arbitrary documents would complicate every image query and deletion path.

This change adds a parallel document path. It depends on `migrate-media-storage-to-s3` for RustFS and the S3-compatible deployment plumbing, then adds a Lemmy-owned S3 client, two document tables, and a new API surface.

## Goals / Non-Goals

**Goals:**

- Upload arbitrary non-image, non-video documents through a stable Lemmy API.
- Store bytes once per content hash in an S3-compatible bucket and reference them per site.
- Serve downloads through Lemmy with tenant isolation, safe headers, and predictable filenames.
- Keep S3 configuration provider-neutral and independent from the pict-rs bucket.
- Support deletion by owner or site administrator, with reference counting.

**Non-Goals:**

- Frontend upload controls. The API is usable by clients, but the Elm UI change is separate.
- Per-file ACLs or expiring share links. Site visibility and private-site policy apply to all files.
- Virus scanning, content moderation, or document previews.
- Image or video processing. Those types are rejected here and continue through pict-rs.
- ActivityPub attachment federation. Public file URLs can be linked in posts, but no new AP object is defined.

## Decisions

### Separate document storage from pict-rs

Keep pict-rs for images and videos. Add a Lemmy-owned document store with its own bucket, tables, and endpoints. This avoids forcing width, height, blurhash, and pict-rs alias semantics onto PDFs and archives, and it keeps image deletion and proxy behavior untouched.

Alternative considered: generalize `media_object` and route documents through pict-rs. Rejected because pict-rs is an image processor, its metadata model is image-shaped, and the change would touch every image query.

### Use a dedicated bucket

Use a document bucket separate from the pict-rs bucket. The pict-rs object layout is owned by pict-rs and may change between versions. A separate bucket gives document storage its own lifecycle, credentials, and backup policy. RustFS can host both buckets; the bucket initializer creates both idempotently.

### Use `aws-sdk-s3` with explicit configuration

Add `aws-sdk-s3` with rustls and Tokio support. Build the S3 client directly from configuration instead of using `aws-config`, so there is no IMDS lookup, no ambient credential chain, and no accidental AWS-only behavior. Configure endpoint, bucket, region, access key, secret key, and path-style explicitly.

Alternative considered: `object_store`. It is smaller, but the AWS SDK gives predictable SigV4 behavior, streaming bodies, multipart support, and a clear path for future presigned URLs. The compile-time cost is accepted for a server component.

### Configuration surface

Add an optional `s3` block to installation settings:

```hjson
s3: {
  endpoint: "http://rustfs:9000"
  bucket: "lemmy-files"
  region: "us-east-1"
  access_key: "..."
  secret_key: "..."
  use_path_style: true
}
```

Support `LEMMY_S3_ENDPOINT`, `LEMMY_S3_BUCKET`, `LEMMY_S3_REGION`, `LEMMY_S3_ACCESS_KEY`, `LEMMY_S3_SECRET_KEY`, and `LEMMY_S3_USE_PATH_STYLE` so the Docker deployment can keep secrets out of the mounted hjson. If the block is absent and no environment values are set, document uploads are disabled and the server still starts.

### Database model

Add two tables:

```text
file_object
  id, content_hash (unique), storage_key (unique), size_bytes,
  content_type, created_at

file_reference
  id, public_id (uuid, unique), local_site_id, file_id,
  owner_person_id, original_filename, created_at
```

Add `file_upload_disabled` and `file_max_upload_size_bytes` to `local_site`, with a default limit of 25 MiB. `file_reference` is the site-scoped unit; `file_object` is installation-wide. Index `(local_site_id, owner_person_id, created_at)`, `file_id`, and `content_hash`.

A UUID public identifier avoids exposing sequential row counts and gives each site reference a stable, unguessable URL component. `original_filename` lives on the reference because identical bytes may be uploaded under different names.

### Upload flow

Stream the multipart body to a temporary file under a configured temp directory while enforcing the site limit and computing SHA-256. Sniff the content type from the bytes, using the filename extension only as a fallback. Reject detected image and video types before any storage write.

Look up `content_hash` in `file_object`. If the object row exists, reuse it after checking the S3 object with `HeadObject`; repair a missing object by uploading again. If it does not exist, upload the temp file with `PutObject` using key `files/{sha256}` and insert the object row. Then insert a `file_reference` with a new UUID. Delete the temp file in all paths.

Alternative considered: buffer the whole file in memory. Rejected because a 25 MiB limit times concurrent uploads is an avoidable memory spike. Temp-file streaming bounds memory and gives a stable hash before the S3 key is chosen.

### Download flow

Resolve the tenant, read the reference by public UUID, and require `file_reference.local_site_id == tenant.local_site.id`. Apply the site's private-instance policy exactly as the image endpoint does. Call `GetObject` and stream the body to the client.

Set `Content-Disposition: attachment` for every document, `X-Content-Type-Options: nosniff`, `Content-Length`, and an RFC 5987 encoded filename. Use the stored content type when it is a known non-active type; otherwise send `application/octet-stream`. Never render HTML, SVG, JavaScript, or XML inline.

### API surface

```text
POST   /api/v4/file                  multipart field: file
GET    /api/v4/file/{public_id}/{filename}
DELETE /api/v4/file/{public_id}
GET    /api/v4/file/list
```

Upload returns `public_id`, `url`, `filename`, `content_type`, and `size_bytes`. The URL uses the current site hostname, not the S3 endpoint. Listing returns only the authenticated user's references on the current site. Deletion is limited to the owner or a site administrator.

### Deletion and consistency

Lock the `file_object` row while checking references. Delete the S3 object before deleting the last reference and object row so a failed S3 delete leaves a retryable reference. If other references remain, delete only the current reference. A background orphan cleanup job is out of scope; a failed upload that leaves an object without a row can be removed manually.

## Risks / Trade-offs

- [S3 provider differs in path-style, region, checksum, or multipart behavior] -> Use explicit path-style configuration, test against RustFS, and document provider overrides.
- [Large uploads exhaust disk or memory] -> Stream to a temp file, enforce the site limit before S3, and clean temp files in all paths.
- [Stored HTML or SVG creates stored XSS] -> Always use attachment disposition and nosniff; never serve active content inline.
- [DB and S3 diverge after a crash] -> Upload before the DB insert, verify with `HeadObject` on reuse, and document orphan cleanup.
- [Sequential IDs leak file counts] -> Use UUID public identifiers for URLs.
- [Private documents leak through a public URL] -> Limit this version to site-scoped visibility; per-file ACLs and expiring links are explicitly out of scope.
- [AWS SDK increases build time] -> Add it only to the file-storage crate or route crate, not to every workspace crate.
- [No UI means the feature is API-only] -> Document the API and leave the Elm/UI work to a follow-up change.

## Migration Plan

1. Apply `migrate-media-storage-to-s3` first, or otherwise provide the RustFS service and S3 configuration it defines.
2. Add the `lemmy-files` bucket to the existing bucket initializer and configure a separate document bucket.
3. Run the database migration to add `file_object`, `file_reference`, and the `local_site` policy fields.
4. Deploy with S3 document configuration. If the configuration is absent, uploads stay disabled and the server starts normally.
5. Verify upload, download, deduplication, cross-site isolation, private-site access, size limits, and deletion against RustFS.
6. Rollback by disabling file uploads and removing the new routes; existing documents remain in S3 and the database, and image storage is unaffected.

## Open Questions

None at planning time. The exact `aws-sdk-s3` version, temp-directory setting, and default file-size limit should be pinned during implementation after checking the workspace's Rust version and deployment defaults.
