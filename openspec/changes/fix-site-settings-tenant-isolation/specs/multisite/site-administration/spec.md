## ADDED Requirements

### Requirement: Existing account exemptions are site-scoped
When a site changes from not requiring email verification to requiring it, the system SHALL mark existing local accounts owned by that site as email verified. When a site changes from not requiring registration applications to requiring them, the system SHALL mark existing local accounts owned by that site as having accepted applications. These changes MUST NOT alter accounts owned by another site. New registrations SHALL remain subject to the site's current policy.

#### Scenario: Enable email verification
- **WHEN** site A enables required email verification
- **THEN** existing site A accounts are marked email verified
- **AND** site B account verification states remain unchanged

#### Scenario: Enable registration applications
- **WHEN** site A enables required registration applications
- **THEN** existing site A accounts are marked as having accepted applications
- **AND** site B account application states remain unchanged

#### Scenario: Policy remains enabled
- **WHEN** site A edits settings without transitioning either policy from disabled to enabled
- **THEN** the edit does not grant the corresponding exemption to its accounts

#### Scenario: New account after policy change
- **WHEN** an account registers on site A after the relevant policy is enabled
- **THEN** the account is subject to the site's verification and application policy
