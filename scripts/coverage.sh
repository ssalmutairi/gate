#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "═══════════════════════════════════════"
echo "  Gate Coverage Report"
echo "═══════════════════════════════════════"
echo ""

# ─── Rust coverage ───────────────────────────────────────────────────────────
echo "── Rust coverage (cargo-llvm-cov) ──"

if ! command -v cargo-llvm-cov &>/dev/null; then
    echo "Installing cargo-llvm-cov..."
    cargo install cargo-llvm-cov
fi

cd "$ROOT_DIR"

# Run proxy tests (no DB required)
echo "Running proxy unit tests with coverage..."
cargo llvm-cov --package proxy --html --output-dir target/llvm-cov-html 2>&1 | tail -5

echo ""
echo "Rust coverage report: target/llvm-cov-html/index.html"

# ─── Dashboard coverage ─────────────────────────────────────────────────────
echo ""
echo "── Dashboard coverage (vitest + v8) ──"

cd "$ROOT_DIR/dashboard"
npm run test:coverage 2>&1 | tail -20

echo ""
echo "Dashboard coverage report: dashboard/coverage/index.html"

echo ""
echo "═══════════════════════════════════════"
echo "  Done!"
echo "═══════════════════════════════════════"
