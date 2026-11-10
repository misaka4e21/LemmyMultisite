## 1. Schema foundation

- [x] 1.1 Add `local_site.hostname` with a unique index and a migration that backfills the current `Settings.hostname`; verify the migration with a copy of a single-site database and a query for one row per hostname.
- [x] 1.2 Add nullable `origin_local_site_id` to `person`, `community`, `post`, `comment`, `private_message`, and `multi_community`, then backfill existing local rows to the bootstrap site; verify remote rows remain NULL and local rows point to the bootstrap site.
- [x] 1.3 Add `local_site_id` to `local_user` and replace the global email and username uniqueness with site-scoped constraints; verify two accounts with the same name and email can exist on different sites.
- [x] 1.4 Add `local_site_id` to `sent_activity`, `received_activity`, `federation_allowlist`, `federation_blocklist`, and `federation_queue_state`, and replace the queue primary key with `(local_site_id, instance_id)`; verify the new keys with a migration test.
- [x] 1.5 Add indexes for `(local_site_id, object_id)` on every site-scoped table and for `(local_site_id, lower(name))` where name lookup is used; verify query plans for the main list queries.

## 2. Tenant routing and site registry

- [x] 2.1 Add a `TenantContext` containing the LocalSite row, Site row, canonical hostname, and scheme; verify concurrent requests for two sites receive different contexts.
- [x] 2.2 Add hostname canonicalization and a tenant lookup cache with invalidation on site create, update, and delete; verify unknown hosts return 404 and changed hostnames stop resolving.
- [x] 2.3 Move `SiteView::read_local` call sites to `read_by_hostname` or `read_for_site`; verify no production path depends on `.first()` for the active site.
- [x] 2.4 Add a per-site rate limiter keyed by `(local_site_id, action, client_ip)` and load bucket values from the active site's `LocalSiteRateLimit`; verify requests to two sites do not share a bucket.
- [x] 2.5 Key `SiteView`, `GetSiteResponse`, `LocalSiteData`, slur filter, URL blocklist, and latest activity ID caches by `local_site_id`; verify a site edit invalidates only that site's entries.
- [x] 2.6 Render WebFinger, nodeinfo, feeds, sitemap, ActivityPub actor and inbox URLs, and `/api/v4/image/{alias}` with the active site hostname; verify the same path returns site-specific URLs on two hosts.

## 3. Authentication and local identity

- [x] 3.1 Add `local_site_id` to JWT claims and validate it against the request tenant in session and API authentication; verify a token from site A is rejected on site B with 403.
- [x] 3.2 Move session middleware behind tenant resolution or make it resolve the tenant before token validation; verify authenticated and anonymous requests on both sites.
- [x] 3.3 Scope login, registration, password reset, email verification, OAuth account linking, login tokens, and invite flows to the active site; verify the same email and username work on two sites.
- [x] 3.4 Scope local actor lookup and WebFinger to the active site when the domain is omitted, and resolve another installation site only when the request host matches that site; verify `alice@A` and `alice@B` resolve to different Person rows.
- [x] 3.5 Scope site administrator checks, bans, registration decisions, reports, modlog, and admin user lists to the active site; verify site A's administrator cannot read or act on site B's local data.
- [x] 3.6 Set the site JWT in a host-scoped HttpOnly cookie with SameSite=Lax on browser login, accept it in session authentication, clear it on logout, and update `lemmyms-frontend` login/logout to rely on the cookie; verify login, logout, and cross-site cookie rejection on two hosts.

## 4. Canonical objects and site presence

- [x] 4.1 Add `person_site`, `community_site`, `post_site`, `comment_site`, `multi_community_site`, and `private_message_site` tables with `local_site_id`, object id, presence timestamps, and site state columns; verify foreign keys and uniqueness constraints.
- [x] 4.2 Create the origin site's presence row when a local object is created and the destination site's presence row when an activity is received; verify site B cannot list a site A post before delivery.
- [x] 4.3 Switch community, post, comment, person, multi-community, private message, report, and modlog reads to canonical plus presence joins; verify no list returns another site's local-only object.
- [x] 4.4 Split origin moderation from local moderation in the write paths and views; verify site A's local removal of a remote post does not change site B's view.
- [x] 4.5 Move per-site featured state, local locks, local counters, and derived thumbnail references into the site tables; verify each site keeps its own values for the same canonical post.
- [x] 4.6 Remove site-specific proxy and thumbnail URLs from canonical rows and render them in the view layer; verify a remote image post fetched on site A renders site B's own proxy URL when B reads it.

## 5. Media deduplication

- [x] 5.1 Add `media_object` and `media_reference` tables with a unique content hash, a unique canonical alias, site scope, owner, and purpose; verify the constraints with migration tests.
- [x] 5.2 Hash the upload stream while forwarding bytes to pict-rs, reuse an existing media object by hash, and insert only a new site reference for duplicates; verify two identical uploads produce one media object and two references.
- [x] 5.3 Replace `local_image` reads and writes with the new reference model, keeping a compatibility view during migration if needed; verify media lists return only the active site's references.
- [x] 5.4 Key `image_details` by media object or source hash instead of a site-specific URL; verify the same remote image uses one details row across two sites.
- [x] 5.5 Delete only the active site's reference, then delete the pict-rs alias when the last reference is gone; verify deleting site A's image leaves site B's image available.
- [x] 5.6 Keep `remote_image` global by source URL and render proxy URLs per site; verify the same remote image is tracked once and proxied from each site's hostname.
- [x] 5.7 Apply the active site's private instance policy to the image endpoint and prevent media enumeration across sites; verify anonymous access to a private site's image returns 401.

## 6. Federation identity and inbound routing

- [x] 6.1 Build one `FederationConfig<LemmyContext>` per LocalSite with the site domain, signed fetch actor, key, and URL verifier; verify two site configs produce different actor domains.
- [x] 6.2 Add a tenant federation middleware that selects the config from Host and inserts the matching `Data<LemmyContext>`; verify concurrent site requests do not share the domain or signer.
- [x] 6.3 Scope inbound signatures, actor fetch, object fetch, WebFinger, inbox, and outbox handling to the active site; verify an activity addressed only to another local site's actor is rejected or treated as external.
- [x] 6.4 Change `received_activity` to `(local_site_id, ap_id)` and keep canonical object upserts idempotent by `ap_id`; verify an activity delivered to A and then B is processed for both sites.
- [x] 6.5 Scope federation allow lists, block lists, federation enabled state, signed fetch, and registration policy to the active site; verify a block on site A does not change site B.

## 7. Outbound queues and internal delivery

- [x] 7.1 Add `local_site_id` to `sent_activity` and derive it when building `SentActivityForm` from the initiating local actor or object; keep an explicit field only for variants without a local actor. Verify an administrator action on a remote object records the administrator's site as origin.
- [x] 7.2 Change the send worker to read the next activity for its site instead of assuming a contiguous global ID, and key retry state by `(local_site_id, instance_id)`; verify two sites sending to the same instance keep independent state.
- [x] 7.3 Add `internal_activity_delivery` rows for same-installation destinations and an internal dispatcher that runs the shared inbound handler; verify a Follow from site A to site B creates the follow state and an Accept without an HTTP request.
- [x] 7.4 Make internal delivery idempotent and policy-aware, including local bans, blocks, and destination site settings; verify duplicate delivery processes once and blocked delivery is not applied.
- [x] 7.5 Keep external federation on the existing HTTP path with site-specific signing; verify a mock external instance receives a signed activity from the correct site.
- [x] 7.6 Isolate internal delivery retries from external queues and from other site pairs; verify a failing A to B delivery does not stop A's external queue or B's other deliveries.

## 8. Installation administration and site lifecycle

- [x] 8.1 Add `installation_admin` credentials and sessions that are not Person or LocalUser rows; verify the session can manage sites but cannot call site user APIs.
- [x] 8.2 Add site creation that writes Instance, Site keys, system account, LocalSite, rate limit, and the first site administrator in one transaction; verify a new host serves `/api/v4/site` without a restart.
- [x] 8.3 Add site disable, remove, and retention handling that removes local rows and references while preserving shared remote objects and media with other references; verify site B keeps shared data after site A is removed.
- [x] 8.4 Enforce hostname immutability after content exists and invalidate the hostname cache on an allowed change; verify a hostname change on a site with local users is rejected.
- [x] 8.5 Move installation settings to the installation layer and site settings to LocalSite; verify a new site starts without editing the installation config file or restarting the process.

## 9. Migration, compatibility, and validation

- [x] 9.1 Add an expand, backfill, dual-write, switch, and contract migration sequence for the schema; verify each phase runs forward and backward on a copy of a single-site database.
- [x] 9.2 Keep a feature flag or compatibility path for the old single-site reads until the multisite path passes integration tests; verify the flag can disable the new tenant path.
- [x] 9.3 Add integration tests for same username and community name on two sites, shared remote objects, local moderation isolation, media deduplication, internal Follow and Accept, and external HTTP federation.
- [x] 9.4 Add a migration test that starts from a single-site database and verifies the backfilled origin site, presence rows, queue keys, and media references.
- [x] 9.5 Remove `Settings.hostname`, singleton site helpers, global queue keys, alias-keyed media rows, and the compatibility columns after all readers switch; verify the full workspace builds and the focused multisite tests pass.
