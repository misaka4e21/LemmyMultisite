## ADDED Requirements

### Requirement: Permanent deletion respects ownership and shared references
A site administrator SHALL permanently purge only objects owned by the current site. Purging or banning SHALL NOT delete or modify another site's local objects, shared canonical objects, other sites' state, or media still referenced by another site. Site-local moderation of shared objects SHALL change only the acting site's state unless an authorized origin action applies.

#### Scenario: Purge another site's content
- **WHEN** site A's administrator attempts to purge a person, community, post or comment owned by site B
- **THEN** the request is rejected and the object and associated data remain unchanged

#### Scenario: Moderate a shared remote object
- **WHEN** site A applies a permitted local moderation action to a shared remote object
- **THEN** site A's state changes without deleting or altering the canonical object or site B's state

#### Scenario: Shared media
- **WHEN** an authorized purge removes an eligible site A object whose media is still referenced by site B
- **THEN** site B's media remains available
