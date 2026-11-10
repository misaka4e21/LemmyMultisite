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
