# Media use inventory

The reference table currently records uploads and downloaded thumbnails. It does not account for every stored use of a local alias.

| Stored use | Fields | Site association and write paths |
| --- | --- | --- |
| Post | `body`, `url`, `thumbnail_url`, `embed_video_url` | `post_site`; local create/update and ActivityPub post ingestion |
| Comment | `content` | `comment_site`; local create/update and ActivityPub comment ingestion |
| Person | `bio`, `avatar`, `banner` | `person_site`; local profile updates and ActivityPub person ingestion |
| Community | `sidebar`, `icon`, `banner` | `community_site`; local create/update and ActivityPub community ingestion |
| Site | `sidebar`, `icon`, `banner` | local site ownership or receiving site; local site create/update and ActivityPub instance ingestion |
| Private message | `content` | sender and recipient site visibility; local create/update and ActivityPub private message ingestion |
| Multi-community | `sidebar` | owner and site visibility; local create/update and ActivityPub ingestion |
| Tagline | `content` | owning site; local create/update |
| Person note | `person_actions.note` | note author's site; local note update |
| Custom emoji | `image_url` | owning site; local create/update |
| Report snapshots | original post, comment, community, and message fields | report site association; report creation and federation ingestion |

Existing generated image URLs use `/api/v4/image/{alias}`. Stored content can also contain legacy image URLs and image proxy URLs with an encoded source URL. Resolve the full origin and path against installation-owned sites before matching an alias; an external hostname with the same path or filename is not a local reference. Markdown parsing must exclude code blocks and recognize image syntax, titles and encoded URLs. Direct URL fields need their own parser.

Reference maintenance must run when content changes and when a site gains or loses presence, in the same consistency boundary as the corresponding write. Existing rows must be scanned before final deletion is enabled. Unrecognized or ambiguous URL forms need conservative handling so the backfill cannot declare a live alias unused.

## Caller audit

The low-level delete helper has two entry points. `delete_image_alias` is used only by upload deduplication after pict-rs returns a second alias for bytes that already have a canonical media object. That duplicate alias has no database object, so removing it does not touch any site's references. `delete_queued_image_alias` requires the media object to be queued and the backfill to be complete; it is the only path that removes canonical media.

Profile, community, site, and user image replacement all use `delete_old_image`, which calls `delete_for_site_and_user`. It removes the current site's reference for the current owner and queues the canonical object only when no upload or content reference remains. Site administrator image deletion uses `delete_for_site` after an administrator check and a current-tenant check; a foreign-only alias returns `NotFound`, while a shared alias loses only the current site's references. User purge enumerates upload references for one person and one site, then uses the same scoped operation.

`purge_image_from_pictrs_url` now delegates to the queued deletion path. Its only caller is `purge_post_images`, which is no longer reached by post purges because post content references are reconciled through `MediaReference`. The scheduled cleanup lists only queued aliases after the backfill state is complete, so a referenced alias cannot be removed by the retry loop.
