#!/usr/bin/env bash
set -eo pipefail

# ─── Configuration ───────────────────────────────────────────────────────────
ADMIN_PORT=19000
PROXY_PORT=18080
ECHO_PORT=18888
DB_HOST=localhost
DB_PORT=5555
DB_USER=gate
DB_PASS=gate
DB_NAME=gate_e2e_test
DATABASE_URL="postgres://${DB_USER}:${DB_PASS}@${DB_HOST}:${DB_PORT}/${DB_NAME}"

ADMIN_URL="http://127.0.0.1:${ADMIN_PORT}"
PROXY_URL="http://127.0.0.1:${PROXY_PORT}"

# Auth token for E2E admin API calls
E2E_ADMIN_TOKEN="e2e-test-token"

PASSED=0
FAILED=0
PIDS=()

# ─── Colors ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# ─── Helpers ─────────────────────────────────────────────────────────────────
log()  { echo -e "${YELLOW}[e2e]${NC} $*"; }
pass() { echo -e "  ${GREEN}✓${NC} $1"; PASSED=$((PASSED + 1)); }
fail() { echo -e "  ${RED}✗${NC} $1 (got: $2)"; FAILED=$((FAILED + 1)); }

assert_status() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        pass "$desc"
    else
        fail "$desc" "HTTP $actual, expected $expected"
    fi
}

# Admin API helper — adds auth token automatically
admin_curl() {
    curl -H "X-Admin-Token: ${E2E_ADMIN_TOKEN}" "$@"
}

cleanup() {
    log "Cleaning up..."
    if [ ${#PIDS[@]} -gt 0 ]; then
        # Use SIGINT — Pingora exits cleanly on SIGINT (not SIGTERM),
        # which allows LLVM coverage profraw data to be flushed via atexit.
        for pid in "${PIDS[@]}"; do
            kill -INT "$pid" 2>/dev/null || true
        done
        sleep 3
        # Force-kill anything still running
        for pid in "${PIDS[@]}"; do
            kill -9 "$pid" 2>/dev/null || true
        done
    fi

    # Drop test database — terminate active connections first
    local container
    container=$(docker ps -q --filter "publish=${DB_PORT}" | head -1)
    if [ -n "$container" ]; then
        docker exec "$container" psql -U "$DB_USER" -d gate \
            -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${DB_NAME}' AND pid <> pg_backend_pid();" 2>/dev/null || true
        sleep 0.5
        docker exec "$container" psql -U "$DB_USER" -d gate \
            -c "DROP DATABASE IF EXISTS ${DB_NAME};" 2>/dev/null || true
    fi
    log "Cleanup complete."
}
trap cleanup EXIT

# ─── Prerequisites ───────────────────────────────────────────────────────────
log "Checking prerequisites..."

# Check Postgres is reachable
PG_CONTAINER=$(docker ps -q --filter "publish=${DB_PORT}" | head -1)
if [ -z "$PG_CONTAINER" ]; then
    echo "ERROR: No Docker container found with port ${DB_PORT}. Is PostgreSQL running?"
    exit 1
fi
log "PostgreSQL container: ${PG_CONTAINER}"

# Create test database (terminate lingering connections first)
docker exec "$PG_CONTAINER" psql -U "$DB_USER" -d gate \
    -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${DB_NAME}' AND pid <> pg_backend_pid();" 2>/dev/null || true
sleep 0.5
docker exec "$PG_CONTAINER" psql -U "$DB_USER" -d gate \
    -c "DROP DATABASE IF EXISTS ${DB_NAME};" 2>/dev/null || true
docker exec "$PG_CONTAINER" psql -U "$DB_USER" -d gate \
    -c "CREATE DATABASE ${DB_NAME};" 2>/dev/null
log "Created database ${DB_NAME}"

# ─── Build ───────────────────────────────────────────────────────────────────
ADMIN_BIN="${ADMIN_BIN:-}"
PROXY_BIN="${PROXY_BIN:-}"

if [ -z "$ADMIN_BIN" ] || [ -z "$PROXY_BIN" ]; then
    log "Building binaries..."
    . "$HOME/.cargo/env"
    cargo build -p admin -p proxy --release 2>&1 | tail -1
    ADMIN_BIN="./target/release/admin"
    PROXY_BIN="./target/release/proxy"
else
    log "Using provided binaries: ADMIN_BIN=$ADMIN_BIN, PROXY_BIN=$PROXY_BIN"
fi

if [ ! -f "$ADMIN_BIN" ] || [ ! -f "$PROXY_BIN" ]; then
    echo "ERROR: Binaries not found"
    exit 1
fi

# Detect if binaries are debug (unoptimized) builds — need longer timeouts
IS_DEBUG=false
if [[ "$ADMIN_BIN" == *"/debug/"* ]]; then
    IS_DEBUG=true
    log "Detected debug binaries — using extended timeouts"
fi

# Timeouts: debug builds are slower
if $IS_DEBUG; then
    STARTUP_WAIT=4
    RELOAD_WAIT=4
else
    STARTUP_WAIT=2
    RELOAD_WAIT=2
fi

# ─── Start echo server ──────────────────────────────────────────────────────
log "Starting echo server on :${ECHO_PORT}..."
python3 -c "
from http.server import HTTPServer, BaseHTTPRequestHandler

class EchoHandler(BaseHTTPRequestHandler):
    def do_ANY(self):
        cl = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(cl) if cl > 0 else b'ok'
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    do_GET = do_POST = do_PUT = do_DELETE = do_PATCH = do_HEAD = do_ANY
    def log_message(self, *args): pass

HTTPServer(('127.0.0.1', ${ECHO_PORT}), EchoHandler).serve_forever()
" &
PIDS+=($!)
sleep 0.5

# ─── Start admin ─────────────────────────────────────────────────────────────
log "Starting admin on :${ADMIN_PORT}..."
DATABASE_URL="$DATABASE_URL" \
    ADMIN_PORT="$ADMIN_PORT" \
    ADMIN_BIND_ADDR="127.0.0.1" \
    ADMIN_TOKEN="$E2E_ADMIN_TOKEN" \
    LOG_LEVEL="warn" \
    "$ADMIN_BIN" &>/dev/null &
PIDS+=($!)

# Wait for admin readiness
for i in $(seq 1 30); do
    if curl -sf "${ADMIN_URL}/admin/health" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done

if ! curl -sf "${ADMIN_URL}/admin/health" >/dev/null 2>&1; then
    echo "ERROR: Admin did not become ready within 15s"
    exit 1
fi
log "Admin is ready"

# ─── Start proxy ─────────────────────────────────────────────────────────────
log "Starting proxy on :${PROXY_PORT}..."
DATABASE_URL="$DATABASE_URL" \
    PROXY_PORT="$PROXY_PORT" \
    METRICS_PORT="19091" \
    LOG_LEVEL="warn" \
    CONFIG_POLL_INTERVAL_SECS="1" \
    HEALTH_CHECK_INTERVAL_SECS="60" \
    "$PROXY_BIN" &>/dev/null &
PIDS+=($!)
sleep "$STARTUP_WAIT"
log "Proxy started"

# ═══════════════════════════════════════════════════════════════════════════════
# TEST SCENARIOS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
log "Running test scenarios..."
echo ""

# ─── 1. Basic Routing ────────────────────────────────────────────────────────
echo "── Basic Routing ──"

# Create upstream with echo target
UPSTREAM=$(admin_curl -sf -X POST "${ADMIN_URL}/admin/upstreams" \
    -H "Content-Type: application/json" \
    -d '{"name":"echo-upstream"}')
UPSTREAM_ID=$(echo "$UPSTREAM" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

# Add target
admin_curl -sf -X POST "${ADMIN_URL}/admin/upstreams/${UPSTREAM_ID}/targets" \
    -H "Content-Type: application/json" \
    -d "{\"host\":\"127.0.0.1\",\"port\":${ECHO_PORT}}" >/dev/null

# Create route
ROUTE=$(admin_curl -sf -X POST "${ADMIN_URL}/admin/routes" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"echo-route\",\"path_prefix\":\"/echo\",\"upstream_id\":\"${UPSTREAM_ID}\",\"strip_prefix\":true}")
ROUTE_ID=$(echo "$ROUTE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

# Wait for config reload
sleep "$RELOAD_WAIT"

# Test proxy forwards to echo
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "${PROXY_URL}/echo/" 2>/dev/null || echo "000")
assert_status "Proxy GET /echo/ returns 200" "200" "$STATUS"

# Test unmatched route
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "${PROXY_URL}/nonexistent" 2>/dev/null)
assert_status "Proxy GET /nonexistent returns 404 (no route)" "404" "$STATUS"

# ─── 2. Body Size Limit ─────────────────────────────────────────────────────
echo ""
echo "── Body Size Limit ──"

# Update route with body limit
admin_curl -sf -X PUT "${ADMIN_URL}/admin/routes/${ROUTE_ID}" \
    -H "Content-Type: application/json" \
    -d '{"max_body_bytes":100}' >/dev/null

sleep "$RELOAD_WAIT"

# Small body should pass
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${PROXY_URL}/echo/" \
    -H "Content-Type: text/plain" \
    -d "small body" 2>/dev/null)
assert_status "POST /echo/ small body returns 200" "200" "$STATUS"

# Large body should fail with 413
LARGE_BODY=$(python3 -c "print('x' * 200)")
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${PROXY_URL}/echo/" \
    -H "Content-Type: text/plain" \
    -d "$LARGE_BODY" 2>/dev/null || echo "000")
assert_status "POST /echo/ 200-byte body returns 413" "413" "$STATUS"

# Remove body limit for subsequent tests
admin_curl -sf -X PUT "${ADMIN_URL}/admin/routes/${ROUTE_ID}" \
    -H "Content-Type: application/json" \
    -d '{"max_body_bytes":null}' >/dev/null

sleep "$RELOAD_WAIT"

# ─── 3. API Key Auth ────────────────────────────────────────────────────────
echo ""
echo "── API Key Auth ──"

# Create API key scoped to our route
KEY_RESP=$(admin_curl -sf -X POST "${ADMIN_URL}/admin/api-keys" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"test-key\",\"route_id\":\"${ROUTE_ID}\"}")
API_KEY=$(echo "$KEY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['key'])")

sleep "$RELOAD_WAIT"

# Without key → 401
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "${PROXY_URL}/echo/" 2>/dev/null || echo "000")
assert_status "GET /echo/ without key returns 401" "401" "$STATUS"

# With key → 200
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" -H "X-API-Key: ${API_KEY}" \
    "${PROXY_URL}/echo/" 2>/dev/null || echo "000")
assert_status "GET /echo/ with valid key returns 200" "200" "$STATUS"

# ─── 4. Rate Limiting ───────────────────────────────────────────────────────
echo ""
echo "── Rate Limiting ──"

# Create rate limit: 2 rps
admin_curl -sf -X POST "${ADMIN_URL}/admin/rate-limits" \
    -H "Content-Type: application/json" \
    -d "{\"route_id\":\"${ROUTE_ID}\",\"requests_per_second\":2}" >/dev/null

sleep "$RELOAD_WAIT"

# Fire 3 requests rapidly — third should be rate limited (429)
STATUS1=$(curl -s -o /dev/null -w "%{http_code}" -H "X-API-Key: ${API_KEY}" \
    "${PROXY_URL}/echo/" 2>/dev/null || echo "000")
STATUS2=$(curl -s -o /dev/null -w "%{http_code}" -H "X-API-Key: ${API_KEY}" \
    "${PROXY_URL}/echo/" 2>/dev/null || echo "000")
STATUS3=$(curl -s -o /dev/null -w "%{http_code}" -H "X-API-Key: ${API_KEY}" \
    "${PROXY_URL}/echo/" 2>/dev/null || echo "000")

# At least one of the last two should be 429
if [ "$STATUS3" = "429" ] || [ "$STATUS2" = "429" ]; then
    pass "Rate limit triggers 429 on rapid requests"
else
    fail "Rate limit should return 429" "got $STATUS1, $STATUS2, $STATUS3"
fi

# ─── 5. Service Metadata ────────────────────────────────────────────────────
echo ""
echo "── Service Metadata (Admin API) ──"

# Create upstream for service
SVC_UP=$(admin_curl -sf -X POST "${ADMIN_URL}/admin/upstreams" \
    -H "Content-Type: application/json" \
    -d '{"name":"svc-upstream"}')
SVC_UP_ID=$(echo "$SVC_UP" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

# Insert service directly via admin SQL-backed endpoint isn't available without import
# Instead, test list, which should be empty initially for new namespace
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" \
    -H "X-Admin-Token: ${E2E_ADMIN_TOKEN}" \
    "${ADMIN_URL}/admin/services" 2>/dev/null || echo "000")
assert_status "GET /admin/services returns 200" "200" "$STATUS"

# Verify stats endpoint works
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" \
    -H "X-Admin-Token: ${E2E_ADMIN_TOKEN}" \
    "${ADMIN_URL}/admin/stats" 2>/dev/null || echo "000")
assert_status "GET /admin/stats returns 200" "200" "$STATUS"

# ═══════════════════════════════════════════════════════════════════════════════
# RESULTS
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "═══════════════════════════════════════"
echo -e "  ${GREEN}Passed: ${PASSED}${NC}"
if [ "$FAILED" -gt 0 ]; then
    echo -e "  ${RED}Failed: ${FAILED}${NC}"
fi
echo "═══════════════════════════════════════"
echo ""

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
