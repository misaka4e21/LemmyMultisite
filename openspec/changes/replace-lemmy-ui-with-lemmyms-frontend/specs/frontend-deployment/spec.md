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

### Requirement: Compose deploys published container images
The standard Docker Compose deployment SHALL pull the backend and frontend images from GHCR using a configurable tag, and the image publication workflow SHALL publish both branch and commit SHA tags.

#### Scenario: Deploy a selected image revision
- **WHEN** an operator starts the standard Compose deployment with an image tag selected
- **THEN** Compose pulls the matching backend and frontend images from GHCR

#### Scenario: Select a commit-pinned image
- **WHEN** an operator selects a commit SHA tag
- **THEN** both services run the image pair published by the workflow for that LemmyMultisite commit

### Requirement: Federation configuration is isolated per tenant
The deployment SHALL preserve tenant-specific federation settings and SHALL NOT disable federation process-wide when one tenant has federation disabled.

#### Scenario: A federation-disabled tenant shares a server with enabled tenants
- **WHEN** one tenant has federation disabled and another tenant has federation enabled
- **THEN** the disabled tenant neither sends nor receives federation traffic, while the enabled tenant continues to send and receive federation traffic
