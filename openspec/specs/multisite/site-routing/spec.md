# Site Routing Specification

## Purpose

Resolve every inbound request to exactly one LocalSite and make that site the scope for configuration, caches, rate limits, rendering, and background work.

## Requirements

### Requirement: Host selects the tenant
The system SHALL resolve the active LocalSite from the request Host header. Body fields, query parameters, path segments, and JWT claims MUST NOT select a different tenant. Host comparison SHALL use lower-case hostnames with the port removed and a trailing dot removed.

#### Scenario: Two sites resolve separately
- **WHEN** a request arrives with Host `a.example`
- **THEN** the request runs as site A
- **AND** a request with Host `b.example` runs as site B

#### Scenario: Unknown host
- **WHEN** Host does not match an enabled LocalSite
- **THEN** the system returns 404

#### Scenario: Client tries to override tenant
- **WHEN** a request to site A includes a body field or JWT claim for site B
- **THEN** the system returns 403 and does not execute the action

### Requirement: Tenant context is explicit
Every request handler that reads or writes site-scoped data SHALL receive a tenant context containing the LocalSite row, Site row, canonical hostname, and scheme. The system MUST NOT use a process-wide mutable current-site value.

#### Scenario: Concurrent requests
- **WHEN** site A and site B requests run at the same time
- **THEN** each handler sees only its own tenant context

### Requirement: Site-scoped caches
Caches for SiteView, GetSiteResponse, LocalSiteData, slur filters, URL blocklists, rate limit configuration, and latest activity IDs SHALL be keyed by `local_site_id`. A site setting change SHALL invalidate the affected site's entries.

#### Scenario: Site edit
- **WHEN** an administrator changes site A's name
- **THEN** site A's cached site response is invalidated
- **AND** site B's cached response remains unchanged

### Requirement: Site-scoped rate limits
The rate limiter SHALL apply the active site's LocalSiteRateLimit values and SHALL include `local_site_id` in its bucket key. Site setup and editing SHALL update only the active site's rate limit configuration. A failed rate limit update SHALL return an error instead of reporting success.

#### Scenario: Same client on two sites
- **WHEN** one client sends post requests to site A and site B
- **THEN** each site counts the requests against its own limit

#### Scenario: Site setup changes rate limits
- **WHEN** an administrator initializes site A with rate limit values
- **THEN** site A stores and applies those values
- **AND** site B's stored and runtime rate limits remain unchanged

#### Scenario: Site editing changes rate limits
- **WHEN** an administrator edits site A's rate limits
- **THEN** site A stores and applies the changed values
- **AND** site B's stored and runtime rate limits remain unchanged

#### Scenario: Rate limit update fails
- **WHEN** site setup or editing cannot persist the requested rate limit update
- **THEN** the request returns an error

### Requirement: Site-scoped routes
WebFinger, nodeinfo, feeds, sitemap, ActivityPub actor and inbox routes, and `/api/v4/image/{alias}` SHALL resolve the tenant from Host and render URLs with that site's hostname.

#### Scenario: WebFinger on two sites
- **WHEN** a client requests `acct:alice@a.example` from site A
- **THEN** the response points to `https://a.example/u/alice`
- **AND** the same name on site B points to `https://b.example/u/alice`

### Requirement: Hostname changes
A LocalSite hostname SHALL be immutable after the site has local actors or federated content. The system MAY allow a hostname change for a newly created site with no content, and MUST invalidate the hostname cache after the change.

#### Scenario: Change blocked
- **WHEN** an administrator changes the hostname of a site that has local users
- **THEN** the system rejects the change and keeps the existing AP identifiers valid
