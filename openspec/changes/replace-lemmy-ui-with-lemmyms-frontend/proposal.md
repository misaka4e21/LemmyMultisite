# Proposal

## Why

The Docker deployment currently serves the upstream Lemmy UI, while this project provides its own Elm frontend for the multisite API. Using the project frontend in the standard deployment makes the shipped interface match the backend capabilities and keeps browser API traffic on the same origin.

## What Changes

- Fetch the `lemmyms-frontend` source directly from GitHub during the Docker build, build the static application, and serve its generated assets without requiring a host checkout.
- Replace the upstream `lemmy-ui` service and update Nginx routing so frontend pages, API requests, ActivityPub traffic, and media use the same public origin and correct backend routes.
- Document the remote frontend source, revision selection, and build requirements without prescribing a host directory.

## Capabilities

### New Capabilities

- `frontend-deployment`: Serving the project frontend and routing its same-origin backend traffic in the container deployment.

### Modified Capabilities

None.

## Impact

Impacts the container build, service orchestration, reverse proxy, and deployment guidance. The frontend production build requires Node.js 20 or newer plus a writable Elm cache. Backend APIs and data storage are unchanged.
