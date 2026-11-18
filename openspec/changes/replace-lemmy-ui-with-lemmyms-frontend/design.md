# Design

## Context

The current Compose stack runs `dessalines/lemmy-ui:nightly` on port 1234. The local Nginx configuration sends most browser requests to that service and sends only a subset of backend paths directly to Lemmy. The replacement application is an Elm/Vite static site that builds with pnpm from a lockfile. It expects same-origin `/api/` requests, and its deployment sample also routes federation and media resources to the backend. The server supports independent federation settings per tenant, while `LEMMY_DISABLE_ACTIVITY_SENDING` disables outgoing activity for the whole process and is intended for horizontally scaled setups.

## Goals / Non-Goals

**Goals:**

- Publish backend and frontend images from GitHub Actions and deploy those images through Compose without relying on a host checkout.
- Keep a single public origin for frontend assets, authentication cookies, API calls, ActivityPub resources, and media.
- Preserve the existing backend and storage services.
- Ensure a tenant's federation setting does not change federation behavior for other tenants in the same process.

**Non-Goals:**

- Change backend API behavior, database schema, or storage configuration.
- Publish or configure a separately hosted frontend.
- Replace the project's existing Docker development and federation test stacks unless required by shared configuration.

## Decisions

- **Build and publish both images in GitHub Actions.** Check out the backend repository including submodules, build the backend image, and provide the frontend repository as a remote BuildKit context pinned to the `lemmy-ms` default branch. Publish separate backend and frontend GHCR packages with branch and commit SHA tags. An optional `NPM_PROXY` Actions secret supplies frontend package registry proxy build arguments; when absent, the build uses no proxy.
- **Pin Compose to a shared image tag variable.** Compose pulls the two GHCR packages using `LEMMY_IMAGE_TAG`, defaulting to the current `multisite` branch tag. Operators can select a branch tag for ongoing updates or a commit SHA tag for a reproducible deployment. `pull_policy: always` ensures Compose checks the registry for the selected tag.
- **Let the public Nginx proxy own routing.** The frontend static server serves assets and falls back to `index.html` for client routes; the existing public Nginx remains the one exposed entry point and routes `/api/` plus backend protocol/media paths to Lemmy. Preserve ActivityPub content negotiation for paths that overlap with frontend routes, notably `/communities/`.
- **Keep the browser API URL relative.** The frontend production app uses same-origin API paths; no build-time backend hostname or browser CORS configuration is needed. Internal upstream hostnames remain Docker service names.
- **Retain the stable proxy upstream role.** The Compose service and Nginx upstream can use a neutral frontend name so replacing the implementation does not require frontend-specific behavior in routing rules.
- **Keep federation switches tenant-scoped.** Rely on each tenant's `federation_enabled` setting and the existing per-tenant federation configuration. Do not set `LEMMY_DISABLE_ACTIVITY_SENDING` in the deployment because it suppresses outbound activity for every tenant in that process.

Alternatives considered: building directly on each deployment host makes deployments depend on host toolchains and source availability; building frontend assets into the Rust image would increase coupling and rebuild cost. A global activity-sending disable flag is unsuitable because it prevents enabled tenants from federating. The CI workflow centralizes image creation while Compose remains a lightweight image-based deployment.

## Risks / Trade-offs

- [GHCR packages are private or unavailable to the deployment host] → Document package visibility and authenticate the host before pulling private packages.
- [A moving branch tag changes over time] → Publish SHA tags and allow deployment operators to select one for reproducible rollback.
- [The frontend package registry is inaccessible from Actions] → Support an optional repository Actions secret for an npm proxy, disabled by default.
- [Elm compilation needs a writable cache and can lengthen clean builds] → Set `ELM_HOME` within the build stage and use normal Docker layer caching for dependency installation.
- [An incomplete backend route list can return SPA HTML for protocol requests] → Compare Nginx routes with the frontend deployment sample and preserve backend paths and content-negotiation routing.
- [Missing translation catalog requests can be masked by SPA fallback] → Ensure static asset paths return 404 when files are absent rather than returning `index.html`.

## Migration Plan

1. Merge or push the workflow source so GitHub Actions publishes backend and frontend images with branch and SHA tags.
2. Select a published image tag and start the Compose deployment; authenticate to GHCR first if packages are private.
3. Verify root, deep frontend routes, API, media, and ActivityPub requests through the public proxy, including tenants with federation independently enabled and disabled.
4. Roll back by selecting the prior image tag and restoring the prior Compose and Nginx definitions; no data migration is involved.

## Open Questions

None. The remote source and revision selection are sufficient for this deployment design.
