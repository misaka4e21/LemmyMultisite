# Spec Delta

## Purpose

Defines how the self-hosted deployment serves the project frontend and exposes its backend APIs and federation resources under one public origin.

## ADDED Requirements

### Requirement: Deployment serves the project frontend
The standard container deployment SHALL serve the production assets built from the `lemmyms-frontend` application as the site's user interface.

#### Scenario: User opens the site
- **WHEN** a browser requests the site root or a frontend route
- **THEN** the deployment serves the project frontend application

### Requirement: Frontend and backend share a public origin
The deployment SHALL route frontend API requests and backend protocol resources to the Lemmy backend while keeping browser requests on the same public origin as the frontend.

#### Scenario: Browser calls the API
- **WHEN** the frontend requests an `/api/` resource
- **THEN** the deployment forwards the request to the Lemmy backend without changing the browser-visible origin

#### Scenario: Client requests federation resources
- **WHEN** a client requests a federation or media resource, including one selected through ActivityPub content negotiation
- **THEN** the deployment forwards that request to the Lemmy backend

### Requirement: Frontend deep links load the application
The deployment SHALL return the frontend application document for valid client-side routes while preserving direct access to backend protocol and API routes.

#### Scenario: Direct navigation to a frontend route
- **WHEN** a browser loads a frontend route directly
- **THEN** the deployment returns the frontend application document
