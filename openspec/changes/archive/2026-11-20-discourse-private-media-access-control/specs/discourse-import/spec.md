# Spec Delta

## MODIFIED Requirements

### Requirement: Discussion and interaction migration
The importer SHALL migrate topics, posts, reply relationships, private messages, votes, bookmarks, and uploads when a safe Lemmy representation exists, preserving source timestamps and source-to-target identifiers where the target model permits. Recorded optimized-image paths SHALL resolve to the imported original image when the target has no matching rendition model.

#### Scenario: Import topic replies
- **WHEN** a topic contains posts with reply references
- **THEN** the topic starter becomes a Lemmy post and replies become comments with the corresponding parent relationships

#### Scenario: Import a private message
- **WHEN** a supported source private message has migrated sender and recipient accounts
- **THEN** it is imported as a site-owned Lemmy private message with the same participants and chronology, and a read notification row makes it available in Lemmy's message history view
- **AND** the importer does not invoke notification hooks, send email, or submit federation activities

#### Scenario: Import poll or unsupported interaction data
- **WHEN** source poll structure or another interaction has no equivalent target model
- **THEN** the importer preserves recoverable poll prompt, options, and aggregate results in an explicitly labeled representation or export report and records any omitted semantics
- **AND** it exposes aggregate counts in public imported content only when source result visibility and poll state allowed public results at migration time
- **AND** it keeps open `on_vote` counts and all `staff_only` results out of public content, while honoring an elapsed poll `close_at` for public result modes
- **AND** it reports unsupported portions of Chat data while marking Chat message/channel tables as partially imported

#### Scenario: Import public non-image attachments when target policy allows
- **WHEN** an archived non-image upload is public, its owner is mapped, generic file storage is configured, and its detected MIME type and size satisfy the selected site's upload policy
- **THEN** the importer stores it in site-scoped generic file storage and rewrites its references in public imported content
- **AND** it retains missing, disallowed, or ownerless files in the upload manifest without making them public

#### Scenario: Import private-message media with participant access
- **WHEN** an archived image or document is referenced by an imported private message, including a supported direct-message Chat message, and its referenced users map to target accounts
- **THEN** the importer stores the media and associates it with the corresponding target private message
- **AND** a download request is allowed only when the current requester is a participant who can currently read that target message
- **AND** group-message fan-out copies authorize only their respective sender and recipient

#### Scenario: Import post-restricted uploads with live post access
- **WHEN** an archived image or document is marked secure or references a Discourse post through `access_control_post_id`, and an equivalent target post with enforceable current visibility exists
- **THEN** the importer stores the media and associates it with that target post
- **AND** each download request is allowed only when the requester can currently view the associated target post
- **AND** permission changes to the target post take effect on the next media request without rerunning the import

#### Scenario: Protected upload has no equivalent target authorization
- **WHEN** a protected upload's source access rule cannot be mapped to an enforceable target message or post authorization rule
- **THEN** the importer records the upload and authorization mapping as unresolved
- **AND** it does not expose the file through a public URL or mark the media phase complete

#### Scenario: Import direct-message Chat history
- **WHEN** the source archive has Discourse Chat direct-message channels and messages
- **THEN** the importer copies messages between currently mapped channel participants into site-owned Lemmy private messages, fans group-channel messages out per recipient, and reports unresolved messages and unsupported channel/thread semantics
- **AND** it reports that departed channel members cannot be reconstructed from Discourse's current membership table
- **AND** preview marks Chat message and channel tables as partially imported rather than fully supported or wholly unsupported

### Requirement: Migration report and validation
The importer SHALL produce a durable report of imported, skipped, transformed, and failed source records and SHALL validate key counts and relationships before marking an import complete.

#### Scenario: Validation finds missing references
- **WHEN** a source reply, message, vote, or attachment refers to an unmapped source record
- **THEN** the importer records the unresolved reference and does not mark the import complete

#### Scenario: Validation finds missing generic file objects
- **WHEN** an imported generic attachment has a missing database reference or its S3 object cannot be verified
- **THEN** validation reports the attachment failure and does not mark the import complete

#### Scenario: Validation finds a failed phase with zero counted source rows
- **WHEN** an import phase fails because its source schema is incomplete, even if source row counts are zero
- **THEN** validation reports the failed phase and does not mark the import complete

#### Scenario: Validate protected media authorization mappings
- **WHEN** an imported private-message or post-restricted media reference has no matching target message/post association or its backing object cannot be verified
- **THEN** validation reports the unresolved media or authorization mapping and does not mark the import complete

#### Scenario: Import completes
- **WHEN** all configured import phases finish and required integrity checks pass
- **THEN** the report records completion and source-to-target mappings for audit and reruns

### Requirement: Configurable legacy URL mapping
The migration SHALL provide configurable source URL patterns and target URL mappings that can be used by a reverse proxy to redirect old Discourse topic and post URLs to LemmyMultisite frontend routes without embedding a Discourse hostname in frontend assets.

#### Scenario: Redirect a legacy topic URL
- **WHEN** a request matches a configured legacy topic or post URL
- **THEN** the deployment mapping redirects it to the corresponding imported Lemmy post route
- **AND** a constant-size slug-agnostic regex map extracts only numeric topic and post identifiers before exact lookup so historical slugs continue to work without per-post regex growth

#### Scenario: Redirect a legacy last-post URL
- **WHEN** a request matches a Discourse ID-only or current-slug `/last` route
- **THEN** the mapping redirects it to the last successfully imported post or reply in that topic

#### Scenario: Change the source hostname
- **WHEN** an operator changes the configured legacy source hostname
- **THEN** the mapping can be regenerated or updated without rebuilding the frontend
