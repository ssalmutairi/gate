#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Ensure cargo is in PATH
if ! command -v cargo &>/dev/null; then
    . "$HOME/.cargo/env"
fi

echo "======================================="
echo "  Gate Coverage Report"
echo "======================================="
echo ""

# ─── Rust coverage ───────────────────────────────────────────────────────────
echo "── Rust coverage (cargo-llvm-cov) ──"

if ! command -v cargo-llvm-cov &>/dev/null; then
    echo "Installing cargo-llvm-cov..."
    cargo install cargo-llvm-cov
fi

cd "$ROOT_DIR"

# 1. Clean previous coverage data
echo "Cleaning previous coverage data..."
cargo llvm-cov clean --workspace

# 2. Run unit + integration tests (profraw → target/llvm-cov-target/)
echo "Running workspace tests with coverage instrumentation..."
cargo llvm-cov test --no-report --workspace -- --test-threads=1 2>&1 | tail -20

# 3. Build instrumented binaries for E2E into the same target dir.
#    cargo llvm-cov test already built admin (has integration tests),
#    but proxy has no integration tests so we need to build it separately.
echo ""
echo "Building instrumented E2E binaries..."
eval "$(cargo llvm-cov show-env --export-prefix 2>&1 | grep '^export ')"
# Override target dir to match where cargo llvm-cov test puts its artifacts,
# and set profraw output to the same directory for E2E processes.
export CARGO_TARGET_DIR="$PWD/target/llvm-cov-target"
export LLVM_PROFILE_FILE="$PWD/target/llvm-cov-target/e2e-%p-%10m.profraw"
cargo build -p admin -p proxy 2>&1 | tail -5

BIN_DIR="./target/llvm-cov-target/debug"
echo "  Binary dir: $BIN_DIR"
echo "  admin: $(test -f "$BIN_DIR/admin" && echo 'found' || echo 'MISSING')"
echo "  proxy: $(test -f "$BIN_DIR/proxy" && echo 'found' || echo 'MISSING')"

# 4. Run E2E tests with instrumented binaries
if [ -f "tests/e2e/run.sh" ] && [ -f "$BIN_DIR/admin" ] && [ -f "$BIN_DIR/proxy" ]; then
    echo ""
    echo "Running E2E tests with instrumented binaries..."
    echo "  ADMIN_BIN=$BIN_DIR/admin"
    echo "  PROXY_BIN=$BIN_DIR/proxy"
    ADMIN_BIN="$BIN_DIR/admin" \
    PROXY_BIN="$BIN_DIR/proxy" \
    bash tests/e2e/run.sh || echo "WARNING: E2E tests had failures (coverage data still collected)"
else
    echo ""
    echo "Skipping E2E tests (binaries or test script not found)"
fi

# 5. Generate combined report (reads all profraw from target/llvm-cov-target/)
#    Unset show-env vars so cargo llvm-cov report uses its default paths.
unset CARGO_TARGET_DIR LLVM_PROFILE_FILE RUSTC_WRAPPER CARGO_LLVM_COV \
      CARGO_LLVM_COV_SHOW_ENV CARGO_LLVM_COV_TARGET_DIR CARGO_LLVM_COV_BUILD_DIR \
      __CARGO_LLVM_COV_RUSTC_WRAPPER __CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS \
      __CARGO_LLVM_COV_RUSTC_WRAPPER_CRATE_NAMES 2>/dev/null || true

echo ""
echo "── Combined coverage report ──"
cargo llvm-cov report --summary-only
cargo llvm-cov report --html --output-dir target/llvm-cov-html

echo ""
echo "Rust coverage report: target/llvm-cov-html/index.html"

# ─── Dashboard coverage ─────────────────────────────────────────────────────
if [ -d "$ROOT_DIR/dashboard" ] && [ -f "$ROOT_DIR/dashboard/package.json" ]; then
    echo ""
    echo "── Dashboard coverage (vitest + v8) ──"
    cd "$ROOT_DIR/dashboard"
    npm run test:coverage 2>&1 | tail -20 || echo "WARNING: Dashboard tests had failures"
    echo ""
    echo "Dashboard coverage report: dashboard/coverage/index.html"
fi

echo ""
echo "======================================="
echo "  Done!"
echo "======================================="
