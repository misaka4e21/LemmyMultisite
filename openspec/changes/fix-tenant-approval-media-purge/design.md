## Context

Application ownership is reached through `local_user`; the current decision handler updates by global ID before reading the site-scoped view. Media objects are shared by content hash and have site references. Purge helpers delete canonical rows, while user cleanup runs asynchronously and contains global queries. See the proposal and delta specs for the required behavior.

## Goals / Non-Goals

**Goals:**

- Make authorization precede database, storage, email, queue, and federation side effects.
- Keep reference creation and final deletion safe under concurrent requests and storage failures.
- Preserve cross-site dependencies throughout synchronous and background purge work.

**Non-Goals:**

- Redesign installation administration, federation policy, or generic file uploads.
- Repair historical unauthorized changes or define a new global administrator purge API.

## Decisions

### Registration decisions

Resolve the request tenant and verify the administrator belongs to it. Inside one transaction, select and lock the application through its site's local user, then update both records using site-constrained predicates. Send email only after commit, using the verified site's settings. A scoped read after a global update is insufficient because the transaction has already committed.

### Media deletion

Give site administrators a site-scoped reference removal operation. Serialize reference creation and last-reference deletion by media identity, with a durable deletion state or equivalent protocol that also covers the external pict-rs call. New uploads must wait or safely retry while deletion is in progress. Retain retryable cleanup state after a storage error. Review every caller of low-level alias deletion, including profile and community images, so none can bypass the protocol. A reference count followed by an unlocked network call permits an upload to acquire an alias being deleted.

### Permanent deletion

Check ownership and the full deletion graph before logging, federation, or task submission. Preserve protected canonical rows and site state when possible. If a cascade or global mutation cannot safely preserve another site's content or state, reject the purge before side effects. Perform dependency checks and mutations with transaction isolation or locking that prevents new protected dependencies from appearing between them. Keep the existing origin authorization checks.

Pass the verified site explicitly to asynchronous cleanup and recheck authority in the worker. Replace global creator and moderation cleanup with site-scoped operations, or reject when preserving protected relationships is not possible. Audit activity generation as well as database changes. Shared account-deletion helpers have other callers; preserve their intended semantics through explicit, separately authorized paths.

## Risks / Trade-offs

- [Protected dependencies can prevent an otherwise valid purge] → Return a clear failure and leave all state unchanged; later work can add safe tombstones or dependency migration.
- [Database and pict-rs cannot share a transaction] → Use durable deletion state and retry, and test crashes and storage failures around each transition.
- [New references can race with dependency checks] → Lock relevant rows or use an equivalent serialization mechanism and test concurrent operations.
- [Existing global helpers serve other workflows] → Audit callers and add focused regression coverage before changing their behavior.

## Migration Plan

Add any required durable cleanup state with a reversible migration and a safe default for existing media. Deploy the migration before enabling the new deletion path; retry pending cleanup after restart. Roll back application code and schema only after resolving pending deletions. No historical data repair is included.
