# Tasks

## 1. Build and serve the frontend

- [ ] 1.1 Add a frontend production Docker build that fetches the GitHub source, installs dependencies with Corepack and `pnpm install --frozen-lockfile`, builds with a writable Elm cache, and serves the generated assets; verify a clean image build succeeds without a host frontend checkout.
- [x] 1.2 Add the frontend service to Compose with a configurable Git revision, remove any local frontend directory setting and the upstream `lemmy-ui` image and obsolete environment variables; verify `docker compose config` resolves the remote source and expected services.

## 2. Route the public site

- [x] 2.1 Update Nginx so frontend assets and deep links reach the static frontend while API, federation, and media paths reach Lemmy; preserve ActivityPub content negotiation and return 404 for missing translation assets, verifying the route rules against the frontend deployment sample.
- [x] 2.2 Update proxy dependencies and internal upstream names to match the new frontend service; verify the parsed Compose YAML has the expected frontend/proxy wiring and no stale `lemmy-ui` references in the standard deployment.

## 3. Document and integrate

- [x] 3.1 Document the remote frontend source and revision selection, required build tooling and network access, and same-origin routing behavior in the Docker deployment guide; verify the documented commands and configuration match Compose without prescribing a local directory.
- [ ] 3.2 Build and start the deployment, then verify the site root, a deep frontend route, an API request, a media request, and ActivityPub content-negotiated requests through the public proxy.
