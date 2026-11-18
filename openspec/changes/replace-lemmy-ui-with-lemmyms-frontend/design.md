# Design

## Context

The current Compose stack runs `dessalines/lemmy-ui:nightly` on port 1234. The local Nginx configuration sends most browser requests to that service and sends only a subset of backend paths directly to Lemmy. The replacement application is an Elm/Vite static site: its documented production workflow is `npm ci`, `npm run build`, then serving the generated `dist/` directory. The frontend expects same-origin `/api/` requests and the deployment sample also routes federation and media resources to the backend.

## Goals / Non-Goals

**Goals:**

- Build the frontend from the adjacent checkout in a reproducible container build and serve its static output.
- Keep a single public origin for frontend assets, authentication cookies, API calls, ActivityPub resources, and media.
- Preserve the existing backend and storage services.

**Non-Goals:**

- Change backend API behavior, database schema, or storage configuration.
- Publish or configure a separately hosted frontend.
- Replace the project's existing Docker development and federation test stacks unless required by shared configuration.

## Decisions

- **Use a dedicated frontend image build stage/service.** Use Node.js 20 or newer with Corepack and the checked-in `pnpm-lock.yaml` to install frozen dependencies, set a writable `ELM_HOME` in the build container, run the production build, and expose the resulting static assets through a small web server. Keeping this separate from the Rust backend image avoids coupling the frontend toolchain to the server runtime. Compose should build from the locally cloned frontend checkout, with a configurable build context if the adjacent directory may differ between installations.
- **Let the public Nginx proxy own routing.** The frontend static server serves assets and falls back to `index.html` for client routes; the existing public Nginx remains the one exposed entry point and routes `/api/` plus backend protocol/media paths to Lemmy. Preserve ActivityPub content negotiation for paths that overlap with frontend routes, notably `/communities/`.
- **Keep the browser API URL relative.** The frontend production app uses same-origin API paths; no build-time backend hostname or browser CORS configuration is needed. Internal upstream hostnames remain Docker service names.
- **Retain the stable proxy upstream role.** The Compose service and Nginx upstream can use a neutral frontend name so replacing the implementation does not require frontend-specific behavior in routing rules.

Alternatives considered: building frontend assets into the Rust image would increase coupling and rebuild cost; serving assets directly from the public proxy container would complicate image ownership and static-site fallback configuration. A separate internal static server keeps those responsibilities distinct.

## Risks / Trade-offs

- [Adjacent frontend checkout is absent or uses a different path] → Make the Compose build context configurable and document the expected path and required checkout.
- [Elm compilation needs a writable cache and can lengthen clean builds] → Set `ELM_HOME` within the build stage and use normal Docker layer caching for dependency installation.
- [An incomplete backend route list can return SPA HTML for protocol requests] → Compare Nginx routes with the frontend deployment sample and preserve backend paths and content-negotiation routing.
- [Missing translation catalog requests can be masked by SPA fallback] → Ensure static asset paths return 404 when files are absent rather than returning `index.html`.

## Migration Plan

1. Build the frontend image and start it with the existing backend services.
2. Update Nginx routing and verify root, deep frontend routes, API, media, and ActivityPub requests through the public ports.
3. Recreate the proxy and frontend services to switch traffic.
4. Roll back by restoring the prior Compose and Nginx definitions and restarting the prior `lemmy-ui` image service; no data migration is involved.

## Open Questions

None. The frontend checkout path can be configurable with a default matching the documented adjacent clone location.
