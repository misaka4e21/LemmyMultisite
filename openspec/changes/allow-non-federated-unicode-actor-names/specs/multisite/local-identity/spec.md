## ADDED Requirements

### Requirement: Non-federated local usernames
When federation is disabled for a site, the system SHALL allow a local account to register or change its username to a name containing Chinese characters, subject to the existing username length, uniqueness, and safety constraints. A user SHALL be able to change their own username, and a site administrator SHALL be able to change a local account's username. When federation is enabled, registration and username changes SHALL continue to enforce the ActivityPub actor-name rules.

#### Scenario: Register with a Chinese username while federation is disabled
- **WHEN** a user registers on a site with federation disabled using a valid Chinese username
- **THEN** registration succeeds if the username is available and satisfies the site's other account requirements

#### Scenario: Change own username while federation is disabled
- **WHEN** a user changes their own username on a site with federation disabled to a valid Chinese username
- **THEN** the username is updated

#### Scenario: Administrator changes a username while federation is disabled
- **WHEN** a site administrator changes a local account's username to a valid Chinese username while federation is disabled
- **THEN** the username is updated

#### Scenario: Reject non-actor username on a federated site
- **WHEN** a user registers or changes a username on a site with federation enabled
- **AND** the username violates the ActivityPub actor-name rules
- **THEN** the operation is rejected with a validation error
