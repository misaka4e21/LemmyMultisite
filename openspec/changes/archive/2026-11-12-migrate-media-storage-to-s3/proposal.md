## Why

The Docker deployment still stores pict-rs blobs in `./volumes/pictrs`, so uploaded media is tied to one host and one disk. The installation-level design already puts object storage in installation configuration, but there is no S3-compatible backend or migration path. Moving pict-rs blobs to S3-compatible storage lets a deployment share or replicate media, use RustFS locally, and point production at any compatible service.

## What Changes

- Add a RustFS service to `docker/docker-compose.yml` as the default local S3-compatible backend, including bucket initialization and health checks.
- Configure pict-rs to store blobs in S3-compatible object storage using endpoint, bucket, region, access key, secret key, and path-style settings. No RustFS-specific code may be required.
- Keep pict-rs sled metadata on a persistent local volume; move only blob objects to S3.
- Add a one-time `pict-rs migrate-store` procedure for existing filesystem data, with backup and rollback steps.
- Order startup as RustFS, bucket initialization, pict-rs, then Lemmy.
- Document environment overrides for any S3-compatible provider, including providers that require path-style addressing.
- Keep Lemmy's existing image APIs and per-site `/api/v4/image/{alias}` URLs unchanged.

Out of scope: generic document upload. That is planned separately as `add-generic-file-uploads`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `multisite/media-storage`: add S3-compatible object storage, bucket and credential configuration, migration, and startup/health requirements.

## Impact

- `docker/docker-compose.yml`: new RustFS service, bucket initializer, env wiring, health checks, and dependency ordering.
- `docker/lemmy.hjson` and deployment docs: document the pict-rs object-storage settings and provider overrides.
- `docker/volumes/pictrs`: remains as pict-rs sled metadata storage; blob data moves to the bucket.
- Operations: new credentials and bucket; migration requires downtime and a backup of the existing sled repo and files.
- No Lemmy API or database schema change is expected for image upload, read, or delete.
