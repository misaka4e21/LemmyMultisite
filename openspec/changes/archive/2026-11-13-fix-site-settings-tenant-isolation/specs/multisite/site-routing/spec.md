## MODIFIED Requirements

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
