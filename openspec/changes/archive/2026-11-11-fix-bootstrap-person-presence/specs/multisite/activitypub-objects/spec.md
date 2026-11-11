## MODIFIED Requirements

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

#### Scenario: Bootstrap actors
- **WHEN** first-site setup or site provisioning creates the administrator or system account
- **THEN** the owning site has a `person_site` row for that Person

#### Scenario: Existing local person without presence
- **WHEN** the installation has a local Person row with no site presence
- **THEN** the owning site gains a presence row for that Person
