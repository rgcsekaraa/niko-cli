#!/bin/sh
set -e

# Repository configuration
REPO_OWNER="rgcsekaraa"
REPO_NAME="niko-cli"
BIN_NAME="niko"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

log() {
    echo "${GREEN}[niko-install]${NC} $1"
}

error() {
    echo "${RED}[niko-install] Error:${NC} $1"
    exit 1
}

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux*)     OS_TYPE="linux";;
    Darwin*)    OS_TYPE="darwin";;
    *)          error "Unsupported operating system: $OS";;
esac

# Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)    ARCH_TYPE="amd64";;
    aarch64)   ARCH_TYPE="arm64";;
    arm64)     ARCH_TYPE="arm64";;
    *)         error "Unsupported architecture: $ARCH";;
esac

ASSET_NAME="niko-${OS_TYPE}-${ARCH_TYPE}"

# Determine version (latest if not specified)
if [ -z "$VERSION" ]; then
    log "Fetching latest version..."
    LATEST_URL="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
    VERSION=$(curl -s $LATEST_URL | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    
    if [ -z "$VERSION" ]; then
        error "Could not determine latest version."
    fi
else
    # Ensure version starts with v
    case "$VERSION" in
        v*) ;;
        *) VERSION="v$VERSION" ;;
    esac
fi

log "Detected: ${OS_TYPE} ${ARCH_TYPE}"
log "Installing ${BIN_NAME} ${VERSION}..."

# Download URL
DOWNLOAD_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${VERSION}/${ASSET_NAME}"

# Create temp directory
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# Download
TARGET_FILE="${TMP_DIR}/${BIN_NAME}"
log "Downloading from ${DOWNLOAD_URL}..."
status=$(curl -sL -w "%{http_code}" -o "$TARGET_FILE" "$DOWNLOAD_URL")

if [ "$status" != "200" ]; then
    error "Download failed. Status code: $status. Check if release exists."
fi

chmod +x "$TARGET_FILE"

# Install location
# Try /usr/local/bin first (requires sudo usually), then ~/.local/bin
INSTALL_DIR="/usr/local/bin"
USE_SUDO=0

if [ ! -w "$INSTALL_DIR" ]; then
    if command -v sudo >/dev/null 2>&1; then
        log "Need sudo access to install to $INSTALL_DIR"
        USE_SUDO=1
    else
        INSTALL_DIR="$HOME/.local/bin"
        mkdir -p "$INSTALL_DIR"
        log "Installing to $INSTALL_DIR (no sudo available)"
    fi
fi

if [ "$USE_SUDO" -eq 1 ]; then
    sudo mv "$TARGET_FILE" "$INSTALL_DIR/$BIN_NAME"
else
    mv "$TARGET_FILE" "$INSTALL_DIR/$BIN_NAME"
fi

log "Successfully installed to $INSTALL_DIR/$BIN_NAME"
log "Run 'niko --version' to verify."
