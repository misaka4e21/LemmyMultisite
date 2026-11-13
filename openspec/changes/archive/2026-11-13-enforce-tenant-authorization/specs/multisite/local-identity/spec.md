## ADDED Requirements

### Requirement: OAuth providers belong to the login site
OAuth authorization, callbacks, account linking and login SHALL use only providers owned by the request site. A provider ID from another site MUST NOT authenticate, register or link an account on the current site.

#### Scenario: Cross-site OAuth provider
- **WHEN** an OAuth login on site B supplies a provider owned by site A
- **THEN** site B rejects the attempt before exchanging credentials or creating or linking an account

#### Scenario: Same-site OAuth provider
- **WHEN** a user completes a valid OAuth login through an enabled provider owned by the current site
- **THEN** the existing registration and account-linking policy applies
