#!/usr/bin/env bash
# Start a disposable Lemmy server with its own PostgreSQL cluster.
#
# Everything lives under a temporary directory and is removed on exit. Useful for
# manually exercising the API without touching the development database.

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

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/lemmy-test.XXXXXX")"
PGDATA="$TMP_DIR/pgdata"
PGSOCKET="$TMP_DIR/pgsocket"
PGLOG="$TMP_DIR/postgres.log"
CONFIG_FILE="$TMP_DIR/lemmy-test.hjson"
PGPORT="${LEMMY_TEST_PG_PORT:-$(pick_free_port)}"
DATABASE_NAME="lemmy_test"
DATABASE_URL="postgres://postgres@127.0.0.1:$PGPORT/$DATABASE_NAME"

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

  if [[ "${LEMMY_TEST_KEEP_TMP:-0}" == 1 ]]; then
    echo "Temporary files kept at: $TMP_DIR"
  else
    rm -rf "$TMP_DIR"
  fi

  exit "$status"
}
trap cleanup EXIT INT TERM

mkdir -p "$PGSOCKET"

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
  pictrs: null
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
  cat <<EOF

Test server is ready:
  URL:   http://$LEMMY_TEST_HOSTNAME:$LEMMY_TEST_PORT
  Admin: $LEMMY_TEST_ADMIN_USERNAME / $LEMMY_TEST_ADMIN_PASSWORD
  DB:    $DATABASE_URL

Press Ctrl-C to stop the server and remove the temporary database.
Set LEMMY_TEST_KEEP_TMP=1 to keep the temporary files for debugging.

EOF
else
  echo "warning: server did not answer within 180 seconds; check the logs below" >&2
fi

wait "$SERVER_PID"
