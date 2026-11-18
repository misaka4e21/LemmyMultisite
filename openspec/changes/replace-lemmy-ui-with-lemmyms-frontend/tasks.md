# Tasks

## 1. Build and serve the frontend

- [x] 1.1 Add a frontend production Docker build that fetches the GitHub source, installs dependencies with Corepack and pnpm honoring approved build scripts, builds with a writable Elm cache, and serves the generated assets; verify the proxy is disabled by default and a configured proxy reaches the package registry.
- [x] 1.2 Add the frontend service to Compose with `lemmy-ms` as the default Git revision and configurable revision override, remove any local frontend directory setting and the upstream `lemmy-ui` image and obsolete environment variables; verify `docker compose config` resolves the remote source and expected services.

## 2. Route the public site

- [x] 2.1 Update Nginx so frontend assets and deep links reach the static frontend while API, federation, and media paths reach Lemmy; preserve ActivityPub content negotiation and return 404 for missing translation assets, verifying the route rules against the frontend deployment sample.
- [x] 2.2 Update proxy dependencies and internal upstream names to match the new frontend service; verify the parsed Compose YAML has the expected frontend/proxy wiring and no stale `lemmy-ui` references in the standard deployment.

## 3. Document and integrate

- [x] 3.1 Document the remote frontend source and revision selection, required build tooling and network access, and same-origin routing behavior in the Docker deployment guide; verify the documented commands and configuration match Compose without prescribing a local directory.
- [ ] 3.2 Start the full deployment, then verify the site root, a deep frontend route, an API request, a media request, and ActivityPub content-negotiated requests through the public proxy. The frontend image build and its root/deep-route responses are verified independently; full-stack routing awaits an available Lemmy deployment.
