# Proposal

## Why

Non-federated sites already support Chinese usernames, but the current allowed character ranges exclude Japanese kana and Korean Hangul. Users choosing those scripts cannot register or rename accounts with their native writing system, even though these sites do not need ActivityPub-compatible actor names.

## What Changes

- Extend the non-federated local username capability to support Japanese hiragana, katakana, and Korean Hangul while retaining current length, uniqueness, and safety constraints.
- Specify that an account with such a username can log in on its owning non-federated site.
- Preserve ActivityPub actor-name validation for federated sites.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `multisite/local-identity`: Extend non-federated local username registration and change behavior to include kana and Hangul, and require login by those usernames.

## Impact

The shared username validation utility and its tests are affected. Login already looks up a username or email within the tenant and does not validate the username's character set; the change documents and verifies that behavior for the newly supported scripts. No API shape or database change is expected.
