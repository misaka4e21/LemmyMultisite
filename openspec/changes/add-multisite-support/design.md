## Context

See `proposal.md` for the problem. This design assumes a hard fork of Lemmy `1.0.0-beta.3`, one process, one PostgreSQL database, and one shared pict-rs deployment. The current code builds one `FederationConfig` at startup, reads the first `LocalSite` through `SiteView::read_local`, keeps one `RateLimit` built from that site, stores outgoing activities in one global table, and keys `local_image` by `pictrs_alias`. Those assumptions appear across `crates/server`, `crates/api`, `crates/apub`, `crates/db_views`, `crates/routes`, and `crates/email`.

## Goals / Non-Goals

**Goals:**

- Resolve one LocalSite from Host and use that tenant for every site-scoped read, write, cache, queue, and URL.
- Keep local accounts isolated by site while sharing canonical remote ActivityPub objects.
- Deduplicate uploaded media by content hash and keep per-site references.
- Give each site its own ActivityPub identity, policy, and outbound queue.
- Deliver activities between sites in the same installation through an internal path backed by database records.

**Non-Goals:**

- Real HTTP federation between sites in the same installation.
- One database or schema per site.
- Full frontend redesign. The backend contract comes first.
- Compatibility with upstream Lemmy migrations. The fork may remove singleton APIs.
- Changes to the external ActivityPub protocol.

## Decisions

### 1. One database and one schema

Use one PostgreSQL database and one schema. Site-scoped rows carry `local_site_id`; shared rows stay global. This keeps Diesel joins and transactions simple. A database per site would give stronger isolation, but it breaks cross-database joins, transactions, and the existing schema code. A schema per site has the same problem with static Diesel table definitions.

### 2. LocalSite is the tenant root

Add `local_site.hostname` with a unique index. Keep the existing `local_site.site_id` link to the Site actor. Each local site also has an Instance row for its hostname, so `person_name_instance_unique` continues to allow the same local username on different sites.

Add `origin_local_site_id` to `person`, `community`, `post`, `comment`, `private_message`, and `multi_community`. The column is NULL for objects that belong to an external instance and non-NULL for objects created by a site in this installation. Existing `local` boolean columns become derived values or compatibility columns during migration. New code uses two helpers: `is_installation_local(object)` and `is_site_local(object, site)`.

The alternative, adding a tenant column to every federated object, creates duplicate remote rows and forces every remote object to belong to one site. The shared canonical model avoids that.

### 3. Canonical objects and site presence

Keep one canonical row per `ap_id` for Person, Community, Post, Comment, PrivateMessage, and MultiCommunity. Add a presence and state table for each content type that a site can list or moderate:

```text
person_site(local_site_id, person_id, known_at, last_refreshed_at)
community_site(local_site_id, community_id, known_at, last_refreshed_at, removed, ...)
post_site(local_site_id, post_id, known_at, last_refreshed_at, removed, locked, featured, ...)
comment_site(local_site_id, comment_id, known_at, last_refreshed_at, removed, ...)
multi_community_site(local_site_id, multi_community_id, ...)
private_message_site(local_site_id, private_message_id, role, ...)
```

Creating a local object inserts the origin site's presence row. Receiving an activity inserts the destination site's presence row. Reading from a site joins the canonical row with that site's presence row. A canonical row alone does not make an object visible.

This is the main departure from a simple `local_site_id` column. Shared canonical rows cannot carry local moderation, featured state, or derived URLs because those values differ by site. The presence tables carry those values and keep remote content shared.

### 4. Separate origin state from local state

Canonical rows store state published by the origin instance: deleted by the author, removed by the origin community, and source URLs. Site rows store state applied by the local site: local removal, local lock, local feature, and local counters.

The current `post.removed`, `post.locked`, `post.featured_local`, `comment.removed`, and `community.removed` columns mix both cases. The migration keeps the canonical columns for origin state and creates site columns for local state. A local administrator removing a remote post writes `post_site.removed`. The origin community removing the post writes `post.removed_by_origin` and that value is visible to every site with the post.

### 5. Canonical rows contain source data

`post.url` and `post.thumbnail_url` store source URLs. `post_site` stores the local thumbnail media reference. Views build proxy URLs from the tenant context. The current code can write `https://a.example/api/v4/image/proxy?...` into a remote post row during metadata generation. That value must not be shared with site B.

The same rule applies to avatars, banners, community icons, and private message attachments. Store a media identifier or source URL in the canonical row, then render the site URL in the view layer.

### 6. Site-scoped accounts and authentication

Add `local_user.local_site_id` as a non-null column. Unique constraints become site-scoped:

```text
local_user(local_site_id, lower(email))
person(name, instance_id)
local_user(person_id)
```

Login resolves the tenant from Host, then finds the Person and LocalUser inside that site. JWT gains a `local_site_id` claim. Session validation rejects a token whose site does not match Host. Password reset, email verification, OAuth account linking, and login tokens use the same site scope.

The middleware order is:

```text
Host -> TenantResolver -> RateLimit(site) -> Session/JWT(site) -> Handler
```

`SessionMiddleware` currently runs with the global context before tenant resolution. It has to move behind tenant resolution, or it has to resolve the tenant itself before validating the token.

### 7. Content-addressed media

Add two tables:

```text
media_object(
  id, content_hash UNIQUE, canonical_pictrs_alias UNIQUE,
  size_bytes, width, height, content_type, blurhash, created_at
)

media_reference(
  id, local_site_id, media_id, owner_person_id, post_id, purpose, created_at
)
```

The upload path hashes the incoming byte stream while forwarding it to pict-rs. If the hash exists, it reuses the canonical alias and inserts a site reference. If the hash is new, it stores the alias and metadata. A unique index on `content_hash` and a transaction prevent duplicate media objects under concurrent uploads.

Deletion removes the current site's reference. The pict-rs alias is deleted only when the last reference is gone. Installation administrator purge may remove every reference and the media object.

`local_image` is replaced by `media_reference` or kept as a compatibility view during migration. `image_details` is folded into `media_object` or keyed by `media_id`. `remote_image` stays global by source URL. The image endpoint still serves `/api/v4/image/{alias}` from the current site hostname.

This design relies on one shared pict-rs backend. Per-site pict-rs deployments would prevent cross-site deduplication and are out of scope.

### 8. One federation configuration per site

Build one `FederationConfig<LemmyContext>` for each LocalSite at startup and after site changes. Each config has the site domain, site actor for signed fetch, site policy, and site URL verifier. A tenant middleware resolves Host, selects the matching config, and inserts `Data<LemmyContext>` for that site.

`LemmyContext` stays the app data type required by `activitypub_federation`. It holds shared resources and the active tenant. Background work builds a tenant-specific context or iterates sites explicitly.

The alternative, changing the domain inside one global config per request, is unsafe under concurrency. The `activitypub_federation` crate may not support swapping the config through its stock middleware. A short spike should confirm whether the fork can provide its own middleware or needs a small patch to the dependency. This is the highest external risk in the change.

### 9. Per-site outbound queues

Add `local_site_id` to `sent_activity`. Replace `federation_queue_state(instance_id)` with `federation_queue_state(local_site_id, instance_id)`. The worker reads the next activity for its site instead of assuming a contiguous global ID. `get_latest_activity_id`, retry state, and cached activity info are keyed by site.

`SendActivityData` carries `local_site_id` explicitly. The origin of an activity can differ from the origin of the object, for example when site A's administrator removes a remote post. The site that initiated the activity signs it and owns its retry state.

### 10. Per-site received activity log

Change `received_activity` to `(local_site_id, ap_id)`. Canonical object upserts remain idempotent by `ap_id`, but delivery deduplication is per site. An activity delivered to A and later to B is processed for both sites. With a global dedupe key, B would miss its notifications and local side effects.

### 11. Internal delivery between sites

Add an `ActivityOrigin` value:

```text
External
Internal(LocalSiteId)
```

The outbound activity is stored once in `sent_activity` with its origin site. External delivery uses the existing HTTP worker. Internal delivery creates `internal_activity_delivery(origin_site_id, destination_site_id, activity_id, state, attempts)` rows. An internal dispatcher deserializes the activity and runs the same inbound handler with the destination site's `Data<LemmyContext>`.

The internal path skips HTTP and HTTP signatures. It still runs the destination site's allow and block policy, ban checks, presence updates, notifications, and idempotency checks. The origin is trusted because the row was created by the local server, not by a remote request.

Directly inserting the resulting follow, vote, or notification rows would skip policy and notification logic. The shared handler keeps external and internal behavior aligned.

### 12. Per-site rate limits and caches

The rate limiter key becomes `(local_site_id, action, client_ip)`, and the bucket configuration comes from the active site's `LocalSiteRateLimit`. Caches for `SiteView`, `GetSiteResponse`, `LocalSiteData`, slur filters, URL blocklists, and latest activity IDs are keyed by `local_site_id`. Site changes invalidate only that site's entries.

### 13. Installation administrator identity

Add an `installation_admin` table with its own credentials and sessions. The account is not a Person and not an ActivityPub actor. Bootstrap can create the first installation administrator from configuration or a CLI command. The HTTP API uses the installation administrator session to create, disable, or remove sites and to change installation settings.

Using a `local_user` on a special site would confuse Fediverse identity and site administration. A global admin flag on every site user has the same problem and weakens the boundary between installation and site administration.

### 14. Hostname lifecycle

`local_site.hostname` is unique and immutable once the site has local actors or federated content. A newly created empty site may change its hostname. After content exists, a hostname change would break AP identifiers, WebFinger, signatures, and remote caches. Operators who need a new hostname create a new site and migrate content through federation.

## Risks / Trade-offs

- Shared rows can leak site-local content through a missing query filter. Mitigation: central tenant query helpers, integration tests for every listing, and a later row-level security layer as defense in depth.
- Per-site presence joins add query cost. Mitigation: composite indexes on `(local_site_id, object_id)` and cached site rows. Accept the cost as the price of correct isolation.
- The `activitypub_federation` crate may not allow per-request federation configuration. Mitigation: run a spike before the queue work and patch the dependency if needed.
- Internal delivery can drift from HTTP delivery. Mitigation: share the inbound handler and add contract tests that run the same activity through both paths.
- Media deduplication adds hashing cost and refcount races. Mitigation: stream hashing, bounded upload size, transactions, unique constraints, and row locks on the media object.
- The migration touches most tables and query paths. Mitigation: expand, backfill, dual-write, switch reads, then contract. Keep a backup and a feature flag that keeps the old single-site path until the switch is verified.
- Combined aggregates can expose site-local votes. Mitigation: keep installation-wide aggregates only for content that is already federated globally, and keep site-local moderation and presence separate.
- Host header spoofing can select the wrong tenant. Mitigation: accept only configured hostnames, canonicalize the header, and configure trusted proxies.
- Site removal can delete data too early. Mitigation: soft disable first, retain data for a configurable window, and remove shared objects only when no reference remains.

## Migration Plan

1. Expand the schema: add `local_site.hostname`, `origin_local_site_id`, presence tables, media tables, site columns on queues and policy tables, and composite keys. Backfill the existing single site as the origin for local rows. Leave remote rows NULL.
2. Add tenant resolution and the per-site federation registry. Keep the current `Settings.hostname` as the bootstrap hostname until the new path is verified.
3. Move authentication, sessions, rate limits, and caches to the tenant context. Add the JWT site claim and cross-site rejection tests.
4. Switch object reads to canonical plus presence joins. Dual-write the old columns during the transition. Move local moderation and derived media to site rows.
5. Backfill `local_image` into `media_object` and `media_reference`. Dual-write uploads and deletions, then switch reads.
6. Add `local_site_id` to outgoing activities and switch queue state to `(local_site_id, instance_id)`. Implement internal delivery and run A to B and B to A integration tests.
7. Contract the schema: remove `Settings.hostname`, singleton helpers, old queue keys, alias-keyed media rows, and compatibility columns that no longer have readers.

Rollback before the contract phase uses the feature flag and the old columns. The migration down scripts should restore the single-site schema. After the contract phase, rollback requires a database backup.

## Testing Strategy

- Unit tests for host canonicalization, JWT site mismatch, media hash reuse, and refcount deletion.
- Integration tests for two sites with the same username and the same community name.
- Integration tests for one shared remote post with separate site presence and separate local removal.
- Integration tests for media uploaded on A and reused on B, including deletion from one site.
- Integration tests for internal Follow and Accept between A and B, including duplicate delivery and policy rejection.
- A mock external instance for HTTP federation signing and inbound handling.
- A migration test that starts from a single-site database and backfills the new rows.
