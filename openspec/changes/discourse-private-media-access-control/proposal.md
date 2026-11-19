# Proposal

## Why

The Discourse importer currently skips media referenced by private messages and uploads protected by post access controls because Lemmy's file and image endpoints do not enforce access at that granularity. Importing those bytes without equivalent checks would expose private content, while skipping them loses conversation history and attachments.

## What Changes

- Import private-message images and documents and associate them with the imported message.
- Import secure and post-restricted uploads while preserving their access rules through checks against the target site's current authorization state at every request.
- Deny access when a requester is not a current participant in the private message or cannot currently view the associated post.
- Extend import reporting and validation to verify protected media mappings and fail on unresolved content or authorization relationships.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `discourse-import`: import private and post-restricted media with request-time authorization and validate protected references.
- `multisite/file-storage`: enforce message- and post-scoped access control on document downloads.
- `multisite/media-storage`: enforce message- and post-scoped access control on image downloads.

## Impact

Expected changes affect the Discourse import media phase and its validation/reporting, file and image download authorization, media/file reference data, and database migrations. Existing public media URLs and site-level private-instance checks must continue to work.
