# syntax=docker/dockerfile:1.20

FROM node:22-bookworm-slim AS builder
WORKDIR /app
ENV ELM_HOME=/tmp/elm-home

RUN corepack enable && corepack prepare pnpm@10.6.5 --activate

COPY --from=frontend /package.json /pnpm-lock.yaml /pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile

COPY --from=frontend /elm.json /index.html /vite.config.js ./
COPY --from=frontend /src ./src
COPY --from=frontend /scripts ./scripts
COPY --from=frontend /public ./public
COPY --from=frontend /translations ./translations
RUN pnpm run build

FROM nginx:1-alpine AS runner
COPY --from=builder /app/dist /usr/share/nginx/html
COPY docker/frontend-nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
