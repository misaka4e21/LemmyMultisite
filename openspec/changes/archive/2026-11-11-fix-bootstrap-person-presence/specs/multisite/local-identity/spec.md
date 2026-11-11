## MODIFIED Requirements

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
