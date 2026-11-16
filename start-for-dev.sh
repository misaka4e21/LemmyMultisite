#!/usr/bin/env bash
# Start a disposable Lemmy server with its own PostgreSQL cluster.
#
# Everything lives under a temporary directory and is removed on exit. Useful for
# manually exercising the API without touching the development database.
# Set LEMMY_TEST_START_MEDIA=1 to also start an isolated RustFS and pict-rs stack.

set -Eeuo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_ROOT"

LEMMY_TEST_HOSTNAME="${LEMMY_TEST_HOSTNAME:-localhost}"
# Tenant resolution compares bare hostnames; the port is configured separately.
LEMMY_TEST_HOSTNAME="${LEMMY_TEST_HOSTNAME%%:*}"
LEMMY_TEST_PORT="${LEMMY_TEST_PORT:-8536}"
LEMMY_TEST_ADMIN_USERNAME="${LEMMY_TEST_ADMIN_USERNAME:-admin}"
LEMMY_TEST_ADMIN_PASSWORD="${LEMMY_TEST_ADMIN_PASSWORD:-lemmy-test-password}"
LEMMY_TEST_ADMIN_EMAIL="${LEMMY_TEST_ADMIN_EMAIL:-admin@example.com}"
LEMMY_TEST_INSTALLATION_ADMIN_USERNAME="${LEMMY_TEST_INSTALLATION_ADMIN_USERNAME:-lemmy-test-installation-admin}"
LEMMY_TEST_INSTALLATION_ADMIN_PASSWORD="${LEMMY_TEST_INSTALLATION_ADMIN_PASSWORD:-lemmy-test-installation-password}"
LEMMY_TEST_START_MEDIA="${LEMMY_TEST_START_MEDIA:-0}"
LEMMY_TEST_DISABLE_S3="${LEMMY_TEST_DISABLE_S3:-0}"

pick_free_port() {
  if command -v python3 >/dev/null 2>&1; then
    python3 - <<'PY'
import socket

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
  else
    echo 55432
  fi
}

for command in initdb pg_ctl createdb cargo curl; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "error: required command '$command' was not found in PATH" >&2
    exit 1
  fi
done

if [[ "$LEMMY_TEST_START_MEDIA" == 1 ]]; then
  if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
    echo "error: LEMMY_TEST_START_MEDIA=1 requires Docker Compose" >&2
    exit 1
  fi
elif [[ "$LEMMY_TEST_START_MEDIA" != 0 ]]; then
  echo "error: LEMMY_TEST_START_MEDIA must be 0 or 1" >&2
  exit 1
fi

if [[ "$LEMMY_TEST_DISABLE_S3" != 0 && "$LEMMY_TEST_DISABLE_S3" != 1 ]]; then
  echo "error: LEMMY_TEST_DISABLE_S3 must be 0 or 1" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lemmy-test.XXXXXX")"
PGDATA="$TMP_DIR/pgdata"
PGSOCKET="$TMP_DIR/pgsocket"
PGLOG="$TMP_DIR/postgres.log"
CONFIG_FILE="$TMP_DIR/lemmy-test.hjson"
PGPORT="${LEMMY_TEST_PG_PORT:-$(pick_free_port)}"
DATABASE_NAME="lemmy_test"
DATABASE_URL="postgres://postgres@127.0.0.1:$PGPORT/$DATABASE_NAME"
COMPOSE_PROJECT="lemmy-dev-media-$$"
MEDIA_STARTED=0

PICTRS_CONFIG="null"
S3_CONFIG="null"
if [[ "$LEMMY_TEST_START_MEDIA" == 1 ]]; then
  LEMMY_TEST_RUSTFS_PORT="${LEMMY_TEST_RUSTFS_PORT:-$(pick_free_port)}"
  LEMMY_TEST_PICTRS_PORT="${LEMMY_TEST_PICTRS_PORT:-$(pick_free_port)}"
  LEMMY_TEST_S3_ACCESS_KEY="${LEMMY_TEST_S3_ACCESS_KEY:-lemmy-rustfs}"
  LEMMY_TEST_S3_SECRET_KEY="${LEMMY_TEST_S3_SECRET_KEY:-lemmy-rustfs-secret}"
  LEMMY_TEST_S3_BUCKET="${LEMMY_TEST_S3_BUCKET:-lemmy-files}"
  LEMMY_TEST_S3_REGION="${LEMMY_TEST_S3_REGION:-us-east-1}"
  export RUSTFS_HOST_PORT="$LEMMY_TEST_RUSTFS_PORT"
  export PICTRS_HOST_PORT="$LEMMY_TEST_PICTRS_PORT"
  export S3_ACCESS_KEY="$LEMMY_TEST_S3_ACCESS_KEY"
  export S3_SECRET_KEY="$LEMMY_TEST_S3_SECRET_KEY"
  export S3_ENDPOINT="http://rustfs:9000"
  export S3_FILE_BUCKET="$LEMMY_TEST_S3_BUCKET"
  export S3_REGION="$LEMMY_TEST_S3_REGION"
  PICTRS_CONFIG="{
    url: \"http://127.0.0.1:$LEMMY_TEST_PICTRS_PORT/\"
    api_key: \"my-pictrs-key\"
  }"
  if [[ "$LEMMY_TEST_DISABLE_S3" == 0 ]]; then
    S3_CONFIG="{
      endpoint: \"http://127.0.0.1:$LEMMY_TEST_RUSTFS_PORT\"
      bucket: \"$LEMMY_TEST_S3_BUCKET\"
      region: \"$LEMMY_TEST_S3_REGION\"
      access_key: \"$LEMMY_TEST_S3_ACCESS_KEY\"
      secret_key: \"$LEMMY_TEST_S3_SECRET_KEY\"
      use_path_style: true
    }"
  else
    S3_CONFIG="null"
    unset LEMMY_S3_ENDPOINT LEMMY_S3_BUCKET LEMMY_S3_REGION
    unset LEMMY_S3_ACCESS_KEY LEMMY_S3_SECRET_KEY LEMMY_S3_USE_PATH_STYLE
  fi
fi

if [[ "$LEMMY_TEST_DISABLE_S3" == 1 ]]; then
  S3_CONFIG="null"
  unset LEMMY_S3_ENDPOINT LEMMY_S3_BUCKET LEMMY_S3_REGION
  unset LEMMY_S3_ACCESS_KEY LEMMY_S3_SECRET_KEY LEMMY_S3_USE_PATH_STYLE
fi

SERVER_PID=""
PG_STARTED=0

cleanup() {
  local status=$?
  trap - EXIT INT TERM

  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi

  if [[ "$PG_STARTED" == 1 ]]; then
    pg_ctl -D "$PGDATA" -m fast -w stop >/dev/null 2>&1 || true
  fi

  if [[ "$MEDIA_STARTED" == 1 ]]; then
    docker compose --project-name "$COMPOSE_PROJECT" --project-directory "$REPO_ROOT/docker" \
      down --remove-orphans >/dev/null 2>&1 || true
  fi

  if [[ "${LEMMY_TEST_KEEP_TMP:-0}" == 1 ]]; then
    echo "Temporary files kept at: $TMP_DIR"
  else
    rm -rf "$TMP_DIR"
  fi

  exit "$status"
}
trap cleanup EXIT INT TERM

mkdir -p "$PGSOCKET"

if [[ "$LEMMY_TEST_START_MEDIA" == 1 ]]; then
  export RUSTFS_DATA_DIR="$TMP_DIR/rustfs"
  export PICTRS_DATA_DIR="$TMP_DIR/pictrs"
  mkdir -p "$RUSTFS_DATA_DIR" "$PICTRS_DATA_DIR"
  chmod 777 "$RUSTFS_DATA_DIR" "$PICTRS_DATA_DIR"
  echo "Starting isolated RustFS and pict-rs services"
  MEDIA_STARTED=1
  docker compose --project-name "$COMPOSE_PROJECT" --project-directory "$REPO_ROOT/docker" \
    up --detach --wait rustfs pictrs
fi

echo "Initializing temporary PostgreSQL in $TMP_DIR"
initdb \
  -D "$PGDATA" \
  --username=postgres \
  --auth-local=trust \
  --auth-host=trust \
  --no-sync \
  >/dev/null

pg_ctl \
  -D "$PGDATA" \
  -l "$PGLOG" \
  -o "-p $PGPORT -k $PGSOCKET -c listen_addresses=127.0.0.1 -c fsync=off" \
  -w start \
  >/dev/null
PG_STARTED=1

createdb -h 127.0.0.1 -p "$PGPORT" -U postgres "$DATABASE_NAME"

cat >"$CONFIG_FILE" <<EOF
{
  setup: {
    hostname: "$LEMMY_TEST_HOSTNAME"
    admin_username: "$LEMMY_TEST_ADMIN_USERNAME"
    admin_password: "$LEMMY_TEST_ADMIN_PASSWORD"
    admin_email: "$LEMMY_TEST_ADMIN_EMAIL"
    site_name: "Lemmy test server"
    bootstrap_instances: []
  }
  database: {
    connection: "$DATABASE_URL"
  }
  bind: "127.0.0.1"
  port: $LEMMY_TEST_PORT
  tls_enabled: false
  pictrs: $PICTRS_CONFIG
  s3: $S3_CONFIG
}
EOF

if [[ "${LEMMY_TEST_SKIP_BUILD:-0}" != 1 ]]; then
  echo "Building lemmy_server"
  cargo build -p lemmy_server
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
SERVER_BINARY="$TARGET_DIR/debug/lemmy_server"
if [[ ! -x "$SERVER_BINARY" ]]; then
  echo "error: server binary not found at $SERVER_BINARY" >&2
  exit 1
fi

export LEMMY_CONFIG_LOCATION="$CONFIG_FILE"
export LEMMY_DATABASE_URL="$DATABASE_URL"
export LEMMY_INSTALLATION_ADMIN_USERNAME="$LEMMY_TEST_INSTALLATION_ADMIN_USERNAME"
export LEMMY_INSTALLATION_ADMIN_PASSWORD="$LEMMY_TEST_INSTALLATION_ADMIN_PASSWORD"
export RUST_LOG="${RUST_LOG:-info}"

echo "Starting lemmy_server on http://$LEMMY_TEST_HOSTNAME:$LEMMY_TEST_PORT"
"$SERVER_BINARY" &
SERVER_PID=$!

# Wait until the HTTP listener answers. Migrations run during startup, so this can
# take a while on a fresh database.
READY=0
for _ in $(seq 1 180); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "error: lemmy_server exited during startup" >&2
    wait "$SERVER_PID" || true
    exit 1
  fi
  if curl -fsS \
    -H "Host: $LEMMY_TEST_HOSTNAME" \
    "http://127.0.0.1:$LEMMY_TEST_PORT/api/v4/site" \
    >/dev/null 2>&1; then
    READY=1
    break
  fi
  sleep 1
done

if [[ "$READY" == 1 ]]; then
  MEDIA_STATUS="disabled"
  if [[ "$LEMMY_TEST_START_MEDIA" == 1 ]]; then
    MEDIA_STATUS="RustFS http://127.0.0.1:$LEMMY_TEST_RUSTFS_PORT, pict-rs http://127.0.0.1:$LEMMY_TEST_PICTRS_PORT"
    if [[ "$LEMMY_TEST_DISABLE_S3" == 1 ]]; then
      MEDIA_STATUS="$MEDIA_STATUS (Lemmy document S3 disabled)"
    fi
  fi
  cat <<EOF

Test server is ready:
  URL:   http://$LEMMY_TEST_HOSTNAME:$LEMMY_TEST_PORT
  Admin: $LEMMY_TEST_ADMIN_USERNAME / $LEMMY_TEST_ADMIN_PASSWORD
  DB:    $DATABASE_URL
  Media: $MEDIA_STATUS

Press Ctrl-C to stop the server and remove the temporary database.
Set LEMMY_TEST_KEEP_TMP=1 to keep the temporary files for debugging.

EOF
else
  echo "warning: server did not answer within 180 seconds; check the logs below" >&2
fi

wait "$SERVER_PID"
