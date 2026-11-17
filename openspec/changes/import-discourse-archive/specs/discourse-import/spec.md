# Spec Delta

## Purpose

Provide operators with a safe, repeatable way to move a Discourse archive into one LemmyMultisite site while preserving account identity, discussion relationships, and legacy links where possible.

## ADDED Requirements

### Requirement: Offline archive inspection
The importer SHALL accept supported Discourse `.tar.gz` backups and `.sql.gz` database backups, inspect their metadata and contents before target writes, and produce a preview with source counts, detected version, warnings, and required operator mappings.

#### Scenario: Preview a full archive
- **WHEN** an operator previews a valid `.tar.gz` backup
- **THEN** the importer reports database and upload contents without modifying the target site

#### Scenario: Reject an unsupported or malformed archive
- **WHEN** an archive lacks a recognized Discourse database dump or contains invalid required tables
- **THEN** the importer reports the problem and performs no target writes

### Requirement: Site-scoped resumable import
An import SHALL target exactly one selected LocalSite, keep federation disabled, emit no outbound federation activities, and resume without duplicating records after interruption.

#### Scenario: Resume an interrupted import
- **WHEN** the operator reruns an import with the same source identity and target site
- **THEN** completed source records are reused through stable mappings and remaining records continue

#### Scenario: Target site has federation enabled
- **WHEN** an operator starts an import for a site whose federation setting is enabled
- **THEN** the importer refuses to write until federation is disabled

### Requirement: Account migration and credential compatibility
The importer SHALL migrate every source user to a site-owned local account, preserve a compatible Discourse password verifier when present, and retain a clear account mapping when a source user cannot authenticate with a password.

#### Scenario: Imported user signs in with legacy password
- **WHEN** a migrated user supplies the password matching the imported Discourse PBKDF2 verifier
- **THEN** authentication succeeds on the target site and the verifier is upgraded to the current password hash format

#### Scenario: Source account has no usable password
- **WHEN** a source account has no supported password verifier
- **THEN** the account is still migrated and the report marks it for password reset or administrator recovery

#### Scenario: Username or email conflict
- **WHEN** a source username or email conflicts with a target-site account
- **THEN** preview requires a deterministic resolution and import does not silently merge identities

#### Scenario: Source account has restrictions or a privileged role
- **WHEN** a source account is inactive, unapproved, suspended, silenced, staged, or has a Discourse admin/moderator role
- **THEN** the account report identifies the source user for operator review, and Discourse roles do not grant Lemmy privileges automatically

#### Scenario: Preserve local account preferences
- **WHEN** a source user has `allow_private_messages` or `external_links_in_new_tab` preferences
- **THEN** the importer maps them to Lemmy's corresponding private-message and open-links preferences

#### Scenario: Preserve profile fields without target equivalents
- **WHEN** a source user has a location or website
- **THEN** the importer retains those values in the imported biography with clear labels

### Requirement: Category hierarchy mapping
The importer SHALL map each leaf Discourse category to one Lemmy community and each parent category to one Lemmy multi-community containing the communities mapped from its descendant leaf categories.

#### Scenario: Import nested categories
- **WHEN** a Discourse category tree contains leaf categories under parent categories
- **THEN** each leaf has one community and each parent has a multi-community with its descendant communities

#### Scenario: Parent category exceeds the target membership limit

- **WHEN** a source parent category has 50 or more imported descendant communities
- **THEN** the importer SHALL report the category as unresolved, SHALL NOT claim the multi-community structure is complete, and SHALL block final validation

#### Scenario: Category access restrictions are unsupported
- **WHEN** a source category has restricted visibility or posting permissions
- **THEN** preview and final report identify the restriction as not enforced by this import

### Requirement: Discussion and interaction migration
The importer SHALL migrate topics, posts, reply relationships, private messages, votes, bookmarks, and uploads when a safe Lemmy representation exists, preserving source timestamps and source-to-target identifiers where the target model permits.

#### Scenario: Import topic replies
- **WHEN** a topic contains posts with reply references
- **THEN** the topic starter becomes a Lemmy post and replies become comments with the corresponding parent relationships

#### Scenario: Import a private message
- **WHEN** a supported source private message has migrated sender and recipient accounts
- **THEN** it is imported as a site-owned Lemmy private message with the same participants and chronology

#### Scenario: Import poll or unsupported interaction data
- **WHEN** source poll structure or another interaction has no equivalent target model
- **THEN** the importer preserves recoverable poll prompt, options, and aggregate results in an explicitly labeled representation or export report and records any omitted semantics

### Requirement: Migration report and validation
The importer SHALL produce a durable report of imported, skipped, transformed, and failed source records and SHALL validate key counts and relationships before marking an import complete.

#### Scenario: Validation finds missing references
- **WHEN** a source reply, message, vote, or attachment refers to an unmapped source record
- **THEN** the importer records the unresolved reference and does not mark the import complete

#### Scenario: Import completes
- **WHEN** all configured import phases finish and required integrity checks pass
- **THEN** the report records completion and source-to-target mappings for audit and reruns

### Requirement: Configurable legacy URL mapping
The migration SHALL provide configurable source URL patterns and target URL mappings that can be used by a reverse proxy to redirect old Discourse topic and post URLs to LemmyMultisite frontend routes without embedding a Discourse hostname in frontend assets.

#### Scenario: Redirect a legacy topic URL
- **WHEN** a request matches a configured legacy topic or post URL
- **THEN** the deployment mapping redirects it to the corresponding imported Lemmy post route

#### Scenario: Change the source hostname
- **WHEN** an operator changes the configured legacy source hostname
- **THEN** the mapping can be regenerated or updated without rebuilding the frontend
