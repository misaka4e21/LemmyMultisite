# Design

## Context

The current Compose stack runs `dessalines/lemmy-ui:nightly` on port 1234. The local Nginx configuration sends most browser requests to that service and sends only a subset of backend paths directly to Lemmy. The replacement application is an Elm/Vite static site that builds with pnpm from a lockfile. It expects same-origin `/api/` requests, and its deployment sample also routes federation and media resources to the backend.

## Goals / Non-Goals

**Goals:**

- Fetch the frontend from its GitHub repository as a Docker build context and serve its static output without relying on a host checkout.
- Keep a single public origin for frontend assets, authentication cookies, API calls, ActivityPub resources, and media.
- Preserve the existing backend and storage services.

**Non-Goals:**

- Change backend API behavior, database schema, or storage configuration.
- Publish or configure a separately hosted frontend.
- Replace the project's existing Docker development and federation test stacks unless required by shared configuration.

## Decisions

- **Use a dedicated frontend image build stage/service with a remote Git build context.** Fetch `misaka4e21/lemmyms-frontend` directly from GitHub through BuildKit, default to the `lemmy-ms` branch, and allow a configurable Git revision. Use Node.js 22 with Corepack and pnpm 10.26.0 to read the checked-in package lock and honor the repository's explicit dependency build approvals. Set a writable `ELM_HOME`, run the production build, and expose the generated static assets through a small web server. An optional `NPM_PROXY` Compose build setting maps to the BuildKit HTTP(S) proxy variables; when empty, all proxy variables are cleared so the build connects directly. This avoids requiring or documenting a host checkout and keeps the frontend toolchain separate from the Rust backend image.
- **Let the public Nginx proxy own routing.** The frontend static server serves assets and falls back to `index.html` for client routes; the existing public Nginx remains the one exposed entry point and routes `/api/` plus backend protocol/media paths to Lemmy. Preserve ActivityPub content negotiation for paths that overlap with frontend routes, notably `/communities/`.
- **Keep the browser API URL relative.** The frontend production app uses same-origin API paths; no build-time backend hostname or browser CORS configuration is needed. Internal upstream hostnames remain Docker service names.
- **Retain the stable proxy upstream role.** The Compose service and Nginx upstream can use a neutral frontend name so replacing the implementation does not require frontend-specific behavior in routing rules.

Alternatives considered: requiring a local checkout makes deployment depend on a machine-specific source directory; cloning inside the Dockerfile duplicates Git-fetch logic. A named Git build context keeps source acquisition in BuildKit while retaining a dedicated frontend image. Building frontend assets into the Rust image would increase coupling and rebuild cost; serving assets directly from the public proxy container would complicate image ownership and static-site fallback configuration.

## Risks / Trade-offs

- [GitHub or the package registry is unavailable during a clean image build] → Make the frontend revision configurable and allow an optional build proxy, disabled by default.
- [A moving frontend revision produces different builds over time] → Allow deployment operators to select an immutable commit revision.
- [Elm compilation needs a writable cache and can lengthen clean builds] → Set `ELM_HOME` within the build stage and use normal Docker layer caching for dependency installation.
- [An incomplete backend route list can return SPA HTML for protocol requests] → Compare Nginx routes with the frontend deployment sample and preserve backend paths and content-negotiation routing.
- [Missing translation catalog requests can be masked by SPA fallback] → Ensure static asset paths return 404 when files are absent rather than returning `index.html`.

## Migration Plan

1. Build the frontend image and start it with the existing backend services.
2. Update Nginx routing and verify root, deep frontend routes, API, media, and ActivityPub requests through the public ports.
3. Recreate the proxy and frontend services to switch traffic.
4. Roll back by restoring the prior Compose and Nginx definitions and restarting the prior `lemmy-ui` image service; no data migration is involved.

## Open Questions

None. The remote source and revision selection are sufficient for this deployment design.
