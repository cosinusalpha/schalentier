#!/bin/bash
# Build static musl binaries for Linux
# Usage:
#   ./build-musl.sh              # Build for current architecture
#   ./build-musl.sh x86_64       # Build for x86_64
#   ./build-musl.sh aarch64      # Build for ARM64
#   ./build-musl.sh all          # Build for both architectures
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Detect host architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             echo "x86_64" ;;  # Default
    esac
}

# Build for a specific target
build_target() {
    local arch="$1"
    local target="${arch}-unknown-linux-musl"
    local output_dir="target/${target}/release"

    echo "=============================================="
    echo "Building for ${target}..."
    echo "=============================================="

    # Use Docker/Podman
    local container_cmd="docker"
    if command -v podman &>/dev/null; then
        container_cmd="podman"
    fi

    if [ "$arch" = "x86_64" ]; then
        # Native x86_64 build
        $container_cmd run --rm \
            -v "$SCRIPT_DIR":/build \
            -w /build \
            rust:latest \
            sh -c "
                rustup target add ${target} && \
                apt-get update && apt-get install -y musl-tools && \
                cargo build --release --target ${target}
            "
    elif [ "$arch" = "aarch64" ]; then
        # ARM64 cross-compilation
        $container_cmd run --rm \
            -v "$SCRIPT_DIR":/build \
            -w /build \
            rust:latest \
            sh -c "
                rustup target add ${target} && \
                apt-get update && apt-get install -y musl-tools gcc-aarch64-linux-gnu && \
                export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc && \
                export CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc && \
                cargo build --release --target ${target}
            "
    else
        echo "Unknown architecture: $arch"
        exit 1
    fi

    echo ""
    echo "Binary built: ${output_dir}/schalentier"
    ls -la "${output_dir}/schalentier"

    echo ""
    echo "Binary info:"
    file "${output_dir}/schalentier"
    echo ""
}

# Main
main() {
    local requested="${1:-$(detect_arch)}"

    case "$requested" in
        x86_64|x64|amd64)
            build_target "x86_64"
            ;;
        aarch64|arm64|arm)
            build_target "aarch64"
            ;;
        all|both)
            build_target "x86_64"
            build_target "aarch64"
            ;;
        *)
            echo "Usage: $0 [x86_64|aarch64|all]"
            echo ""
            echo "  x86_64   - Build for x86_64 (Intel/AMD)"
            echo "  aarch64  - Build for ARM64 (Apple Silicon, Raspberry Pi, etc.)"
            echo "  all      - Build for both architectures"
            echo ""
            echo "Default: build for current architecture ($(detect_arch))"
            exit 1
            ;;
    esac

    echo "=============================================="
    echo "Build complete!"
    echo "=============================================="
}

main "$@"
