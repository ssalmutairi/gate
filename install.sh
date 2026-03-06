#!/usr/bin/env bash
set -euo pipefail

REPO="ssalmutairi/gate"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
MIGRATIONS_DIR="${MIGRATIONS_DIR:-/usr/local/share/gate/migrations}"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux)  OS_TARGET="unknown-linux-gnu" ;;
    Darwin) OS_TARGET="apple-darwin" ;;
    *)      echo "Error: unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)  ARCH_TARGET="x86_64" ;;
    aarch64|arm64) ARCH_TARGET="aarch64" ;;
    *)             echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"

# Determine version
if [ -z "${VERSION:-}" ]; then
    echo "Fetching latest release..."
    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')"
fi

ARCHIVE="gate-${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
CHECKSUMS_URL="https://github.com/${REPO}/releases/download/${VERSION}/SHA256SUMS.txt"

echo "Installing Gate ${VERSION} for ${TARGET}..."

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

# Download archive and checksums
echo "Downloading ${URL}..."
curl -fsSL -o "${TMPDIR}/${ARCHIVE}" "$URL"
curl -fsSL -o "${TMPDIR}/SHA256SUMS.txt" "$CHECKSUMS_URL"

# Verify checksum
echo "Verifying checksum..."
cd "$TMPDIR"
if command -v sha256sum &>/dev/null; then
    grep "$ARCHIVE" SHA256SUMS.txt | sha256sum -c --quiet
elif command -v shasum &>/dev/null; then
    grep "$ARCHIVE" SHA256SUMS.txt | shasum -a 256 -c --quiet
else
    echo "Warning: no sha256sum or shasum found, skipping verification"
fi

# Extract
tar xzf "$ARCHIVE"
EXTRACT_DIR="${ARCHIVE%.tar.gz}"

# Install binaries
echo "Installing binaries to ${INSTALL_DIR}..."
if [ -w "$INSTALL_DIR" ]; then
    cp "${EXTRACT_DIR}/gate-proxy" "${INSTALL_DIR}/"
    cp "${EXTRACT_DIR}/gate-admin" "${INSTALL_DIR}/"
    cp "${EXTRACT_DIR}/gate-portable" "${INSTALL_DIR}/"
    chmod +x "${INSTALL_DIR}/gate-proxy" "${INSTALL_DIR}/gate-admin" "${INSTALL_DIR}/gate-portable"
else
    sudo cp "${EXTRACT_DIR}/gate-proxy" "${INSTALL_DIR}/"
    sudo cp "${EXTRACT_DIR}/gate-admin" "${INSTALL_DIR}/"
    sudo cp "${EXTRACT_DIR}/gate-portable" "${INSTALL_DIR}/"
    sudo chmod +x "${INSTALL_DIR}/gate-proxy" "${INSTALL_DIR}/gate-admin" "${INSTALL_DIR}/gate-portable"
fi

# Install migrations
echo "Installing migrations to ${MIGRATIONS_DIR}..."
if [ -w "$(dirname "$MIGRATIONS_DIR")" ]; then
    mkdir -p "$MIGRATIONS_DIR"
    cp -r "${EXTRACT_DIR}/migrations/"* "$MIGRATIONS_DIR/"
else
    sudo mkdir -p "$MIGRATIONS_DIR"
    sudo cp -r "${EXTRACT_DIR}/migrations/"* "$MIGRATIONS_DIR/"
fi

echo ""
echo "Gate ${VERSION} installed successfully!"
echo ""
echo "  gate-proxy      → ${INSTALL_DIR}/gate-proxy"
echo "  gate-admin      → ${INSTALL_DIR}/gate-admin"
echo "  gate-portable → ${INSTALL_DIR}/gate-portable"
echo "  migrations      → ${MIGRATIONS_DIR}/"
echo ""
echo "Quick start (standalone, no dependencies):"
echo "  gate-portable"
echo ""
echo "Full mode (requires PostgreSQL):"
echo "  Set DATABASE_URL and run gate-admin + gate-proxy"
