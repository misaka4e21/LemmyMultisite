## 1. Validation policy

- [ ] 1.1 Add a policy-aware local actor-name validator that accepts the agreed Han/ASCII character set only when federation is disabled, preserving current length and safety constraints; verify it with focused validator tests.
- [ ] 1.2 Keep strict actor-name validation for ActivityPub identifiers, community tags, multi-communities, and all federated local names; verify existing strict-validation tests still pass.
- [ ] 1.3 Add validator coverage for Han-only names, Han mixed with ASCII letters/digits/underscores, disallowed punctuation/whitespace, length boundaries, and strict federated behavior; run the utils validation test target.

## 2. Local username registration and changes

- [ ] 2.1 Apply the site-aware validator to password registration and OAuth registration using the request tenant's federation setting; verify both registration paths with API tests.
- [ ] 2.2 Add username-change behavior for the account owner and site administrators, with site-scoped uniqueness checks and tenant ownership enforcement; verify owner/admin permissions and duplicate-name rejection with API tests.
- [ ] 2.3 Add API coverage for non-federated registration and username changes, federated rejection, unauthorized changes, and cross-site isolation; run the focused account API test target.

## 3. Local community names

- [ ] 3.1 Apply the site-aware validator to community creation; verify Chinese names are accepted only when federation is disabled with API tests.
- [ ] 3.2 Add community name changes to the edit API, restricted to local communities and existing moderator/site administrator authorization; preserve actor ID and key material, verified by community API tests.
- [ ] 3.3 Add API coverage for non-federated Chinese community creation and renaming, federated rejection, permissions, and site scope; run the focused community API test target.

## 4. Federation enablement guard

- [ ] 4.1 Before enabling federation, find current-site local usernames and community names that fail strict actor-name validation and return actionable details; verify the returned errors identify actor type and offending names.
- [ ] 4.2 Make the validation and federation setting update safe against concurrent local name creation or changes; verify failed enablement leaves the setting disabled and site cache consistent with a tenant-scoped integration test.
- [ ] 4.3 Add integration coverage for compatible names, incompatible usernames, incompatible community names, multiple incompatible names, and unchanged settings after rejection; run the focused site configuration integration test target.
