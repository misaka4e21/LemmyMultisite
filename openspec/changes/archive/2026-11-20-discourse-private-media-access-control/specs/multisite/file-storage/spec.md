# Spec Delta

## ADDED Requirements

### Requirement: Request-time authorization for protected documents
Document references associated with a private message or restricted post SHALL require authorization against that target resource on every download request. Protected documents MUST NOT be readable through a public URL by users who cannot currently access the associated resource.

#### Scenario: Private-message document requested by a current participant
- **WHEN** a currently authenticated participant requests a document attached to a private message they can currently read
- **THEN** the server streams the document

#### Scenario: Private-message document requested by a nonparticipant
- **WHEN** a requester who is not currently allowed to read the associated private message requests its document
- **THEN** the server denies access without returning document bytes

#### Scenario: Restricted-post document authorization changes
- **WHEN** a user's permission to view the associated post changes after import
- **AND** that user requests the attached document
- **THEN** the server evaluates current post visibility and allows or denies the download accordingly

#### Scenario: Unauthenticated request for protected document
- **WHEN** an unauthenticated requester requests a document attached to a private message or restricted post
- **THEN** the server denies access without returning document bytes

#### Scenario: Existing public document
- **WHEN** a requester accesses an unprotected document reference on a public site
- **THEN** existing site-level download behavior remains unchanged
