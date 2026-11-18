# Proposal

## Why

Operators need to move a Discourse installation into LemmyMultisite from a standard Discourse backup without losing account identity or the discussion structure. The import must preserve useful relationships and history while making unsupported source features visible instead of silently discarding them.

## What Changes

- Add an offline, resumable importer for Discourse `.tar.gz` and `.sql.gz` backups that targets one explicitly selected LemmyMultisite site.
- Migrate users, categories, topics, posts, replies, private messages, votes, bookmarks, uploads, and other source data where Lemmy has a safe equivalent; report unsupported or lossy conversions.
- Support verification of imported Discourse PBKDF2 hashes and existing bcrypt hashes, rehashing to Argon2id after successful login; new and reset passwords use Argon2id.
- Map each leaf Discourse category to a Lemmy community, and each parent category to a Lemmy multi-community containing its imported descendant communities. Category access restrictions are deferred and reported.
- Provide configurable legacy URL mappings suitable for reverse-proxy redirects to LemmyMultisite frontend routes, with no Discourse hostname or URL baked into the frontend.
- Keep federation disabled for the target site throughout import and do not emit outbound activities.

## Capabilities

### New Capabilities

- `discourse-import`: Offline backup inspection, mapping, import, resumability, validation, and legacy URL compatibility.

### Modified Capabilities

- `multisite/local-identity`: Support multiple password hash formats for imported credentials and upgrade legacy hashes after successful login.

## Impact

- Backend password verification and storage in `crates/api`, `crates/db_schema`, and database migrations.
- A new offline importer and its operator documentation, likely under `scripts/` with a Rust or Python implementation selected during design validation.
- Import mappings for Discourse users, categories, topics, posts, replies, private messages, votes, poll data, bookmarks, and uploads into LemmyMultisite's site-scoped data model.
- Deployment documentation and configurable reverse-proxy rules for old post and user URLs.
- The separate frontend repository is read-only for this change; its current routes remain the destination format.
