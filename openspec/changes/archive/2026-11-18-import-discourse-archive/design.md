# Design

## Context

See proposal.md for motivation and specs for externally visible behavior. Discourse backups contain a PostgreSQL SQL dump and may contain `uploads/` files. The LemmyMultisite data model is site-scoped and has no native Discourse poll model. Existing Lemmy local-user authentication uses bcrypt, and the frontend generates `/post/:id`, `/post/:id/:commentId`, `/comment/:id`, `/c/:name`, and `/u/:username` routes. The frontend repository is outside this change's write scope.

## Goals / Non-Goals

**Goals:**

- Make source extraction, transformation, target writes, and verification repeatable and auditable.
- Preserve account identity and as much source data and chronology as target semantics allow.
- Ensure import operations follow LemmyMultisite tenant and presence invariants without producing federation deliveries.
- Offer deployable legacy URL redirects generated from mappings and configured source host information.

**Non-Goals:**

- Import Discourse themes, plugins, moderation workflows, category ACLs, trust-level behavior, or application configuration.
- Enable federation or make imported data available to other instances.
- Change frontend route definitions or embed source hostnames in frontend assets.
- Guarantee semantic equivalence for Discourse polls, badges, likes, and plugin-specific features that have no Lemmy equivalent.

## Decisions

### Isolated source extraction and preview before writes

The importer will be an offline Rust workspace utility that reads a local archive, verifies its expected structure, and restores the SQL dump into a disposable source database/schema for normalized reads. It will use a read-only source connection and a separate target connection. Preview and mapping resolution complete before target writes. Archive extraction must reject absolute paths, traversal entries, links escaping the extraction root, and excessive expanded size. `--plan`/preview output will include archive metadata, detected Discourse version, source counts, warnings, and explicit user/category/URL conflict decisions.

**Alternative considered:** parsing the SQL text directly. Discourse dumps are large, schema-versioned SQL with sequences and PostgreSQL-specific data; restoring into an isolated temporary database delegates SQL interpretation to PostgreSQL and keeps the parser focused on stable table data.

### Rust importer with explicit mapping ledger

The importer will live in a dedicated workspace crate or binary and share Lemmy schema types and import helpers rather than invoking public APIs one record at a time. A durable ledger keyed by `(source fingerprint, target local_site_id, source table, source id)` will record target IDs, phase status, and errors. Each record or bounded batch is transactional and idempotent. A source fingerprint and target-site identity prevent accidentally resuming into another site. Before creating rows, the importer will check that the selected site exists, federation is disabled, and it owns no conflicting in-progress import.

The importer must use domain insertion paths or dedicated import helpers that establish `origin_local_site_id`, actor identifiers/keys, presence rows, site ownership, timestamps, aggregates, and media references. It must not call activity submission paths. Imported local actors and content remain local-only; no outbound queue records are created.

**Alternative considered:** drive Lemmy's public REST API. Normal create endpoints impose registration and community permissions, assign current timestamps, trigger user-facing notifications and activity submission, and cannot directly preserve all source relationships. Imported private messages still need read notification rows because Lemmy uses them to list message history; the importer inserts those rows transactionally as already read without invoking notification hooks, email, or activity submission. **Alternative considered:** raw SQL-only inserts. They bypass transaction helpers and tenant invariants. A dedicated Rust path can use typed forms and transactions while deliberately excluding external side effects.

### Import order and source-to-target mappings

The importer will process dependencies in stages:

1. Site selection, source inventory, normalized conflict decisions, and mapping ledger.
2. Users and local identities, then categories and communities, then parent-category multi-communities.
3. Topics and first posts, then replies/comments in parent-before-child order.
4. Private-message participants and messages, votes/bookmarks, tags and supported metadata.
5. Upload objects/references, content URL rewriting, URL redirect map generation, and integrity validation.

Stable source IDs are retained in the ledger, not forced into Lemmy primary keys. Timestamps are copied where schema and insertion APIs permit. Category leaves become communities. Each parent category becomes a multi-community containing its imported descendant communities; a category that is both a parent and has its own topics is also represented as a community so its content is not lost. Lemmy's multi-community membership limit is fewer than 50 communities. A parent with 50 or more mapped descendants is reported unresolved and blocks final validation rather than producing an incomplete membership set. Category ACLs remain warnings for later policy import.

Discourse `posts` map the first non-action post of each topic to one Lemmy Post. Reply posts map to Comments with parent references reconstructed from topic/post-number relationships. Deleted, system, staged, or anonymized accounts require explicit deterministic identity rules in preview. Private messages are imported only when both endpoints can be mapped; sender/recipient site presence and visibility must be established without making the message visible to unrelated sites.

Lemmy has no native poll entity equivalent to Discourse's polls. The first implementation will preserve poll prompt, options, and aggregate outcome as a clearly labeled textual rendering attached to the imported post, and record detailed voter/choice rows in the migration report/export ledger when they cannot safely become native votes. This keeps historical information without claiming that interactive poll behavior survived.

### Password verifier dispatch and upgrade

The stored verifier string will remain self-identifying: Argon2id PHC strings for new/reset passwords, bcrypt strings for existing accounts, and a validated Discourse PBKDF2 representation for imported accounts. Login dispatches by recognized prefix/version. Successful bcrypt or PBKDF2 verification replaces the verifier with Argon2id using the submitted plaintext password; failed or unknown formats do not modify it. Exact Discourse PBKDF2 encoding, digest, iteration and salt representation must be confirmed against the source version before implementation, with fixtures from Discourse code or a generated test account. Password verification and Argon2id hashing remain in blocking workers with bounded resource settings.

**Alternative considered:** forcing password resets for all migrated accounts. This is simpler but fails the requested account continuity. **Alternative considered:** adding an algorithm column. The existing verifier field can distinguish PHC and bcrypt formats, avoiding a schema column; if the confirmed Discourse encoding is ambiguous, a tagged envelope or explicit algorithm column can be selected during implementation.

### Data compatibility and loss accounting

For each source feature, a conversion registry will define source query, target representation, dependencies, and loss notes. Native target equivalents include account records, posts, comments, private messages, post/comment votes, bookmarks, and local communities. Where semantics differ, preview and report will state the conversion: e.g., Discourse likes may become positive Lemmy votes only when source meaning maps cleanly; reactions, poll ballots, wiki/edit history, badges, and plugin data remain preserved as metadata/report output or are explicitly marked unsupported. Source timestamps and ownership are retained where possible. Import completion requires referential checks for users, categories, topic starters, reply parents, message participants, and attachments.

Uploads are read from the archive's `uploads/` tree and matched through Discourse upload rows. Images will be ingested through supported pict-rs/media helpers and content URLs rewritten to target aliases. Non-image attachments will use the existing generic file storage only if its ownership and access controls can represent the source visibility; otherwise files and a manifest are retained for manual import and reported.

### External legacy URL mapping

The importer will export a data file of source URL paths and target frontend paths based on source-to-target mappings. Deployment documentation will provide a configurable reverse-proxy include/template that accepts the legacy hostname and generated mapping file from operator configuration. Topic URLs redirect to `/post/{lemmy_post_id}`; URLs targeting a particular reply redirect to `/post/{lemmy_post_id}/{lemmy_comment_id}` when available. Unknown or unmapped paths fall through according to operator proxy configuration. Neither hostnames nor the Discourse installation path are compiled into the Elm frontend.

**Alternative considered:** adding legacy route parsing to the frontend. That requires frontend changes and still cannot reliably resolve arbitrary old IDs without a mapping service; a generated reverse-proxy map is deployable without rebuilding frontend assets.

## Risks / Trade-offs

- [Discourse schema changes between archive versions] -> Detect backup metadata/version, support a declared compatibility range, and reject unknown schemas during preview instead of partially importing.
- [Large archives exhaust disk or memory] -> Stream file processing, bound decompression and batch sizes, and report required temporary storage before restore.
- [Import may expose restricted content] -> Require explicit ACL warnings and target privacy review; do not claim source ACLs are preserved.
- [Password format details vary by Discourse version] -> Confirm upstream implementation and fixtures before enabling credential import; unsupported formats use reset-required status.
- [Direct imports can violate model invariants] -> Use typed domain helpers, per-phase transactions, and pre-completion SQL integrity audits; never mark a phase complete on partial failure.
- [Source-to-target feature mismatch] -> Keep a per-record loss report and preserve unsupported source data in a manifest where feasible.
- [Redirect map can contain stale IDs] -> Generate it from the completed ledger and provide a validation mode that checks all target IDs before publishing.
- [Federation-disabled controls may still permit local event side effects] -> Use import-specific helpers that persist only the read notification rows required to display imported private messages, do not invoke notification hooks or submit activities, and assert no outbound deliveries were created.

## Migration Plan

1. Deploy password verifier support and schema changes while keeping current bcrypt logins valid; new/reset credentials use Argon2id.
2. Run importer preview against a backup copy and resolve account, category, privacy, and URL mapping warnings.
3. Back up the target database and media store, pause target writes, and run import with federation disabled.
4. Run integrity validation and inspect the report; publish generated redirect configuration only after validation succeeds.
5. Resume normal target writes and retain the source archive and ledger until operator acceptance.

Rollback restores the target database and media snapshot taken before import and removes the generated proxy include. The importer ledger is scoped to the same target snapshot and must be restored or discarded with it. Hash upgrades are backward-compatible with existing bcrypt verifiers; rollback of the password migration restores the database snapshot if required.

