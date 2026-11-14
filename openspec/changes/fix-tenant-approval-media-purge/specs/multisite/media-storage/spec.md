## MODIFIED Requirements

### Requirement: Reference-counted deletion
Deleting media SHALL remove only the current site's reference. Site administrators SHALL have authority only over references belonging to their current site. The system SHALL delete the pict-rs alias only when no reference remains, including during concurrent uploads and deletions. Installation administrator purge MAY remove all references and the media object. A failed storage deletion SHALL remain recoverable without deleting another site's reference.

#### Scenario: Delete one site's reference
- **WHEN** site A deletes an image that site B still uses
- **THEN** site B's image remains available
- **AND** the pict-rs alias remains

#### Scenario: Last reference removed
- **WHEN** the last reference to a media object is deleted
- **THEN** the system removes the pict-rs alias and the media object

#### Scenario: Administrator deletes another site's image
- **WHEN** site A's administrator deletes an alias with no site A reference
- **THEN** the request is rejected and site B's references and image remain unchanged

#### Scenario: Concurrent upload and last-reference deletion
- **WHEN** an upload creates a reference while another request removes the last existing reference
- **THEN** any successfully created reference resolves to available media

#### Scenario: Storage deletion fails
- **WHEN** deleting the last alias fails at the storage service
- **THEN** the system retains recoverable cleanup state
- **AND** no other site's reference is removed
