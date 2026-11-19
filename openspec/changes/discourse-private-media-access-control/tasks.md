# Tasks

## 1. Protected reference model

- [ ] 1.1 Add nullable private-message and post authorization bindings for file and image references, with constraints and indexes that prevent conflicting scopes; verify fresh and upgrade migrations preserve existing unbound references.
- [ ] 1.2 Add database access helpers to resolve a reference's protected target resource and verify that public references keep their existing behavior; verify duplicate or invalid bindings are rejected.

## 2. Request-time file and image authorization

- [ ] 2.1 Enforce current private-message read access and current post visibility in generic file downloads before reading from S3; verify authorized users receive bytes and unauthorized or anonymous users receive none.
- [ ] 2.2 Enforce the same checks for original and processed image requests before proxying to pict-rs; verify all image variants deny unauthorized requests and existing public image requests still work.
- [ ] 2.3 Update file and media storage documentation for protected reference behavior and verify it describes request-time checks, denial behavior, and compatibility for existing references.

## 3. Discourse protected-media import

- [ ] 3.1 Map ordinary private-message uploads and supported Chat direct-message attachments to the correct imported target message; verify group fan-out copies bind only to their own sender/recipient conversation.
- [ ] 3.2 Import secure and post-restricted uploads only when an enforceable target authorization resource exists, and bind each reference before rewriting content; verify unmappable protected media stays inaccessible and fails the media phase.
- [ ] 3.3 Extend resumable mappings, reports, and validation for protected media objects and resource bindings; verify reruns do not duplicate or broaden access and missing associations block completion.
- [ ] 3.4 Update the Discourse import runbook with the protected-media requirements and deployment order; verify documented commands and rollback instructions match the implemented flow.

## 4. End-to-end authorization verification

- [ ] 4.1 Verify private-message image and document access for current participants, nonparticipants, and users whose message access changes after import.
- [ ] 4.2 Verify restricted-post image and document access tracks current post visibility changes and that an unmapped source restriction cannot become a public target link.
- [ ] 4.3 Verify public media, private-site behavior, tenant isolation, and import completion remain correct across the complete migration and validation flow.
