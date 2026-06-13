#!/usr/bin/env bash
set -euo pipefail

REPO="${FORGE_REPO:-cardozoarthur/forge-core}"
VERSION="${FORGE_VERSION:-latest}"
PREFIX="${FORGE_PREFIX:-$HOME/.local}"
BIN_DIR="${FORGE_BIN_DIR:-$PREFIX/bin}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"

case "$os" in
  linux) platform="linux" ;;
  darwin) platform="macos" ;;
  *) echo "Unsupported platform: $os" >&2; exit 1 ;;
esac

case "$arch" in
  x86_64|amd64) target_arch="x86_64" ;;
  aarch64|arm64) target_arch="aarch64" ;;
  *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
esac

mkdir -p "$BIN_DIR"

if [[ "$VERSION" == "latest" ]]; then
  release_url="https://github.com/${REPO}/releases/latest/download/forge-${platform}-${target_arch}.tar.gz"
else
  release_url="https://github.com/${REPO}/releases/download/${VERSION}/forge-${platform}-${target_arch}.tar.gz"
fi

archive="$TMP_DIR/forge.tar.gz"
curl -fsSL "$release_url" -o "$archive"
tar -xzf "$archive" -C "$TMP_DIR"

if [[ ! -f "$TMP_DIR/forge" ]]; then
  echo "forge binary not found in archive: $release_url" >&2
  exit 1
fi

install -m 0755 "$TMP_DIR/forge" "$BIN_DIR/forge"
echo "Installed forge to $BIN_DIR/forge"
