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

run_debian() {
    echo -e "${YELLOW}Building and running Debian smoke tests...${NC}"
    docker build -t schalentier-smoke-debian --target smoke-debian -f tests/smoke/Dockerfile .
    docker run --rm schalentier-smoke-debian
}

run_arch() {
    echo -e "${YELLOW}Building and running Arch Linux smoke tests...${NC}"
    docker build -t schalentier-smoke-arch --target smoke-arch -f tests/smoke/Dockerfile .
    docker run --rm schalentier-smoke-arch
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
