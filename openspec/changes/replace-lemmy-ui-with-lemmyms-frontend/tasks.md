# Tasks

## 1. Build and serve the frontend

- [x] 1.1 Add or update GitHub Actions to check out backend submodules, build backend and frontend images, and publish both to GHCR with branch and commit SHA tags; verify workflow YAML syntax and image metadata configuration.
- [x] 1.2 Keep the frontend production Docker build on the `lemmyms-frontend` GitHub source, with `lemmy-ms` as its default revision, approved pnpm build scripts, a writable Elm cache, and optional proxy build arguments; verify the proxy defaults to disabled.

## 2. Route the public site

- [x] 2.1 Update Nginx so frontend assets and deep links reach the static frontend while API, federation, and media paths reach Lemmy; preserve ActivityPub content negotiation and return 404 for missing translation assets, verifying the route rules against the frontend deployment sample.
- [x] 2.2 Change standard Compose services to pull the backend and frontend GHCR packages using one configurable image tag, remove the process-wide activity sending disable flag, and verify `docker compose config` contains only published image references while the existing tenant-scoped federation settings remain controlling.

## 3. Document and integrate

- [x] 3.1 Document image publication, GHCR authentication/visibility, tag selection, optional Actions npm proxy setup, and same-origin routing; verify the documented deployment commands match Compose without prescribing host paths.
- [ ] 3.2 Verify the workflow definition, Compose image/tag resolution, and published images, then exercise root, deep route, API, media, and ActivityPub routes through the public proxy with one federation-disabled tenant and one enabled tenant.
