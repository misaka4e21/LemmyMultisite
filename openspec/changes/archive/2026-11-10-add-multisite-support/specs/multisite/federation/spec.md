## Purpose

Give each site an independent Fediverse identity, apply its own federation policy, and deliver activities between sites in the same installation through database records instead of HTTP.

## ADDED Requirements

### Requirement: Per-site ActivityPub identity
Each LocalSite SHALL have its own Site actor, keypair, WebFinger subject, actor URL, inbox URL, and outbox URL. Actor and activity identifiers SHALL use the site hostname.

#### Scenario: Actor URLs
- **WHEN** a client fetches `/u/alice` from site A and site B
- **THEN** the responses describe different Person rows
- **AND** their `ap_id` values use `a.example` and `b.example`

#### Scenario: Activity ID
- **WHEN** site B creates an activity for `alice@b.example`
- **THEN** the activity ID uses `https://b.example/activities/...`

### Requirement: Inbound tenant routing
An inbound ActivityPub request SHALL be processed for the LocalSite selected by Host. The activity MUST address at least one local actor of that site. An activity that addresses only another local site's actor SHALL be rejected or treated as an external object.

#### Scenario: Wrong local inbox
- **WHEN** an activity addressed to `https://b.example/c/general` arrives at site A's inbox
- **THEN** site A does not process it as a local delivery to site B

#### Scenario: External activity
- **WHEN** an external instance delivers an activity to site A
- **THEN** site A processes it under site A's policy

### Requirement: Per-site federation policy
Federation enabled state, allow lists, block lists, signed fetch, image proxy policy, and registration policy SHALL be site-scoped. A policy change on site A MUST NOT change site B.

#### Scenario: Block one instance
- **WHEN** site A blocks `evil.example`
- **THEN** site A rejects content from that instance
- **AND** site B's policy remains unchanged

### Requirement: Per-site outbound queue
Every outgoing activity SHALL record `local_site_id`. Queue state SHALL be keyed by `(local_site_id, instance_id)`. A worker SHALL send only activities for its site and SHALL sign them with that site's or actor's key.

#### Scenario: Administrator action on a remote object
- **WHEN** site A's administrator removes a remote post
- **THEN** the outgoing activity records site A as `local_site_id`
- **AND** the worker signs it with site A's actor key

#### Scenario: Two queues
- **WHEN** site A and site B both send to `lemmy.world`
- **THEN** each site has its own queue state and retry state
- **AND** a failure on site A does not block site B

#### Scenario: Activity rejected by remote instance
- **WHEN** site A's delivery fails
- **THEN** site A retries under its own policy
- **AND** site B's delivery state is unchanged

### Requirement: Internal delivery between installation sites
Activities between sites in the same installation SHALL be delivered through an internal database path. The internal path SHALL use the same activity handlers, federation policy checks, presence updates, and notification logic as external delivery. It SHALL NOT require an HTTP request or HTTP signature.

#### Scenario: A follows a community on B
- **WHEN** a user on site A follows a community on site B
- **THEN** site B processes the Follow through its activity handler
- **AND** site A receives the Accept through the internal path
- **AND** no HTTP request is made between the two sites

#### Scenario: Internal delivery is idempotent
- **WHEN** the internal path delivers the same activity twice
- **THEN** the destination site processes it once
- **AND** both sites keep consistent presence and queue state

### Requirement: External federation remains HTTP
Activities between an installation site and an external instance SHALL use standard HTTP ActivityPub with HTTP signatures.

#### Scenario: Site A to external instance
- **WHEN** site A sends an activity to `lemmy.world`
- **THEN** the request uses site A's actor key and HTTP signature

### Requirement: Internal delivery failure isolation
A failure in internal delivery for one site pair SHALL NOT block external delivery or another site pair. The system SHALL record retry state per destination site.

#### Scenario: Internal failure
- **WHEN** internal delivery from site A to site B fails
- **THEN** site A's external queue continues
- **AND** the A to B delivery retries independently
