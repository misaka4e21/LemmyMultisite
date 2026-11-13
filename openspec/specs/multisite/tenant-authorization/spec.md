# Tenant Authorization Specification

## Purpose

Enforce authorization for every protected action using the request site, the acting identity, and the target resource, including indirect and asynchronous operations.

## Requirements

### Requirement: Authorization binds actor, action, target, and site
The system SHALL authorize every protected read and write against the authenticated identity, requested action, target resource, and current site. A site administrator role SHALL NOT grant authority over another site's resources. A community moderator SHALL retain explicitly granted authority within that community, subject to the action's rules. Installation administrator credentials SHALL NOT confer site-user authority.

#### Scenario: Cross-site target ID
- **WHEN** an administrator on site A submits a protected action using a target ID owned by site B
- **THEN** the system rejects the action without exposing protected target data
- **AND** no database, cache, media, notification, federation, or queued-task side effect occurs

#### Scenario: Same-site authorized action
- **WHEN** a site A administrator performs a permitted action on an eligible site A resource
- **THEN** the system permits the action subject to existing role and hierarchy rules

#### Scenario: Community moderator
- **WHEN** a user with an explicit moderator role performs an allowed action in that community
- **THEN** the system authorizes the action under the community's applicable rules

### Requirement: Complete authorization audit
The implementation SHALL inventory and audit registered v4 and v3 routes, authentication paths, media, private messages, federation handlers, scheduled jobs, queues and background tasks for actor, target, action, tenant and side effects. It SHALL fix confirmed authorization gaps discovered by this audit and retain a reviewable record of coverage and outcomes.

#### Scenario: An additional unscoped write is found
- **WHEN** the audit confirms that a handler can change another site's protected data
- **THEN** the handler and its indirect effects are fixed and covered by a regression test before the audit task is complete

### Requirement: Authorization is preserved across indirect work
The system SHALL preserve and enforce the authorized site's scope in asynchronous work, batch operations, notifications, federation and cache invalidation. Rejected requests MUST NOT enqueue work or partially mutate protected resources.

#### Scenario: Rejected operation with background work
- **WHEN** a cross-site action would normally schedule a background deletion
- **THEN** the request is rejected before scheduling and the target remains unchanged
