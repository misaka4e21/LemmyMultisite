# Spec Delta

## ADDED Requirements

### Requirement: Site administrator can search the site user list
The admin user list SHALL support case-insensitive partial matching against a local user's username or email while retaining current-site filtering and cursor pagination.

#### Scenario: Search by username or email
- **WHEN** a site administrator searches with a fragment matching a local user's username or email regardless of letter case
- **THEN** the site user list includes that account

#### Scenario: Search is tenant-scoped
- **WHEN** a site administrator searches for a username or email belonging only to another site's local account
- **THEN** that account is absent from the result

#### Scenario: Search results use pagination
- **WHEN** a search matches more local accounts than the requested page size
- **THEN** the API returns the matching accounts across pages using its cursor pagination

### Requirement: Site administrator can read a site user's details
The admin user detail endpoint SHALL require a site administrator and SHALL return details, including administrator status, only for a local account owned by the current site.

#### Scenario: Authorized detail read
- **WHEN** a site administrator reads a local account owned by the current site
- **THEN** the API returns that account's user details and administrator status

#### Scenario: Non-administrator detail read
- **WHEN** a non-administrator requests the admin user detail endpoint
- **THEN** the API rejects the request as unauthorized

#### Scenario: Cross-site detail read
- **WHEN** a site administrator requests details for a local account owned by another site
- **THEN** the API returns NotFound and does not disclose the account details
