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

## S3-compatible media storage

The compose stack stores pict-rs blobs in an S3-compatible bucket. RustFS is
the default local backend, but the settings are provider-neutral.

1. Copy `docker/.env.example` to `docker/.env` and replace the credentials.
2. Start the stack with `docker compose up --build` from this directory.
3. `rustfs-init` creates the `S3_BUCKET` bucket before pict-rs starts. The
   bucket is private; Lemmy serves images through its own image endpoints.

The relevant settings are:

| Variable | Purpose | RustFS default |
| --- | --- | --- |
| `S3_ENDPOINT` | S3 API endpoint | `http://rustfs:9000` |
| `S3_BUCKET` | pict-rs blob bucket | `lemmy-media` |
| `S3_REGION` | SigV4 region | `us-east-1` |
| `S3_ACCESS_KEY` | S3 access key | development value |
| `S3_SECRET_KEY` | S3 secret key | development value |
| `S3_USE_PATH_STYLE` | Path-style bucket addressing | `true` |

For another S3-compatible provider, change these values and set
`S3_USE_PATH_STYLE` to match the provider. AWS S3 normally uses virtual-host
addressing, while RustFS and many local providers use path-style. The bucket
must exist or be creatable by the credentials used by `rustfs-init`.

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
