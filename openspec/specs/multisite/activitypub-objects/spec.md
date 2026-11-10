# Activitypub Objects Specification

## Purpose

Store each ActivityPub object once per installation while keeping per-site visibility, moderation, and derived data separate.

## Requirements

### Requirement: Canonical object uniqueness
Person, Community, Post, Comment, and Site SHALL have one canonical row per `ap_id`. The row SHALL record `origin_local_site_id` when the object belongs to a site in this installation, and NULL when it belongs to an external instance.

#### Scenario: Shared remote post
- **WHEN** site A and site B fetch the same remote Post
- **THEN** the database contains one Post row for that `ap_id`
- **AND** each site has its own site state row

#### Scenario: Same name on two sites
- **WHEN** site A and site B each create a community named `general`
- **THEN** the two Community rows have different `ap_id` values
- **AND** each row has its own `origin_local_site_id`

### Requirement: Site presence controls visibility
A site SHALL see an object only when the site has a presence row for that object. Creating a local object SHALL create the origin site's presence row. Receiving an activity SHALL create the destination site's presence row. The existence of a canonical row alone MUST NOT make an object visible to a site.

#### Scenario: Activity not delivered
- **WHEN** site A creates a local post
- **AND** site B has not received the post through federation
- **THEN** site B does not list or read the post

#### Scenario: Activity delivered
- **WHEN** site B receives the post
- **THEN** site B creates its presence row
- **AND** the post becomes visible to site B under its federation policy

### Requirement: Origin moderation and local moderation are separate
The canonical object SHALL store moderation and deletion state that the origin instance published. Site state rows SHALL store moderation that the local site applied. A local removal on site A MUST NOT change the canonical object or site B's view.

#### Scenario: Local removal of a remote post
- **WHEN** site A's administrator removes a remote post
- **THEN** site A's state row records the removal
- **AND** site B's view and the canonical post remain unchanged

#### Scenario: Origin removal
- **WHEN** the origin community removes a post
- **THEN** the canonical post records the origin removal
- **AND** every site that has the post sees it as removed

### Requirement: Shared rows contain source data
Canonical rows SHALL store source URLs and media identifiers. They MUST NOT store proxy URLs or thumbnail URLs that contain a site hostname. Views SHALL build per-site proxy and thumbnail URLs from the tenant context.

#### Scenario: Remote image post
- **WHEN** site A fetches a remote image post
- **THEN** the canonical row keeps the source URL
- **AND** site A and site B each render their own proxy URL

### Requirement: Aggregates follow content scope
Aggregates for content that is globally shared in the Fediverse MAY be stored at installation level. Aggregates or state that depend on a site's local users or local moderation SHALL be stored in the site state rows.

#### Scenario: Shared remote post score
- **WHEN** users on site A and site B vote on the same remote post
- **THEN** the installation may keep a combined score for the canonical post
- **AND** site-local moderation and presence remain separate

### Requirement: Object upsert is idempotent
Receiving the same object or activity more than once SHALL update the canonical row in place and MUST NOT create a duplicate. Activity delivery records SHALL be scoped per site.

#### Scenario: Duplicate activity
- **WHEN** site A receives the same activity twice
- **THEN** the second delivery is ignored for site A
- **AND** a later delivery to site B is processed for site B

### Requirement: Private messages stay private
PrivateMessage rows SHALL be visible only to the sender's site and the recipient's site. Shared object rules MUST NOT expose a private message to an unrelated site.

#### Scenario: Private message on another site
- **WHEN** site A's user sends a private message to a user on site B
- **THEN** site C cannot read or list that message
