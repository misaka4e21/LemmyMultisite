# Spec Delta

## ADDED Requirements

### Requirement: Request-time authorization for protected images
Image references associated with a private message or restricted post SHALL require authorization against that target resource on every image request. Protected images MUST NOT be retrievable by users who cannot currently access the associated resource.

#### Scenario: Private-message image requested by a current participant
- **WHEN** a currently authenticated participant requests an image attached to a private message they can currently read
- **THEN** the image endpoint serves the image

#### Scenario: Private-message image requested by a nonparticipant
- **WHEN** a requester who is not currently allowed to read the associated private message requests its image
- **THEN** the image endpoint denies access without returning image bytes

#### Scenario: Restricted-post image authorization changes
- **WHEN** a user's permission to view the associated post changes after import
- **AND** that user requests an attached image
- **THEN** the image endpoint evaluates current post visibility and allows or denies the request accordingly

#### Scenario: Unauthenticated request for protected image
- **WHEN** an unauthenticated requester requests an image attached to a private message or restricted post
- **THEN** the image endpoint denies access without returning image bytes

#### Scenario: Existing public image
- **WHEN** a requester accesses an unprotected image on a public site
- **THEN** existing site-level image access behavior remains unchanged
