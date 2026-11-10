# Media Storage Specification

## Purpose

Store uploaded files once by content hash while letting each site use its own hostname, access policy, and lifecycle.

## Requirements

### Requirement: Content-addressed media object
The system SHALL store one media object per content hash. The media object SHALL hold the canonical pict-rs alias, size, dimensions, content type, and blurhash. Uploading identical bytes SHALL reuse the existing media object.

#### Scenario: Same image uploaded twice
- **WHEN** a user on site A uploads an image
- **AND** a user on site B uploads the same bytes
- **THEN** the system stores one media object
- **AND** the two sites hold separate references

### Requirement: Site media reference
Each use of media SHALL have a reference row with `local_site_id`, media object, owner, and purpose. A site's media list SHALL return only its own references.

#### Scenario: List media
- **WHEN** site A lists uploaded media
- **THEN** the response contains site A's references
- **AND** it does not contain site B's references

### Requirement: Per-site media URL
The system SHALL render media URLs as `https://{site_hostname}/api/v4/image/{alias}`. The pict-rs backend SHALL be shared. Stored canonical object rows MUST NOT contain a site-specific media URL.

#### Scenario: Two sites serve the same alias
- **WHEN** site A and site B both reference the same media object
- **THEN** site A serves it at `https://a.example/api/v4/image/{alias}`
- **AND** site B serves it at `https://b.example/api/v4/image/{alias}`

### Requirement: Deduplicated upload
The upload path SHALL hash the incoming bytes before or while sending them to pict-rs. When the hash already exists, the system SHALL reuse the canonical alias and avoid creating a second stored file.

#### Scenario: Upload duplicate
- **WHEN** a user uploads bytes whose hash already exists
- **THEN** the system returns the canonical alias
- **AND** it creates only a new site reference

### Requirement: Reference-counted deletion
Deleting media SHALL remove only the current site's reference. The system SHALL delete the pict-rs alias only when no reference remains. Installation administrator purge MAY remove all references and the media object.

#### Scenario: Delete one site's reference
- **WHEN** site A deletes an image that site B still uses
- **THEN** site B's image remains available
- **AND** the pict-rs alias remains

#### Scenario: Last reference removed
- **WHEN** the last reference to a media object is deleted
- **THEN** the system removes the pict-rs alias and the media object

### Requirement: Remote image cache
Remote images SHALL be tracked by source URL at installation level. Image details SHALL be shared by source or media hash. Proxy URLs SHALL be rendered per site.

#### Scenario: Same remote image
- **WHEN** site A and site B proxy the same remote image
- **THEN** the system stores one remote image record
- **AND** each site renders its own proxy URL

### Requirement: Media access control
The image endpoint SHALL apply the current site's private instance policy. A site MUST NOT enumerate another site's media references.

#### Scenario: Private site
- **WHEN** an anonymous client requests an image from a private site
- **THEN** the system returns 401
