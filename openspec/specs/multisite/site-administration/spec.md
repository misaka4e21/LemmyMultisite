# Site Administration Specification

## Purpose

Let an installation operator create and manage independent Fediverse sites without giving the operator a Fediverse Person identity.

## Requirements

### Requirement: Installation administrator is separate
The system SHALL provide an installation administrator identity that is not a Person, LocalUser, or ActivityPub actor. Installation administrator credentials MUST NOT authenticate against a site API.

#### Scenario: Installation admin login
- **WHEN** the installation administrator signs in
- **THEN** the session can list and manage sites
- **AND** the session cannot act as a site user

### Requirement: Site provisioning
Creating a site SHALL create an Instance row for the hostname, a Site actor with its keypair and AP identifiers, a system account, a LocalSite row, a LocalSiteRateLimit row, and the initial site administrator in one transaction.

#### Scenario: Create site
- **WHEN** the installation administrator creates `b.example` with an initial administrator
- **THEN** the new site can serve `/api/v4/site` on `b.example`
- **AND** its AP actor identifiers use `https://b.example`

#### Scenario: Hostname conflict
- **WHEN** the installation administrator creates a site with an existing hostname
- **THEN** the transaction fails and no partial site rows remain

### Requirement: Site removal
Removing a site SHALL first disable it, then remove or archive its local accounts, local actors, local content, site-scoped policy, queues, and media references. Shared remote objects and media objects with other references MUST remain.

#### Scenario: Remove one site
- **WHEN** the installation administrator removes site A
- **THEN** site A stops serving requests
- **AND** remote objects and media still used by site B remain available

#### Scenario: AP identifier reuse
- **WHEN** a site is removed
- **THEN** its hostname and AP identifiers are not reassigned to a new site

### Requirement: Administrator boundaries
A site administrator SHALL manage only the current site. The installation administrator SHALL manage site lifecycle and installation settings and MUST NOT gain site-level moderation powers through that role.

#### Scenario: Cross-site admin call
- **WHEN** site A's administrator calls an admin endpoint on site B
- **THEN** the system returns 403

### Requirement: Configuration boundary
Installation configuration SHALL contain database, pict-rs, SMTP transport, bind address, worker counts, logging, and object storage. Site configuration SHALL contain hostname, site name, SMTP sender, registration policy, federation policy, image policy, languages, and site administrators.

#### Scenario: New site without restart
- **WHEN** the installation administrator creates a site
- **THEN** the site starts serving without editing the installation config file or restarting the process

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

### Requirement: Site-owned administrative configuration
OAuth providers, custom emoji and taglines SHALL belong to exactly one site. Create, list, site response, update and delete operations SHALL use the current site; modifying one site's configuration SHALL NOT change another site's configuration or cached responses.

#### Scenario: Cross-site configuration ID
- **WHEN** site A's administrator updates or deletes site B's OAuth provider, custom emoji or tagline by ID
- **THEN** the request is rejected and site B's configuration remains unchanged

#### Scenario: Site configuration listing
- **WHEN** a client reads site A's configuration or lists its emoji or taglines
- **THEN** it receives only site A's corresponding records

### Requirement: Administrative targets and reports are scoped
Administrator assignment, removal, hierarchy checks, site bans, registration actions, reports, report counts, bulk operations and purge SHALL be scoped to the current site and the action's target. Handling a report SHALL require authority over that report; a report's global ID alone MUST NOT grant access to its content.

#### Scenario: Assign another site's user
- **WHEN** site A's administrator attempts to grant or remove administrator status for a site B account
- **THEN** the system rejects the request and leaves site B's account unchanged

#### Scenario: Another site's private message report
- **WHEN** site A's administrator attempts to resolve or read a private message report outside site A's authority
- **THEN** the system rejects the request without returning the report or private message

#### Scenario: Report count
- **WHEN** an administrator reads the current site's report count
- **THEN** the count excludes reports outside that site's authority

#### Scenario: Federated report received by multiple sites
- **WHEN** a federated report is received by site A and site B
- **THEN** each receiving site has its own report association and resolution state
- **AND** resolving the report on site A does not resolve or expose site B's state

### Requirement: Existing configuration migration preserves ownership
Migration SHALL assign existing global configuration to one site per record. A single-site installation SHALL assign records to its only site. A multi-site installation SHALL require an explicit complete ownership mapping and SHALL reject migration when ownership or OAuth account associations are ambiguous or inconsistent; failed migration MUST leave existing data unchanged.

#### Scenario: Incomplete multi-site mapping
- **WHEN** a multi-site installation migrates with a missing or invalid configuration mapping
- **THEN** migration fails without assigning or copying the affected records

#### Scenario: Existing OAuth association
- **WHEN** an OAuth account belongs to a different site from its mapped provider
- **THEN** migration fails and reports the conflict for resolution
