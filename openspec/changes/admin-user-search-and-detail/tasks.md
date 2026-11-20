# Tasks

## 1. User list search

- [x] 1.1 Review and complete the local-user query predicate for case-insensitive partial username and email matching; verify username, email, mixed-case, empty-term, and no-match cases with the local-user view tests.
- [x] 1.2 Add matching accounts owned by separate sites and verify a search only returns the active site's accounts.
- [x] 1.3 Add a multi-page search case and verify the cursor returns all matching accounts once, in stable order, while banned filtering remains applied.

## 2. Admin user detail

- [x] 2.1 Verify the admin detail route returns the requested current-site `LocalUserView`, including the target account's admin flag, using an API handler test.
- [x] 2.2 Verify a non-admin receives the standard not-admin error and a site admin requesting another site's account receives NotFound; assert neither response contains the target account.
- [ ] 2.3 Verify the route is registered alongside `/users` and run the focused API crate checks for the new handler tests.
