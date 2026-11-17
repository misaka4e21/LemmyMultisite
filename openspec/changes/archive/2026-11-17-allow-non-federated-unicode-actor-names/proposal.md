## Why

Sites that do not participate in federation have no interoperability need to restrict local usernames and community names to ActivityPub actor-name characters. The current shared validation prevents these sites from using Chinese names and from changing existing names to Chinese.

## What Changes

- Allow Chinese characters in local usernames and community names while the site's federation is disabled.
- Allow a user to change their own username and an authorized community moderator to change a community name under the same non-federated policy.
- Reject enabling federation when any local username or community name violates the existing ActivityPub actor-name rules, and identify the names administrators must resolve.
- Preserve the existing actor-name validation for federated sites.

## Capabilities

### New Capabilities

### Modified Capabilities
- `multisite/local-identity`: Make local username validation depend on site federation policy, including account username changes.
- `multisite/federation`: Permit non-actor-compatible local names only while federation is disabled, and validate local usernames and community names before enabling federation.

## Impact

- Account registration and username update APIs.
- Community creation and community name update APIs, including authorization checks for moderators.
- Site configuration update flow for federation enablement and administrator feedback.
- Shared actor-name validation and its API tests.
