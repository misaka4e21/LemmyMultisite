## Context

See proposal.md for the motivation. Username and community creation currently use the same ActivityPub actor-name validator. Site settings already provide the federation flag to request handlers. Community edit currently changes presentation and policy fields but not the community's unique name; account settings likewise do not expose username changes.

## Goals / Non-Goals

**Goals:**
- Apply site federation policy consistently to local username and community name creation and changes.
- Keep ActivityPub-compatible validation as the single rule for federated sites.
- Prevent enabling federation until all local usernames and community names meet that rule.
- Enforce ownership and site-scoped moderation permissions for name changes.

**Non-Goals:**
- Change validation for community tags, multi-communities, display names, or remote actors.
- Rename remote actors or migrate identities between sites.
- Change name length, uniqueness, or case-folding rules.

## Decisions

- Keep `is_valid_actor_name` as the strict ActivityPub rule. Add a policy-aware local-name validation path: federation-enabled sites use the strict rule; non-federated sites may use Han characters together with ASCII letters, digits, and underscores. Both paths retain the existing 2–20 UTF-16 code-unit length limits and prohibit whitespace and other punctuation. This avoids weakening actor validation globally while supporting Chinese names.
- Pass the request site's `federation_enabled` value to local account registration, community creation, and both name-change operations. A username change is authorized for the account owner or a site administrator. A community name change follows the existing community edit authorization for moderators and site administrators.
- Treat a community's unique name as a new editable field in the existing community edit API. Keep its current actor ID and key material stable during a rename; only the local name/handle changes. Since the operation is allowed only while federation is disabled, no rename activity is sent to other instances.
- Before saving a false-to-true federation transition, query local Person and Community rows owned by the current LocalSite, validate each name with the strict actor-name rule, and reject the update with a useful list of incompatible actor type and name. Do not change the setting or invalidate the tenant cache on rejection.
- Perform the enablement check and settings update in a transaction or equivalent concurrency-safe operation so a simultaneous local rename/create cannot introduce an incompatible name between validation and enabling federation. Local creation and rename paths must observe the same site setting and database serialization boundary.

## Risks / Trade-offs

- [Existing deployments may already contain names that fail the actor-name rule] → The federation-enable response lists the incompatible local usernames and community names; administrators can rename them while federation remains disabled.
- [Changing a name while retaining its actor ID can leave an old name in the persisted actor URL] → Keep actor identity stable to avoid changing references and keys; verify local actor lookup and account/community responses expose the new name consistently.
- [Unicode has visually confusable characters] → Limit the non-federated extension to Han characters plus the existing ASCII letters, digits, and underscore; preserve length, uniqueness, and other safety checks.
