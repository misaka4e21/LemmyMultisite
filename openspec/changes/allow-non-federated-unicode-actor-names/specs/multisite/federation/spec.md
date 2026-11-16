## ADDED Requirements

### Requirement: Local names must be actor-compatible before federation is enabled
Before enabling federation for a site, the system SHALL verify that every local username and local community name satisfies the ActivityPub actor-name rules. If any name does not satisfy those rules, the system MUST reject the configuration change and identify the offending names so an administrator can resolve them. Existing federated sites SHALL retain the current actor-name validation for newly created or renamed local actors.

#### Scenario: Enable federation when all local names are compatible
- **WHEN** an administrator enables federation and all local usernames and community names satisfy the ActivityPub actor-name rules
- **THEN** the federation setting is enabled

#### Scenario: Reject enabling federation with an incompatible username
- **WHEN** an administrator enables federation while a local username violates the ActivityPub actor-name rules
- **THEN** the federation setting remains disabled
- **AND** the response identifies the incompatible username

#### Scenario: Reject enabling federation with an incompatible community name
- **WHEN** an administrator enables federation while a local community name violates the ActivityPub actor-name rules
- **THEN** the federation setting remains disabled
- **AND** the response identifies the incompatible community name

### Requirement: Non-federated community names
When federation is disabled for a site, the system SHALL allow creation and renaming of a local community to a name containing Chinese characters, subject to existing community name length, uniqueness, and safety constraints. An authorized community moderator or site administrator SHALL be able to rename a local community. When federation is enabled, community creation and renaming SHALL continue to enforce the ActivityPub actor-name rules.

#### Scenario: Create a community with a Chinese name while federation is disabled
- **WHEN** an authorized user creates a local community on a site with federation disabled using a valid Chinese name
- **THEN** the community is created if the name is available and satisfies the site's other community requirements

#### Scenario: Rename a community while federation is disabled
- **WHEN** an authorized community moderator or site administrator renames a local community on a site with federation disabled to a valid Chinese name
- **THEN** the community name is updated

#### Scenario: Reject non-actor community name on a federated site
- **WHEN** a user creates or renames a community on a site with federation enabled
- **AND** the name violates the ActivityPub actor-name rules
- **THEN** the operation is rejected with a validation error
