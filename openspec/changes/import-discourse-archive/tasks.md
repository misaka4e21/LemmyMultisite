# Tasks

## 1. Source and target inventory

- [ ] 1.1 Confirm the supported Discourse backup/database versions from backup metadata and source schema fixtures; verify preview reports unsupported versions before target writes.
- [ ] 1.2 Inventory source tables for users, credentials, category hierarchy, topics/posts, PMs, polls, votes, bookmarks, uploads, and plugin data against Lemmy target models; record conversion and loss rules in importer documentation and verify each source entity is classified.
- [ ] 1.3 Define importer command options, source fingerprint, site selection, mapping conflict format, and report format; verify operator documentation shows a preview-only run and an explicit target-site selection.

## 2. Password verifier compatibility

- [ ] 2.1 Confirm Discourse PBKDF2 verifier encoding, digest, iterations, salt and test vectors for the supported source version; verify generated fixtures authenticate only for the expected passwords.
- [ ] 2.2 Add Argon2id hashing and verifier dispatch for Argon2id, existing bcrypt, and confirmed Discourse PBKDF2 formats; verify wrong passwords and malformed or unknown verifier formats fail without changing stored values.
- [ ] 2.3 Upgrade legacy bcrypt/PBKDF2 verifiers to Argon2id after successful authentication and use Argon2id for new/reset passwords; verify successful login preserves account and tenant identity while replacing only the verifier.
- [ ] 2.4 Document password compatibility, resource limits, and recovery behavior; verify existing bcrypt accounts continue to log in and new/reset credentials use Argon2id.

## 3. Archive inspection and mapping ledger

- [ ] 3.1 Add a Rust offline importer entry point that recognizes `.tar.gz` and `.sql.gz`, safely extracts supported backup members, restores the dump into an isolated source database, and enforces path/decompression limits; verify malformed and traversal archives are rejected before target writes.
- [ ] 3.2 Implement preview inventory, source-version checks, and operator mapping decisions for account conflicts and restricted categories; verify preview reports counts and warnings without changing the target database.
- [ ] 3.3 Add a durable source-fingerprint and target-site-scoped mapping/progress ledger with transactional idempotent updates; verify rerunning a completed phase reuses mappings and an interrupted phase resumes without duplicates.
- [ ] 3.4 Document backup handling, temporary storage requirements, required database access, and cleanup; verify every documented command matches the CLI help output.

## 4. Accounts and category structure

- [ ] 4.1 Import site-owned Person and LocalUser records with deterministic username/email conflict handling, account flags, compatible password verifier, and preserved timestamps where supported; verify every source user maps to exactly one target account or a reported blocking conflict.
- [ ] 4.2 Import categories as communities and parent categories as multi-communities containing descendant leaf communities; preserve categories that contain both topics and children as communities too; verify ledger relationships and presence rows for each created object.
- [ ] 4.3 Report source visibility/posting ACLs without applying unsupported restrictions; verify preview and final report identify each affected category and make no claim that its original ACL is enforced.

## 5. Discussions and interactions

- [ ] 5.1 Import topics and first posts, then posts/comments in parent-before-child order with source timestamps and reply mappings; verify topic counts, post ownership, and comment parent relationships against source references.
- [ ] 5.2 Import private messages only after both participants map, preserving site ownership, sender/recipient visibility, chronology, and media references; verify unrelated site accounts cannot read imported messages.
- [ ] 5.3 Import supported post/comment votes, bookmarks, and metadata while recording unsupported reactions, poll ballots, edit history, badges, and plugin records; verify aggregate counts and loss report match source inventory.
- [ ] 5.4 Render Discourse polls as labeled historical prompt/options/results on imported content and retain per-vote detail in the migration report where no native poll model exists; verify output distinguishes historical results from an interactive Lemmy poll.
- [ ] 5.5 Ensure import-specific writes create required local-site presence, aggregates, media references, and actor identifiers without notifications or outbound federation activities; verify no outbound delivery records are created and federation remains disabled.

## 6. Media and legacy links

- [ ] 6.1 Import images through pict-rs/media helpers and rewrite source references to target aliases; route non-image attachments through generic file storage when access rules permit and otherwise retain a manifest; verify every rewritten reference resolves or is listed as unresolved.
- [ ] 6.2 Generate a configurable legacy URL mapping file from source/target IDs for topic and reply URLs, and document a reverse-proxy include that accepts the old hostname from operator configuration; verify the emitted destinations match frontend post routes without adding source-host strings to frontend assets.

## 7. Completion validation and operations

- [ ] 7.1 Add final referential and count validation for users, categories, topic starters, replies, PM participants, votes, bookmarks, and attachments; verify unresolved required references prevent completion and appear in the durable report.
- [ ] 7.2 Document target backup, write pause, import, validation, redirect activation, rollback, and source-archive retention steps; verify the runbook explicitly keeps federation disabled through import.
- [ ] 7.3 Verify a representative archived fixture end to end, including interruption/resume, account login and verifier upgrade, private-message isolation, media resolution, URL redirects, and zero federation delivery; record the resulting import report and known losses.
