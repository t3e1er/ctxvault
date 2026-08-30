#!/usr/bin/env bash
set -e

# cxtvault Universal Installer for macOS and Linux
# Installs standalone native binary directly into ~/.local/bin/cxtvault

REPO="${CXTV_GITHUB_REPO:-t3e1er/ctxvault}"
INSTALL_DIR="${CXTV_INSTALL_DIR:-$HOME/.local/bin}"

# 1. Detect architecture & OS
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$OS" in
    darwin)
        if [ "$ARCH" = "arm64" ]; then
            TARGET="aarch64-apple-darwin"
        elif [ "$ARCH" = "x86_64" ]; then
            TARGET="x86_64-apple-darwin"
        else
            echo "[ERROR] Unsupported macOS architecture: $ARCH" >&2
            exit 1
        fi
        ;;
    linux)
        if [ "$ARCH" = "x86_64" ]; then
            TARGET="x86_64-unknown-linux-gnu"
        elif [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
            TARGET="aarch64-unknown-linux-gnu"
        else
            echo "[ERROR] Unsupported Linux architecture: $ARCH" >&2
            exit 1
        fi
        ;;
    *)
        echo "[ERROR] Unsupported operating system: $OS (For Windows, run install.ps1)" >&2
        exit 1
        ;;
esac

# 2. Fetch latest release version from GitHub API
echo "[*] Resolving latest release for $REPO..."
TAG=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$TAG" ]; then
    echo "[ERROR] Failed to fetch latest release tag from https://api.github.com/repos/$REPO/releases/latest" >&2
    exit 1
fi

ARCHIVE_NAME="cxtvault-${TAG}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${TAG}/${ARCHIVE_NAME}"

echo "[*] Downloading $DOWNLOAD_URL..."
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ARCHIVE_NAME"

echo "[*] Extracting binary..."
tar -xzf "$TMP_DIR/$ARCHIVE_NAME" -C "$TMP_DIR"

mkdir -p "$INSTALL_DIR"
cp "$TMP_DIR/cxtvault-${TAG}-${TARGET}/cxtvault" "$INSTALL_DIR/cxtvault"
chmod +x "$INSTALL_DIR/cxtvault"

# Optional symlink for short shorthand alias `cxtv`
ln -sf "$INSTALL_DIR/cxtvault" "$INSTALL_DIR/cxtv" 2>/dev/null || true

echo ""
echo "[+] Successfully installed 'cxtvault' to $INSTALL_DIR/cxtvault"
echo ""

# 3. Path hint
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "[NOTE] $INSTALL_DIR is not currently in your PATH."
        echo "   Add it to your shell config (~/.bashrc or ~/.zshrc):"
        echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        ;;
esac

echo "[>] Quick check: run 'cxtvault --version' to get started."
