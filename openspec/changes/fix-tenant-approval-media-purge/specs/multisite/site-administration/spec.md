## ADDED Requirements

### Requirement: Registration decisions are authorized before side effects
The system SHALL verify that the request site, administrator account, and registration application belong to the same site before changing an application or account. Approval and denial SHALL update the application and account atomically. The system MUST NOT send a decision email for a rejected or failed decision.

#### Scenario: Another site's application
- **WHEN** a site A administrator approves or denies a site B application by ID
- **THEN** the request is rejected
- **AND** the application and account remain unchanged and no decision email is sent

#### Scenario: Valid decision
- **WHEN** a site A administrator approves or denies a site A application
- **THEN** the application and account reflect the decision together
- **AND** any decision email uses site A's configuration

#### Scenario: Request site differs from administrator site
- **WHEN** an administrator submits a registration decision through a different site
- **THEN** the request is rejected before any update or email

#### Scenario: Decision update fails
- **WHEN** either database update fails
- **THEN** neither update is committed and no decision email is sent
