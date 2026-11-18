# Proposal

## Why

The Docker deployment currently serves the upstream Lemmy UI, while this project provides its own Elm frontend for the multisite API. Using the project frontend in the standard deployment makes the shipped interface match the backend capabilities and keeps browser API traffic on the same origin.

## What Changes

- Build the Lemmy backend and `lemmyms-frontend` images in GitHub Actions and publish them to GHCR with branch and commit SHA tags.
- Configure Docker Compose to pull the published GHCR images, with a selectable image tag and no local source checkout requirement.
- Replace the upstream `lemmy-ui` service and update Nginx routing so frontend pages, API requests, ActivityPub traffic, and media use the same public origin and correct backend routes.
- Keep federation behavior isolated per tenant so a federation-disabled site neither sends nor receives federation traffic while enabled tenants continue operating.
- Document image selection, publication, and deployment without prescribing host paths.

## Capabilities

### New Capabilities

- `frontend-deployment`: Serving the project frontend and routing its same-origin backend traffic in the container deployment.

### Modified Capabilities

None.

## Impact

Impacts the container build workflow, GHCR packages, service orchestration, reverse proxy, and deployment guidance. Tenant federation settings remain independent; disabling federation for one tenant must not disable it for other tenants in the same server process. Backend APIs and data storage are unchanged.
