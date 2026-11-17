# Design

## Context

Non-federated usernames currently use a dedicated validation expression that permits Han characters, Latin letters, digits, and underscores. Federated sites use the separate ActivityPub actor-name validator. Login resolves the supplied username or email within the request tenant and has no character-set validation of its own.

## Goals / Non-Goals

**Goals:**

- Add Japanese hiragana, katakana, and Korean Hangul to the non-federated username character ranges.
- Keep the existing validation behavior for federated usernames and retain current length and uniqueness checks.
- Cover registration, rename, and login behavior with focused tests.

**Non-Goals:**

- Changing community-name validation.
- Changing login lookup, normalization, case folding, or database schema.
- Allowing mixed scripts beyond what the existing non-federated policy permits.

## Decisions

- Extend only the non-federated username validator with Unicode script ranges/properties for Hiragana, Katakana, and Hangul. Keep the existing Han/Latin/digit/underscore set intact. This matches the established site-specific exception; changing the shared federated validator would affect ActivityPub actor compatibility.
- Keep login lookup unchanged because it already searches the tenant-scoped username directly. Add a focused test proving a stored kana or Hangul username can authenticate, so future changes cannot introduce a conflicting restriction.
- Keep community name validation out of scope because the request concerns usernames used for account login.

## Risks / Trade-offs

- [Unicode script-property ranges may include characters beyond everyday kana or Hangul letters] → Select and test the intended Unicode ranges explicitly, while retaining all existing common username constraints.
- [A narrower validator change could accidentally alter existing Chinese username behavior] → Add regression cases for existing Chinese usernames alongside the new scripts.
