## MODIFIED Requirements

### Requirement: Site media reference
Each use of media SHALL have a reference row with `local_site_id`, media object, owner, and purpose. The system SHALL track local media used in persisted content for each site where that content is present, and SHALL reconcile those references when content or site presence changes. Existing content uses SHALL be backfilled before final media deletion is enabled. Unclassifiable historical uses SHALL be retained conservatively. A site's media list SHALL return only its own references.

#### Scenario: List media
- **WHEN** site A lists uploaded media
- **THEN** the response contains site A's references
- **AND** it does not contain site B's references

#### Scenario: Embedded image on another site
- **WHEN** site B content uses a known local media URL
- **THEN** site B has a reference to the media for that content
- **AND** removing site A's upload reference does not delete the image
- **AND** the original site A URL continues to work under site A's access policy while the content reference remains

#### Scenario: Guessed alias on an unrelated site
- **WHEN** content names a local image URL on a site that was not already allowed to serve that alias
- **THEN** creating the content reference does not grant that site access to the media

#### Scenario: Edit removes an embedded image
- **WHEN** content is edited to remove a local media URL
- **THEN** the corresponding content reference is removed
- **AND** other references remain unchanged

#### Scenario: Existing content backfill
- **WHEN** the installation upgrades with existing content containing local media URLs
- **THEN** the system records its site-specific uses before final media deletion is enabled
- **AND** rerunning the backfill does not duplicate references

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
