# Proposal

## Why

The Docker deployment currently serves the upstream Lemmy UI, while this project provides its own Elm frontend for the multisite API. Using the project frontend in the standard deployment makes the shipped interface match the backend capabilities and keeps browser API traffic on the same origin.

## What Changes

- Build the `lemmyms-frontend` static application as part of the Docker deployment and serve its generated assets.
- Replace the upstream `lemmy-ui` service and update Nginx routing so frontend pages, API requests, ActivityPub traffic, and media use the same public origin and correct backend routes.
- Document the frontend build input and deployment configuration needed by Compose.

## Capabilities

### New Capabilities

- `frontend-deployment`: Serving the project frontend and routing its same-origin backend traffic in the container deployment.

### Modified Capabilities

None.

## Impact

Impacts `docker/Dockerfile`, `docker/docker-compose.yml`, `docker/nginx.conf`, and deployment documentation. The frontend source is maintained in the adjacent `lemmyms-frontend` checkout and its production build requires Node.js 20 or newer plus an Elm cache. Backend APIs and data storage are unchanged.
