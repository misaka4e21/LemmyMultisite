# Building Lemmy Images

Lemmy's images are meant to be **built** on `linux/amd64`,
but they can be **executed** on both `linux/amd64` and `linux/arm64`.

To do so we need to use a _cross toolchain_ whose goal is to build
**from** amd64 **to** arm64.

Namely, we need to link the _lemmy_server_ with `pq` and `openssl`
shared libraries and a few others, and they need to be in `arm64`,
indeed.

The toolchain we use to cross-compile is specifically tailored for
Lemmy's needs, see [the image repository][image-repo].

#### References

- [The Linux Documentation Project on Shared Libraries][tldp-lib]

[tldp-lib]: https://tldp.org/HOWTO/Program-Library-HOWTO/shared-libraries.html
[image-repo]: https://github.com/raskyld/lemmy-cross-toolchains

## Container images

GitHub Actions builds the Lemmy backend and `lemmyms-frontend` images and
publishes them to GHCR on branch and release updates. The frontend source uses
the `lemmy-ms` branch. Pull requests build both images without publishing them.
The frontend build uses Node.js 22, Corepack, pnpm 10.26.0, and a writable Elm
cache.

The Compose deployment pulls matching backend and frontend images. Set
`LEMMY_IMAGE_TAG` to a published branch, release, or full commit SHA; its
default is the `multisite` branch. A SHA tag pins both images to the same
backend commit. Compose checks GHCR for the selected tag whenever services
start.

GHCR packages may be private by default. Make both packages public in GitHub
package settings for unauthenticated pulls, or authenticate the deployment host
to `ghcr.io` with a GitHub account and a token that can read packages.

To use an npm registry proxy in GitHub Actions, configure the repository secret
`NPM_PROXY` with the proxy URL. If it is absent, the frontend build uses a direct
connection. The same-origin public Nginx proxy serves the frontend and forwards
API, media, discovery, and ActivityPub requests to Lemmy.

Keep the frontend and API behind the same external HTTPS hostname so
authentication cookies and browser origin checks work correctly.

## S3-compatible media storage

The compose stack stores pict-rs blobs and generic documents in separate
S3-compatible buckets. RustFS is
the default local backend, but the settings are provider-neutral.

1. Copy `docker/.env.example` to `docker/.env` and replace the credentials.
2. Start the stack with `docker compose up -d`.
3. `rustfs-init` creates the `S3_BUCKET` and `S3_FILE_BUCKET` buckets before
   pict-rs starts. Both buckets are private; Lemmy serves content through its
   own API endpoints.

The relevant settings are:

| Variable | Purpose | RustFS default |
| --- | --- | --- |
| `S3_ENDPOINT` | S3 API endpoint | `http://rustfs:9000` |
| `S3_BUCKET` | pict-rs blob bucket | `lemmy-media` |
| `S3_FILE_BUCKET` | Generic document bucket | `lemmy-files` |
| `S3_REGION` | SigV4 region | `us-east-1` |
| `S3_ACCESS_KEY` | S3 access key | development value |
| `S3_SECRET_KEY` | S3 secret key | development value |
| `S3_USE_PATH_STYLE` | Path-style bucket addressing | `true` |

The Compose file passes the document-store values to Lemmy as
`LEMMY_S3_ENDPOINT`, `LEMMY_S3_BUCKET`, `LEMMY_S3_REGION`,
`LEMMY_S3_ACCESS_KEY`, `LEMMY_S3_SECRET_KEY`, and
`LEMMY_S3_USE_PATH_STYLE`. Outside Compose, set those `LEMMY_S3_*` variables
or configure the matching `s3` block in `config.hjson`. If any required value
is missing, generic file uploads stay disabled while Lemmy and pict-rs continue
to start.

For the native disposable server, run `LEMMY_TEST_START_MEDIA=1 ./start-for-dev.sh`
from the repository root. The script starts RustFS and pict-rs in a separate
Compose project with temporary data directories and dynamically selected local
ports, then stops the services and removes their temporary data on exit. Set
`LEMMY_TEST_S3_ACCESS_KEY`, `LEMMY_TEST_S3_SECRET_KEY`,
`LEMMY_TEST_S3_BUCKET`, or `LEMMY_TEST_S3_REGION` to override the local defaults.
With that server running, use a second terminal to run
`python3 scripts/test_file_storage_integration.py`. To verify a missing S3
configuration, restart with `LEMMY_TEST_START_MEDIA=1 LEMMY_TEST_DISABLE_S3=1`
and run `python3 scripts/test_file_storage_failure.py`. To simulate an
unavailable S3 endpoint, start with
`LEMMY_TEST_START_MEDIA=1 LEMMY_S3_ENDPOINT=http://127.0.0.1:1`, then run
`LEMMY_TEST_FAILURE_MODE=unavailable python3 scripts/test_file_storage_failure.py`.

For another S3-compatible provider, change these values and set
`S3_USE_PATH_STYLE` to match the provider. AWS S3 normally uses virtual-host
addressing, while RustFS and many local providers use path-style. The bucket
must exist or be creatable by the credentials used by `rustfs-init`.

### Generic file API

The authenticated API accepts one multipart field named `file` at
`POST /api/v4/file`. It returns `public_id`, a tenant-hosted download `url`,
the sanitized filename, detected content type, and size. Files can be listed
with `GET /api/v4/file/list`, downloaded through
`GET /api/v4/file/{public_id}/{filename}`, and removed through
`DELETE /api/v4/file/{public_id}`. The owner or a site administrator can remove
a file reference.

Site administrators configure `file_upload_disabled`,
`file_max_upload_size_bytes`, and `file_allowed_mime_types` through site
settings. The default upload limit is 25 MiB; oversized bodies return 413.
The MIME list matches types detected from file bytes. An empty list allows any
non-image and non-video type; image and video uploads always use the existing
pict-rs endpoints. Downloads are always attachments with `nosniff`. Private-site
authentication and site-scoped references apply to downloads, so a site's file
URL does not resolve through another site's hostname.
This change provides API support; frontend upload controls are separate work.

Back up the Lemmy database and the complete `S3_FILE_BUCKET` object set
together. Restoring only one side can leave references without bytes or stored
objects without references.

### Migrating an existing filesystem store

Stop Lemmy and pict-rs before migrating. Keep the backup until an old alias,
a new upload, and a deletion have all been verified.

```bash
cd docker
docker compose stop lemmy pictrs
cp -a volumes/pictrs "volumes/pictrs.backup-$(date +%Y%m%d%H%M%S)"
docker compose up -d rustfs rustfs-init

docker compose run --rm --entrypoint /usr/local/bin/pict-rs pictrs \
  --old-repo-path /mnt/sled-repo \
  migrate-store --concurrency 32 \
  filesystem -p /mnt/files \
  object-storage -e "$S3_ENDPOINT" -b "$S3_BUCKET" -r "$S3_REGION" \
    -a "$S3_ACCESS_KEY" -s "$S3_SECRET_KEY" -u

docker compose up -d
```

The `-u` flag selects path-style addressing; remove it for a provider that
requires virtual-host addressing. The sled repository stays on
`./volumes/pictrs`; only the blob files move to S3. Back up both the sled
volume and the bucket.

To roll back, stop pict-rs, restore the backup directory as `volumes/pictrs`,
and start pict-rs with the filesystem store configuration (remove the
`PICTRS__STORE__*` object-storage variables). Keep the bucket for a later
retry; do not delete it until the rollback is confirmed.
