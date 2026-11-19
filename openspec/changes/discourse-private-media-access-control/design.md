# Design

## Context

See proposal.md for motivation and specs/discourse-import/spec.md, specs/multisite/file-storage/spec.md, and specs/multisite/media-storage/spec.md for the behavior contract. Today `file_reference` has no resource association, `media_reference` can be associated with a post but not a private message, and the document and image download routes enforce only site-level policy. The importer rewrites public discussion media but records private-message upload references as unresolved.

## Goals / Non-Goals

**Goals:**
- Associate protected files and images with the target private message or post whose current access rules govern them.
- Authorize each download using the target resource's current read permissions; never persist a migration-time user allowlist.
- Keep imported protected media unavailable until its target resource association and authorization check are ready.
- Preserve the current behavior for public media and legacy references.

**Non-Goals:**
- Reimplement Discourse's permission engine in the media endpoints.
- Make an unprotected target post restricted solely by attaching a protected file to it. A source restriction must have an enforceable target equivalent or remain unresolved.
- Synchronize target authorization back to a running Discourse instance.

## Decisions

1. **Bind each protected reference to one target resource.** Extend file and media reference metadata so a protected use points to exactly one target private message or post. Keep the existing site and media/file reference model, content-addressed storage, and public identifiers. Nullable resource bindings mean existing references remain unprotected and retain current behavior. Reject inconsistent bindings that name both resource types.

   Alternative considered: store a copied list of source-authorized users. Rejected because it becomes stale when participants, bans, community membership, or other target permissions change, and would violate request-time authorization.

2. **Use the target resource's normal read authorization for every request.** Resolve the reference's resource binding before serving bytes. For private messages, check the requester's current ability to read the associated target message. For posts, check current ability to view the associated target post. Apply the same check for original images, processed image variants, and document downloads. Return the normal not-found/unauthorized response without contacting storage when access fails.

   Alternative considered: authorize only when rendering message/post content. Rejected because media URLs can be copied and requested directly after permissions change.

3. **Import only when the source access rule maps to an enforceable target resource.** For private-message posts and supported direct-message Chat copies, map each attachment to the exact target message copy; group fan-out uses each recipient-specific copy. For `access_control_post_id`, map to the imported target post. If a source rule has no target resource with equivalent live visibility, leave the media unavailable, report the unresolved mapping, and fail the media phase rather than expose it publicly.

4. **Keep association updates resumable and auditable.** Store source-to-target upload mappings as today, and make resource bindings idempotent with the import ledger. Validation checks that every protected source reference has a verified stored object and exactly one valid target resource binding. A rerun must repair or reuse the same binding without broadening access.

## Risks / Trade-offs

- [The target post may not enforce a Discourse category/group restriction because category ACLs are currently reported as unsupported] → Do not treat a public target post as equivalent to a restricted source post; report it unresolved and block completion until an enforceable target visibility rule exists.
- [Adding authorization lookups to media requests adds database work and may affect cached image delivery] → Keep public-reference requests on their existing fast path; cache no positive authorization result across requests.
- [A deployment rollback to code without these checks could expose protected references] → Deploy database and server authorization support before importing protected references; rollback must first disable or remove those references, or keep the protected-media-capable server version active.
- [Pictrs aliases may be reachable through deployment-specific direct routes] → Ensure externally served image requests pass through the Lemmy authorization route; document and verify the supported proxy topology.

## Migration Plan

1. Add nullable resource-binding fields and indexes for protected file and media references, preserving all existing rows and URLs.
2. Deploy the authorization-aware document and image download handlers. Unbound existing references continue to follow current public/site policy.
3. Run the importer against the supported archive. It stores media, binds it to mapped messages/posts, and rewrites message content only after the protected association is in place.
4. Validate object presence, source-to-target mappings, and protected resource bindings before enabling normal site writes or redirects.
5. For rollback, pause writes and remove or disable protected references before deploying code that lacks request-time checks; restore database and blob storage from a matched backup if reverting the import.
