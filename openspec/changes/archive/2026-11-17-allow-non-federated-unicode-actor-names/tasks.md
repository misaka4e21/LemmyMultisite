## 1. Validation policy

- [x] 1.1 Add a policy-aware local actor-name validator that accepts the agreed Han/ASCII character set only when federation is disabled, preserving current length and safety constraints; verify it with focused validator tests.
- [x] 1.2 Keep strict actor-name validation for ActivityPub identifiers, community tags, multi-communities, and all federated local names; verify existing strict-validation tests still pass.
- [x] 1.3 Add validator coverage for Han-only names, Han mixed with ASCII letters/digits/underscores, disallowed punctuation/whitespace, length boundaries, and strict federated behavior; run the utils validation test target.

## 2. Local username registration and changes

- [x] 2.1 Apply the site-aware validator to password registration and OAuth registration through their shared person-creation path; verify password registration with an API test and confirm OAuth uses the same validated path.
- [x] 2.2 Add username-change behavior for the account owner and site administrators, with site-scoped uniqueness checks and tenant ownership enforcement; verify owner/admin permissions and duplicate-name rejection with API tests.
- [x] 2.3 Add API coverage for non-federated registration and username changes, federated rejection, unauthorized changes, and cross-site isolation; run the focused account API test target.

## 3. Local community names

- [x] 3.1 Apply the site-aware validator to community creation; verify Chinese names are accepted only when federation is disabled with API tests.
- [x] 3.2 Add community name changes to the edit API, restricted to local communities and existing moderator/site administrator authorization; preserve actor ID and key material, verified by community API tests.
- [x] 3.3 Add API coverage for non-federated Chinese community creation and renaming, federated rejection, permissions, and site scope; run the focused community API test target.

## 4. Federation enablement guard

- [x] 4.1 Before enabling federation, find current-site local usernames and community names that fail strict actor-name validation and return actionable details; verify the returned errors identify actor type and offending names.
- [x] 4.2 Make the validation and federation setting update safe against concurrent local name creation or changes; verify failed enablement leaves the setting disabled and site cache consistent with a tenant-scoped integration test.
- [x] 4.3 Add integration coverage for compatible names, incompatible usernames, incompatible community names, multiple incompatible names, and unchanged settings after rejection; run the focused site configuration integration test target.
