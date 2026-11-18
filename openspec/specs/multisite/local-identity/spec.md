# Local Identity Specification

## Purpose

Keep accounts, local actors, and locally created content inside the site that owns them, so two sites can use the same names without sharing identity or permissions.

## Requirements

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

#### Scenario: Bootstrap administrator
- **WHEN** first-site setup creates the initial administrator and system account
- **THEN** both Person rows have `origin_local_site_id` set to that site's LocalSite

#### Scenario: Provisioned site system account
- **WHEN** an installation administrator provisions a new site
- **THEN** the site's system account Person row has `origin_local_site_id` set to that site's LocalSite

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

### Requirement: OAuth providers belong to the login site
OAuth authorization, callbacks, account linking and login SHALL use only providers owned by the request site. A provider ID from another site MUST NOT authenticate, register or link an account on the current site.

#### Scenario: Cross-site OAuth provider
- **WHEN** an OAuth login on site B supplies a provider owned by site A
- **THEN** site B rejects the attempt before exchanging credentials or creating or linking an account

#### Scenario: Same-site OAuth provider
- **WHEN** a user completes a valid OAuth login through an enabled provider owned by the current site
- **THEN** the existing registration and account-linking policy applies

### Requirement: Non-federated local usernames
When federation is disabled for a site, the system SHALL allow a local account to register or change its username to a name containing Chinese characters, Japanese hiragana or katakana, or Korean Hangul, subject to the existing username length, uniqueness, and safety constraints. A user SHALL be able to change their own username, and a site administrator SHALL be able to change a local account's username. A user SHALL be able to log in on the owning site using a valid username in any of these scripts. When federation is enabled, registration and username changes SHALL continue to enforce the ActivityPub actor-name rules.

#### Scenario: Register with a Chinese username while federation is disabled
- **WHEN** a user registers on a site with federation disabled using a valid Chinese username
- **THEN** registration succeeds if the username is available and satisfies the site's other account requirements

#### Scenario: Register with a Japanese kana username while federation is disabled
- **WHEN** a user registers on a site with federation disabled using a valid username containing Japanese hiragana or katakana
- **THEN** registration succeeds if the username is available and satisfies the site's other account requirements

#### Scenario: Register with a Korean Hangul username while federation is disabled
- **WHEN** a user registers on a site with federation disabled using a valid username containing Korean Hangul
- **THEN** registration succeeds if the username is available and satisfies the site's other account requirements

#### Scenario: Change own username to kana or Hangul while federation is disabled
- **WHEN** a user changes their own username on a site with federation disabled to a valid name containing Japanese kana or Korean Hangul
- **THEN** the username is updated

#### Scenario: Change own username while federation is disabled
- **WHEN** a user changes their own username on a site with federation disabled to a valid Chinese username
- **THEN** the username is updated

#### Scenario: Administrator changes a username while federation is disabled
- **WHEN** a site administrator changes a local account's username to a valid Chinese username while federation is disabled
- **THEN** the username is updated

#### Scenario: Administrator changes a username to kana or Hangul while federation is disabled
- **WHEN** a site administrator changes a local account's username on a site with federation disabled to a valid name containing Japanese kana or Korean Hangul
- **THEN** the username is updated

#### Scenario: Log in with a kana or Hangul username
- **WHEN** a user submits the correct credentials on their owning non-federated site using their Japanese kana or Korean Hangul username
- **THEN** login succeeds and authenticates the account for that site

#### Scenario: Reject non-actor username on a federated site
- **WHEN** a user registers or changes a username on a site with federation enabled
- **AND** the username violates the ActivityPub actor-name rules
- **THEN** the operation is rejected with a validation error

### Requirement: Non-federated community names
When federation is disabled for a site, the system SHALL allow creation and renaming of a local community to a name containing Chinese characters, subject to existing community name length, uniqueness, and safety constraints. An authorized community moderator or site administrator SHALL be able to rename a local community. When federation is enabled, community creation and renaming SHALL continue to enforce the ActivityPub actor-name rules.

#### Scenario: Create a community with a Chinese name while federation is disabled
- **WHEN** an authorized user creates a local community on a site with federation disabled using a valid Chinese name
- **THEN** the community is created if the name is available and satisfies the site's other community requirements

#### Scenario: Rename a community while federation is disabled
- **WHEN** an authorized community moderator or site administrator renames a local community on a site with federation disabled to a valid Chinese name
- **THEN** the community name is updated

#### Scenario: Reject non-actor community name on a federated site
- **WHEN** a user creates or renames a community on a site with federation enabled
- **AND** the name violates the ActivityPub actor-name rules
- **THEN** the operation is rejected with a validation error

### Requirement: Multiple password verifier formats
Local account authentication SHALL verify supported legacy PBKDF2 and bcrypt password hash formats as well as Argon2id. New passwords SHALL use Argon2id, and a successful login using a legacy verifier SHALL replace it with an Argon2id verifier without changing the account identity or site scope.

#### Scenario: Authenticate with imported PBKDF2 verifier
- **WHEN** a site-scoped local user presents a password matching a supported Discourse PBKDF2 verifier
- **THEN** login succeeds and the stored verifier is upgraded to Argon2id

#### Scenario: Authenticate with Argon2id verifier
- **WHEN** a local user presents a password matching an Argon2id verifier
- **THEN** login succeeds without changing the verifier unless its parameters require rehashing

#### Scenario: Reject unknown verifier format
- **WHEN** the stored verifier uses an unsupported algorithm or malformed encoding
- **THEN** login fails safely without revealing verifier details or changing the stored value
