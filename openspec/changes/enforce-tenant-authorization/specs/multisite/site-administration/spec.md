## ADDED Requirements

### Requirement: Site-owned administrative configuration
OAuth providers, custom emoji and taglines SHALL belong to exactly one site. Create, list, site response, update and delete operations SHALL use the current site; modifying one site's configuration SHALL NOT change another site's configuration or cached responses.

#### Scenario: Cross-site configuration ID
- **WHEN** site A's administrator updates or deletes site B's OAuth provider, custom emoji or tagline by ID
- **THEN** the request is rejected and site B's configuration remains unchanged

#### Scenario: Site configuration listing
- **WHEN** a client reads site A's configuration or lists its emoji or taglines
- **THEN** it receives only site A's corresponding records

### Requirement: Administrative targets and reports are scoped
Administrator assignment, removal, hierarchy checks, site bans, registration actions, reports, report counts, bulk operations and purge SHALL be scoped to the current site and the action's target. Handling a report SHALL require authority over that report; a report's global ID alone MUST NOT grant access to its content.

#### Scenario: Assign another site's user
- **WHEN** site A's administrator attempts to grant or remove administrator status for a site B account
- **THEN** the system rejects the request and leaves site B's account unchanged

#### Scenario: Another site's private message report
- **WHEN** site A's administrator attempts to resolve or read a private message report outside site A's authority
- **THEN** the system rejects the request without returning the report or private message

#### Scenario: Report count
- **WHEN** an administrator reads the current site's report count
- **THEN** the count excludes reports outside that site's authority

#### Scenario: Federated report received by multiple sites
- **WHEN** a federated report is received by site A and site B
- **THEN** each receiving site has its own report association and resolution state
- **AND** resolving the report on site A does not resolve or expose site B's state

### Requirement: Existing configuration migration preserves ownership
Migration SHALL assign existing global configuration to one site per record. A single-site installation SHALL assign records to its only site. A multi-site installation SHALL require an explicit complete ownership mapping and SHALL reject migration when ownership or OAuth account associations are ambiguous or inconsistent; failed migration MUST leave existing data unchanged.

#### Scenario: Incomplete multi-site mapping
- **WHEN** a multi-site installation migrates with a missing or invalid configuration mapping
- **THEN** migration fails without assigning or copying the affected records

#### Scenario: Existing OAuth association
- **WHEN** an OAuth account belongs to a different site from its mapped provider
- **THEN** migration fails and reports the conflict for resolution
