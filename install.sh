#!/bin/sh
# Schalentier installer
# Usage: curl -fsSL https://raw.githubusercontent.com/cosinusalpha/schalentier/main/install.sh | sh
#
# Environment variables:
#   SCHALENTIER_INSTALL_DIR - Override install directory (default: ~/.local/bin)
#   SCHALENTIER_VERSION     - Install specific version (default: latest)
#   SCHALENTIER_NO_INIT     - Skip running 'schalentier init' after install

set -e

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

info() {
    printf "${BLUE}info${NC}: %s\n" "$1"
}

success() {
    printf "${GREEN}success${NC}: %s\n" "$1"
}

warn() {
    printf "${YELLOW}warning${NC}: %s\n" "$1"
}

error() {
    printf "${RED}error${NC}: %s\n" "$1" >&2
    exit 1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *)       error "Unsupported operating system: $(uname -s)" ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

# Get the download URL for the binary
get_download_url() {
    local os="$1"
    local arch="$2"
    local version="$3"
    local base_url="https://github.com/cosinusalpha/schalentier/releases"

    # Map to release artifact name (raw binary)
    local artifact
    case "$os" in
        linux)
            case "$arch" in
                x86_64)  artifact="schalentier-linux-x86_64" ;;
                aarch64) artifact="schalentier-linux-aarch64" ;;
                *) error "Unsupported Linux architecture: $arch" ;;
            esac
            ;;
        macos)
            case "$arch" in
                x86_64)  artifact="schalentier-darwin-x86_64" ;;
                aarch64) artifact="schalentier-darwin-aarch64" ;;
                *) error "Unsupported macOS architecture: $arch" ;;
            esac
            ;;
        windows)
            error "Windows install via script is not supported. Use 'cargo install schalentier' or download a release binary manually."
            ;;
        *)
            error "Unsupported OS: $os"
            ;;
    esac

    if [ "$version" = "latest" ]; then
        echo "${base_url}/latest/download/${artifact}"
    else
        echo "${base_url}/download/${version}/${artifact}"
    fi
}

# Check for required tools
check_requirements() {
    local missing=""

    if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
        missing="${missing} curl/wget"
    fi

    if [ -n "$missing" ]; then
        error "Missing required tools:${missing}"
    fi
}

# Download a file
download() {
    local url="$1"
    local output="$2"

    info "Downloading from ${url}..."

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$output" || return 1
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$output" || return 1
    else
        error "Neither curl nor wget found"
    fi
}

main() {
    info "Schalentier Installer"
    echo ""

    # Detect system
    local os=$(detect_os)
    local arch=$(detect_arch)
    local version="${SCHALENTIER_VERSION:-latest}"
    local install_dir="${SCHALENTIER_INSTALL_DIR:-$HOME/.local/bin}"

    info "Detected: ${os} ${arch}"
    info "Version: ${version}"
    info "Install directory: ${install_dir}"
    echo ""

    # Check requirements
    check_requirements

    # Create install directory if it doesn't exist
    if [ ! -d "$install_dir" ]; then
        info "Creating install directory..."
        mkdir -p "$install_dir"
    fi

    # Create temp directory
    local tmp_dir=$(mktemp -d)
    trap "rm -rf '$tmp_dir'" EXIT

    # Get download URL
    local url=$(get_download_url "$os" "$arch" "$version")
    local outfile="$tmp_dir/schalentier"

    # Download
    if ! download "$url" "$outfile"; then
        error "Failed to download schalentier. Check your internet connection and try again."
    fi

    # Binary path
    local binary="$outfile"

    # Install
    info "Installing to ${install_dir}..."
    if [ "$os" = "windows" ]; then
        cp "$binary" "$install_dir/schalentier.exe"
    else
        cp "$binary" "$install_dir/schalentier"
        chmod +x "$install_dir/schalentier"
    fi

    success "Schalentier installed successfully!"
    echo ""

    # Check if install_dir is in PATH
    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *)
            warn "Install directory is not in your PATH."
            echo ""
            echo "Add this to your shell configuration:"
            echo ""
            echo "  export PATH=\"\$PATH:$install_dir\""
            echo ""
            ;;
    esac

    # Run init if not disabled
    if [ -z "$SCHALENTIER_NO_INIT" ]; then
        echo ""
        info "Running 'schalentier init'..."
        echo ""

        if "$install_dir/schalentier" init 2>/dev/null; then
            success "Initialization complete!"
        else
            warn "Initialization failed or already initialized. Run 'schalentier init' manually."
        fi
    fi

    echo ""
    echo "To get started, run:"
    echo ""
    echo "  schalentier --help"
    echo ""
    echo "Add tools with:"
    echo ""
    echo "  schalentier add ripgrep"
    echo "  schalentier add fd"
    echo ""
}

main "$@"
