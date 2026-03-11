#!/bin/bash
set -e

IMAGE="gate-deno"
VERSION="${1:-latest}"
TAG="${IMAGE}:${VERSION}"

echo "Building ${TAG}..."
docker build -t "$TAG" .

if [ "$VERSION" != "latest" ]; then
    docker tag "$TAG" "${IMAGE}:latest"
fi

echo ""
echo "Done: ${TAG} ($(docker image inspect "$TAG" --format='{{.Size}}' | numfmt --to=iec 2>/dev/null || docker image inspect "$TAG" --format='{{.Size}}'))"
