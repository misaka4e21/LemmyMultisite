## Purpose

Keep accounts, local actors, and locally created content inside the site that owns them, so two sites can use the same names without sharing identity or permissions.

## ADDED Requirements

### Requirement: Site-scoped accounts
LocalUser SHALL have a non-null `local_site_id`. Username and email uniqueness SHALL be enforced per site. A Person row for a local account SHALL have an `origin_local_site_id` equal to that site.

#### Scenario: Same username on two sites
- **WHEN** site A registers `alice` and site B registers `alice`
- **THEN** the database contains two Person rows and two LocalUser rows
- **AND** each account authenticates only on its own site

#### Scenario: Same email on two sites
- **WHEN** the same email registers on site A and site B
- **THEN** both registrations succeed and produce separate accounts

### Requirement: Tenant-bound authentication
Login, registration, password reset, email verification, OAuth callback, and session validation SHALL use the request tenant. A JWT SHALL include `local_site_id`, and the system SHALL reject a token whose site does not match Host.

#### Scenario: Token used on another site
- **WHEN** a site A token is sent to site B
- **THEN** the system returns 403

#### Scenario: Password reset
- **WHEN** a user requests a password reset on site A
- **THEN** the email link and token work only on site A

### Requirement: Browser JWT cookie
Browser login SHALL set the site JWT in an HttpOnly cookie scoped to the active site host. The cookie SHALL use Path=/, SameSite=Lax, and Secure when the site scheme is HTTPS. API authentication SHALL accept the JWT from this cookie, and logout SHALL clear it.

#### Scenario: Browser login
- **WHEN** a user logs in on site A from a browser
- **THEN** the response sets the site JWT cookie
- **AND** subsequent same-host API requests authenticate without JavaScript reading the token

#### Scenario: Cross-site cookie
- **WHEN** a cookie issued for site A is sent to site B
- **THEN** site B rejects it because the JWT local_site_id does not match Host

#### Scenario: Logout
- **WHEN** the user logs out
- **THEN** the response clears the JWT cookie for that host

### Requirement: Local actor ownership
Person, Community, Post, Comment, PrivateMessage, and MultiCommunity rows SHALL record an `origin_local_site_id` for objects created by a site and NULL for objects created by an external instance. The system SHALL expose separate checks for "belongs to this installation" and "belongs to this site".

#### Scenario: Local actor lookup
- **WHEN** site A requests `/u/alice` without a domain
- **THEN** the lookup is limited to site A's local Person rows

#### Scenario: External actor
- **WHEN** site A reads a Person from `lemmy.world`
- **THEN** that Person has a NULL `origin_local_site_id`
- **AND** site B can use the same Person row

### Requirement: Local reads are site-scoped
Listings and reads for local communities, posts, comments, people, reports, modlog, invites, media, and administrator views SHALL apply the current site scope. A site MUST NOT return another site's local content unless a per-site presence or federation rule allows it.

#### Scenario: Community listing
- **WHEN** a user lists local communities on site A
- **THEN** the response contains site A's local communities
- **AND** it does not contain site B's local-only communities

### Requirement: Site administrator scope
A site administrator SHALL moderate only the current site and its local communities. Site bans, registration decisions, reports, and modlog entries SHALL remain within that site.

#### Scenario: Site ban
- **WHEN** site A's administrator bans a Person
- **THEN** the ban applies to site A
- **AND** site B's access for that Person remains unchanged

### Requirement: Name resolution
WebFinger and actor lookup SHALL resolve the domain to a LocalSite when the domain belongs to this installation, and SHALL verify that the request tenant matches the resource domain.

#### Scenario: Cross-host WebFinger
- **WHEN** site A receives a WebFinger request for `acct:alice@b.example`
- **THEN** the system rejects the cross-host request
