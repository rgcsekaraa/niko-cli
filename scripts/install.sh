#!/bin/bash
set -e

# Niko CLI Installer
# Usage: curl -fsSL https://get.niko.dev | sh

REPO="rgcsekaraa/niko-cli"
BINARY_NAME="niko"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

print_banner() {
    echo -e "${CYAN}"
    echo "  _   _ _ _         "
    echo " | \ | (_) | _____  "
    echo " |  \| | | |/ / _ \ "
    echo " | |\  | |   < (_) |"
    echo " |_| \_|_|_|\_\___/ "
    echo -e "${NC}"
    echo "Natural Language to Shell Commands"
    echo ""
}

detect_os() {
    local os
    os="$(uname -s)"
    case "${os}" in
        Linux*)     echo "linux";;
        Darwin*)    echo "darwin";;
        MINGW*|MSYS*|CYGWIN*) echo "windows";;
        *)          echo "unknown";;
    esac
}

detect_arch() {
    local arch
    arch="$(uname -m)"
    case "${arch}" in
        x86_64|amd64)   echo "amd64";;
        arm64|aarch64)  echo "arm64";;
        *)              echo "unknown";;
    esac
}

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

download_binary() {
    local os="$1"
    local arch="$2"
    local version="$3"
    local ext=""

    if [ "$os" = "windows" ]; then
        ext=".exe"
    fi

    local filename="${BINARY_NAME}-${os}-${arch}${ext}"
    local url="https://github.com/${REPO}/releases/download/${version}/${filename}"

    echo -e "${CYAN}Downloading ${filename}...${NC}"

    local tmp_file=$(mktemp)
    if curl -fsSL "$url" -o "$tmp_file"; then
        echo "$tmp_file"
    else
        echo ""
    fi
}

install_binary() {
    local tmp_file="$1"
    local install_path="${INSTALL_DIR}/${BINARY_NAME}"

    # Check if we need sudo
    if [ -w "$INSTALL_DIR" ]; then
        mv "$tmp_file" "$install_path"
        chmod +x "$install_path"
    else
        echo -e "${YELLOW}Installing to ${INSTALL_DIR} requires sudo access${NC}"
        sudo mv "$tmp_file" "$install_path"
        sudo chmod +x "$install_path"
    fi

    echo "$install_path"
}

verify_installation() {
    local install_path="$1"

    if [ -x "$install_path" ]; then
        local version
        version=$("$install_path" version 2>/dev/null || echo "unknown")
        echo -e "${GREEN}✓ Niko ${version} installed successfully!${NC}"
        return 0
    else
        echo -e "${RED}✗ Installation failed${NC}"
        return 1
    fi
}

main() {
    print_banner

    local os=$(detect_os)
    local arch=$(detect_arch)

    if [ "$os" = "unknown" ] || [ "$arch" = "unknown" ]; then
        echo -e "${RED}Unsupported platform: $(uname -s) $(uname -m)${NC}"
        echo "Please build from source: https://github.com/${REPO}"
        exit 1
    fi

    echo -e "Detected: ${CYAN}${os}/${arch}${NC}"

    echo -e "Fetching latest version..."
    local version
    version=$(get_latest_version)

    if [ -z "$version" ]; then
        echo -e "${YELLOW}Could not fetch latest version, using 'latest'${NC}"
        version="latest"
    else
        echo -e "Latest version: ${CYAN}${version}${NC}"
    fi

    local tmp_file
    tmp_file=$(download_binary "$os" "$arch" "$version")

    if [ -z "$tmp_file" ] || [ ! -f "$tmp_file" ]; then
        echo -e "${RED}Failed to download binary${NC}"
        echo ""
        echo "You can install manually:"
        echo "  go install github.com/${REPO}/cmd/niko@latest"
        exit 1
    fi

    echo -e "Installing to ${CYAN}${INSTALL_DIR}${NC}..."
    local install_path
    install_path=$(install_binary "$tmp_file")

    verify_installation "$install_path"

    echo ""
    echo -e "${GREEN}Get started (it's that simple):${NC}"
    echo ""
    echo "  niko \"list all files\""
    echo ""
    echo -e "${YELLOW}First run will auto-download Ollama + AI model (~1GB).${NC}"
    echo -e "${YELLOW}After that, it works offline - no API keys needed!${NC}"
    echo ""
    echo "More examples:"
    echo "  niko \"find files larger than 100MB\""
    echo "  niko \"git commits from last week\""
    echo "  niko \"docker logs for nginx container\""
    echo ""
    echo -e "Documentation: ${CYAN}https://github.com/${REPO}${NC}"
}

main "$@"
