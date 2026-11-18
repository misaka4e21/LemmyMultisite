# Tasks

## 1. Build and serve the frontend

- [ ] 1.1 Add a frontend production Docker build that installs dependencies with Corepack and `pnpm install --frozen-lockfile`, builds with a writable Elm cache, and serves `dist/`; verify the image build succeeds from the documented frontend checkout.
- [ ] 1.2 Add the frontend service to Compose with a configurable local build context and remove the upstream `lemmy-ui` image and its obsolete environment variables; verify `docker compose config` resolves the expected services and paths.

## 2. Route the public site

- [x] 2.1 Update Nginx so frontend assets and deep links reach the static frontend while API, federation, and media paths reach Lemmy; preserve ActivityPub content negotiation and return 404 for missing translation assets, verifying the route rules against the frontend deployment sample.
- [x] 2.2 Update proxy dependencies and internal upstream names to match the new frontend service; verify the parsed Compose YAML has the expected frontend/proxy wiring and no stale `lemmy-ui` references in the standard deployment.

## 3. Document and integrate

- [x] 3.1 Document the frontend checkout location, configurable Compose build context, required build tools, and same-origin routing behavior in the Docker deployment guide; verify documented commands and defaults match Compose.
- [ ] 3.2 Build and start the deployment, then verify the site root, a deep frontend route, an API request, a media request, and ActivityPub content-negotiated requests through the public proxy.
