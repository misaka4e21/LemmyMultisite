## Context

See `proposal.md` for motivation. The current stack runs `asonix/pictrs:0.5` with a local volume at `/mnt`; Lemmy only knows the pict-rs URL and API key. pict-rs 0.5 supports an S3-compatible object-storage store and ships a `migrate-store` command that moves filesystem blobs into a bucket while keeping the existing sled repository.

The deployment has no S3 service today. RustFS will be the default Docker backend, but the configuration must remain provider-neutral so an operator can point the same stack at AWS S3, MinIO, Ceph, or another compatible service.

## Goals / Non-Goals

**Goals:**

- Store pict-rs blob objects in an S3-compatible bucket while keeping alias metadata in the existing pict-rs sled repository.
- Make the endpoint, bucket, region, credentials, and path-style mode environment-driven.
- Provide RustFS in `docker/docker-compose.yml` with a persistent data volume and an idempotent bucket initialization step.
- Provide an offline migration and rollback procedure for the existing `docker/volumes/pictrs` data.
- Keep Lemmy's image API, media deduplication, and per-site media URLs unchanged.

**Non-Goals:**

- Generic document or attachment upload. That is planned separately as `add-generic-file-uploads`.
- Per-site buckets or per-site S3 credentials. One installation-level bucket is shared by all sites.
- Public bucket access or direct browser-to-S3 uploads. Lemmy continues to proxy image reads.
- Automatic online migration while pict-rs is serving uploads.

## Decisions

### Keep pict-rs and change its store

Use pict-rs's `object_storage` store type instead of adding an S3 client to Lemmy. pict-rs already owns image decoding, resizing, deduplication, alias deletion, and video handling. Replacing it would require a new Rust upload API, new database rows, and a migration of existing aliases. This change only moves the blob layer.

Alternative considered: add S3 storage to Lemmy and bypass pict-rs. Rejected because it expands the change into a new media service and does not help the immediate storage goal.

### Keep S3 configuration outside Lemmy's hjson

The S3 settings belong to pict-rs. Lemmy keeps its existing `pictrs.url` and `pictrs.api_key` block, so no Lemmy settings struct or API change is needed. Operators configure pict-rs through environment variables, using the same values in the bucket initializer and the migration command.

Configuration surface:

```text
PICTRS__STORE__TYPE=object_storage
PICTRS__STORE__ENDPOINT=http://rustfs:9000
PICTRS__STORE__BUCKET_NAME=lemmy-media
PICTRS__STORE__REGION=us-east-1
PICTRS__STORE__ACCESS_KEY=...
PICTRS__STORE__SECRET_KEY=...
PICTRS__STORE__USE_PATH_STYLE=true
```

The compose file should source these from `${S3_ENDPOINT}`, `${S3_BUCKET}`, `${S3_REGION}`, `${S3_ACCESS_KEY}`, `${S3_SECRET_KEY}`, and `${S3_USE_PATH_STYLE}` so the same stack can point at another provider. Secrets belong in an untracked `.env` or the deployment's secret manager, not in committed YAML.

### RustFS as the default local backend

Add a `rustfs` service using a pinned `rustfs/rustfs` image with a persistent `./volumes/rustfs:/data` volume. Bind the S3 API on 9000 and the console on 9001 to localhost for the development stack; containers reach the API through the compose network. Configure `RUSTFS_ACCESS_KEY` and `RUSTFS_SECRET_KEY` from the environment.

Bucket creation is a separate one-shot `rustfs-init` service using the AWS CLI. It retries until it can create the bucket or confirm that the bucket already exists. pict-rs then depends on `rustfs-init` completing successfully, which gives a deterministic startup order without relying on a RustFS-specific initialization feature.

Alternative considered: let pict-rs create the bucket. Rejected because pict-rs expects the bucket to exist and surfaces a less useful error when it does not.

### Keep sled on a persistent volume

The object-storage store still needs the sled repository for aliases and metadata. Keep `./volumes/pictrs:/mnt` mounted. Only `/mnt/files` moves to S3. Losing the volume while keeping the bucket would leave orphaned objects and broken aliases, so the volume remains part of the backup set.

### Private bucket, Lemmy-proxied reads

The bucket should not be public. Browsers continue to request `/api/v4/image/{alias}` from Lemmy, and Lemmy fetches the object through pict-rs. This preserves per-site access checks and avoids exposing bucket credentials or requiring CORS on the S3 service.

## Risks / Trade-offs

- [RustFS or another provider differs from AWS in path-style, region, checksum, or multipart behavior] -> Pin the RustFS image, use path-style mode for custom endpoints, and validate upload, read, and delete against the actual service.
- [Migration changes aliases or leaves objects behind] -> Stop pict-rs, back up the sled repository and files, migrate offline, and verify both an old alias and a new upload before removing the backup.
- [The sled volume is lost while the bucket survives] -> Treat the volume as part of the media backup set and document that the bucket alone is not a complete restore.
- [S3 credentials leak through compose or logs] -> Keep secrets in `.env` or a secret manager, avoid printing them in health checks, and never return them in API responses.
- [S3 becomes unavailable at runtime] -> pict-rs reports unhealthy and uploads fail; no local blob fallback is introduced, so operators see the storage failure directly.
- [The existing pict-rs 0.5 image is old] -> Keep the pinned image for this change and validate the migration command against that version before switching the compose default.

## Migration Plan

1. Stop Lemmy and pict-rs, then back up `docker/volumes/pictrs` to a timestamped directory outside the compose volume.
2. Start RustFS, wait for it to accept S3 requests, and create the media bucket with the one-shot initializer.
3. Run the version's store migration while pict-rs is offline. The documented shape is:

   ```bash
   pict-rs migrate-store --concurrency 32 \
     filesystem -p /mnt/files \
     object-storage -e "$S3_ENDPOINT" -b "$S3_BUCKET" -r "$S3_REGION" \
       -a "$S3_ACCESS_KEY" -s "$S3_SECRET_KEY" \
     sled -p /mnt/sled
   ```

   Check `pict-rs migrate-store --help` for the exact path-style flag in the pinned image.
4. Start pict-rs with the object-storage configuration, then start Lemmy.
5. Verify an existing avatar or image alias still resolves, upload a new image, and delete it. Confirm the bucket contains the new object and that the sled volume contains the alias metadata.
6. If migration fails, stop pict-rs, restore the backed-up volume, remove the object-storage environment, and start the old filesystem configuration. Keep the bucket for a later retry.

## Open Questions

None at planning time. The exact RustFS image tag, health endpoint, and AWS CLI image tag should be pinned during implementation after checking the selected versions.
