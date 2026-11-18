# Verification record

The user did not provide a backup archive, so verification used a synthetic archive built from the inspected Discourse 2026.7.4 schema and supported database version `20261014065735`. It included three users with PBKDF2 credentials; a parent and leaf category; two public topics and a reply; a two-person private-message thread; poll options and votes; likes, bookmarks, tags; and a PNG referenced by a post. The archive was restored into a fresh isolated PostgreSQL source database before the final import validation.

## Results

- Archive preview recognized the supported Discourse and database versions and found one archived upload.
- Import created three accounts, two communities, one multi-community, two topics, one reply, two private-message copies, two likes, two bookmarks, two topic tags, and one image mapping.
- The imported PBKDF2 credential accepted its expected password and rejected a wrong password. The conditional password-upgrade helper used by the login handler changed the verifier to Argon2id and preserved the local-user ID.
- A run recorded as failed after its posts were committed resumed all three post mappings without making duplicate posts or replies.
- Both imported private messages had presence edges for the selected target site only. No private-message edge was visible to a different site ID.
- The PNG upload passed through a local pict-rs protocol stub. Lemmy stored its returned alias, created a post media reference, and rewrote the post body to a site-configured `/api/v4/image/{alias}` URL. Validation found no unresolved media references.
- The generated proxy map resolved the topic starter, reply, and last-post routes to `/post/{post_id}` and `/post/{post_id}/{comment_id}` frontend paths.
- Federation remained disabled. The target had zero `sent_activity` rows for the site and zero `internal_activity_delivery` rows.
- Final validation completed with no blocking reasons or invalid target references. It counted three users, two communities, one multi-community, two topics, one reply, two private-message copies, two likes, two bookmarks, two topic tags, and one upload.

## Limits of this fixture

This fixture does not replace testing against the operator's actual archive. It did not include Discourse Chat tables, non-image or secure attachments, category ACL data, or plugin-specific rows. Polls are preserved as frozen historical results rather than interactive Lemmy polls. The media test used a protocol stub, so it verifies importer-to-pict-rs request/response handling and database references, not a running pict-rs image-serving deployment. The credential test exercised the imported verifier and the same conditional upgrade helper called by the login handler; it did not send an HTTP login request through a running Lemmy server.
