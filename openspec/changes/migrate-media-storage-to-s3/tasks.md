## 1. Configuration surface

- [x] 1.1 Add `S3_ENDPOINT`, `S3_BUCKET`, `S3_REGION`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, and `S3_USE_PATH_STYLE` to the Docker environment with RustFS defaults, then verify `docker compose -f docker/docker-compose.yml config` renders the expected values.
- [x] 1.2 Add an untracked `.env.example` or equivalent deployment template with placeholder credentials and provider overrides, then verify no real secret is added to tracked files.
- [x] 1.3 Leave `docker/lemmy.hjson`'s `pictrs.url` and `pictrs.api_key` unchanged and verify no S3 credential or endpoint is added to Lemmy's hjson.

## 2. RustFS and bucket initialization

- [x] 2.1 Add a pinned `rustfs` service with `./volumes/rustfs:/data`, the S3 API on localhost port 9000, the console on localhost port 9001, and `RUSTFS_ACCESS_KEY`/`RUSTFS_SECRET_KEY` from the environment; verify the container starts and the S3 API answers `ListBuckets` with the configured credentials.
- [x] 2.2 Add a one-shot bucket initializer with a pinned AWS CLI image that creates `S3_BUCKET` idempotently and retries until the S3 endpoint is reachable; verify a first run creates the bucket and a second run exits successfully without recreating or deleting it.
- [x] 2.3 Make `pictrs` depend on the initializer completing and expose a health check that fails when pict-rs cannot reach its store; verify `docker compose up` starts pict-rs only after the bucket is ready.
- [ ] 2.4 Update `docker/docker_update.sh` to create the RustFS data volume and set the ownership or permissions required by the pinned image; verify a fresh checkout starts successfully through that script.

## 3. pict-rs object storage

- [x] 3.1 Configure pict-rs with `PICTRS__STORE__TYPE=object_storage` and the endpoint, bucket, region, access key, secret key, and path-style variables from the shared S3 environment; verify a new upload writes an object to the bucket and the image remains readable through Lemmy.
- [x] 3.2 Keep `./volumes/pictrs:/mnt` mounted for the sled repository and verify a pict-rs container restart preserves existing aliases and image reads.
- [x] 3.3 Stop the S3 service and verify an upload fails without creating a local blob fallback, then restore S3 and verify uploads work again.
- [x] 3.4 Verify path-style mode against RustFS and document the override needed for a provider that uses virtual-host addressing; record which mode each tested provider requires.

## 4. Existing media migration

- [x] 4.1 Document the offline migration: stop Lemmy and pict-rs, back up `docker/volumes/pictrs`, create the bucket, and keep the backup until verification completes.
- [x] 4.2 Seed a temporary pict-rs filesystem store with at least one image and alias, run the pinned image's `migrate-store` command against RustFS, and verify the old alias resolves after switching to object storage.
- [x] 4.3 Verify rollback by restoring the backed-up volume and starting the filesystem configuration, then confirm the old alias serves media again.
- [x] 4.4 After migration, upload a new image, replace an avatar, and delete an image; verify the bucket, sled metadata, and Lemmy database stay consistent.

## 5. Validation and documentation

- [x] 5.1 Run an integration check with two sites sharing one bucket: verify the same alias serves through each site's hostname, a private site returns 401 for anonymous image access, and S3 credentials never appear in API responses.
- [x] 5.2 Run `openspec validate migrate-media-storage-to-s3 --strict` and `docker compose -f docker/docker-compose.yml config`; fix any validation or compose errors.
- [x] 5.3 Document the provider-neutral settings, the RustFS development defaults, the bucket privacy requirement, and the fact that sled metadata remains part of the backup set; verify the docs contain no real credentials.
