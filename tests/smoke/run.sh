#!/bin/bash
# Run smoke tests in Docker containers
# Usage: ./tests/smoke/run.sh [debian|arch|all]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Prefer podman, fall back to docker. Override with ENGINE=docker ./run.sh
ENGINE="${ENGINE:-}"
if [ -z "$ENGINE" ]; then
    if command -v podman > /dev/null 2>&1; then
        ENGINE=podman
    elif command -v docker > /dev/null 2>&1; then
        ENGINE=docker
    else
        echo -e "${RED}Neither podman nor docker found in PATH.${NC}" >&2
        exit 1
    fi
fi
echo -e "${YELLOW}Using container engine: ${ENGINE}${NC}"

# The image is self-building (compiles the binary in a builder stage), so no host
# Rust toolchain or pre-built binary is required.

# Pin the build/run platform to the host arch. This prevents a stale mismatched image
# in the local cache (e.g. an arm64 debian:bookworm-slim) from being used on an amd64
# host, which fails at runtime with "exec format error".
case "$(uname -m)" in
    x86_64|amd64)  PLATFORM=linux/amd64 ;;
    aarch64|arm64) PLATFORM=linux/arm64 ;;
    *)             PLATFORM="" ;;
esac
PLATFORM_ARG=()
if [ -n "$PLATFORM" ]; then
    PLATFORM_ARG=(--platform "$PLATFORM")
    echo -e "${YELLOW}Platform: ${PLATFORM}${NC}"
fi

run_debian() {
    echo -e "${YELLOW}Building and running Debian smoke tests...${NC}"
    "$ENGINE" build "${PLATFORM_ARG[@]}" -t schalentier-smoke-debian --target smoke-debian -f tests/smoke/Dockerfile .
    "$ENGINE" run --rm "${PLATFORM_ARG[@]}" schalentier-smoke-debian
}

run_arch() {
    echo -e "${YELLOW}Building and running Arch Linux smoke tests...${NC}"
    "$ENGINE" build "${PLATFORM_ARG[@]}" -t schalentier-smoke-arch --target smoke-arch -f tests/smoke/Dockerfile .
    "$ENGINE" run --rm "${PLATFORM_ARG[@]}" schalentier-smoke-arch
}

case "${1:-all}" in
    debian)
        run_debian
        ;;
    arch)
        run_arch
        ;;
    all)
        run_debian
        echo ""
        run_arch
        ;;
    *)
        echo "Usage: $0 [debian|arch|all]"
        exit 1
        ;;
esac

echo ""
echo -e "${GREEN}All smoke tests completed!${NC}"
