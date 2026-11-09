## Why

LemmyMultisite currently behaves like upstream single-site Lemmy. One hostname identifies the installation, one local site owns every local user and community, and one federation identity signs every activity. The installation needs to host several independent Fediverse sites in one process and one database while storing shared remote objects and uploaded media once.

## What Changes

- Separate the installation from LocalSite. Each LocalSite has its own hostname, Site actor, keys, administrators, accounts, communities, and federation policy.
- Resolve the tenant from Host for API, WebFinger, ActivityPub, feeds, images, mail, and scheduled work.
- Keep local accounts isolated by site. `alice@A` and `alice@B` are different accounts with different passwords and Person rows. JWT carries `local_site_id` and must match Host.
- Store canonical ActivityPub objects once. Remote Person, Community, Post, and Comment rows are shared. Each site has presence and state rows for visibility, local moderation, featured state, aggregates, and derived media.
- Deduplicate uploads by content hash. A global media object holds the canonical alias, and site rows reference it. Deletion follows reference counts.
- Serve `/api/v4/image/{alias}` from each site's hostname while using one pict-rs backend.
- Deliver activities between sites in the same installation through an internal database path. External instances keep standard HTTP ActivityPub.
- **BREAKING**: remove single-site assumptions around `Settings.hostname`, global `SiteView::read_local`, one federation queue, and alias-keyed local images.

## Capabilities

### New Capabilities

- `multisite/site-routing`: resolve LocalSite from Host, provide the tenant context, load site-scoped configuration and caches, and isolate API, feeds, ActivityPub, and media routes.
- `multisite/site-administration`: define installation administrators, site creation and removal, site hostname and key lifecycle, and the boundary between installation and site configuration.
- `multisite/local-identity`: scope accounts, Person, Community, Post, Comment, and LocalUser rows to a site, and enforce the site boundary in login, JWT, and administrator checks.
- `multisite/activitypub-objects`: define shared canonical ActivityPub objects, per-site presence and state, object visibility, and the separation between origin moderation and local moderation.
- `multisite/media-storage`: define content-addressed media objects, site references, per-site URLs, deduplicated uploads, and reference-counted deletion.
- `multisite/federation`: define per-site ActivityPub identity, allow and block policy, inbound routing, outbound queues, site signing, and internal database delivery between sites in the same installation.

### Modified Capabilities

None. `openspec/specs/` is empty, so all capabilities are new.

## Impact

- Database: `local_site`, `local_user`, `person`, `community`, `post`, `comment`, `private_message`, `multi_community`, `sent_activity`, `received_activity`, `federation_queue_state`, `federation_allowlist`, `federation_blocklist`, `local_image`, and `image_details`, plus new `*_site` and `media_*` tables.
- Server: middleware order, federation configuration, and startup site setup in `crates/server`; tenant filters in `crates/api` and `crates/db_views`; actor URLs, inbox and outbox handling, sending, and receiving in `crates/apub`; WebFinger, feeds, nodeinfo, images, and scheduled work in `crates/routes`.
- Authentication and security: JWT claims, session middleware, login, registration, password reset, email verification, OAuth, site administrator boundaries, and installation administrator boundaries.
- Media: shared pict-rs storage, media aliases and URLs, upload deduplication, reference-counted deletion, and remote image proxying.
- Operations: site creation and removal, per-site SMTP sender, per-site rate limits, cache invalidation, migration, and rollback.
