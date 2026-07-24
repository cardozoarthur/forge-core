#!/usr/bin/env bash
set -euo pipefail

REPO="${FORGE_REPO:-cardozoarthur/forge-core}"
VERSION="${FORGE_VERSION:-latest}"
PREFIX="${FORGE_PREFIX:-$HOME/.local}"
BIN_DIR="${FORGE_BIN_DIR:-$PREFIX/bin}"
RELEASE_BASE_URL="${FORGE_RELEASE_BASE_URL:-}"
TMP_DIR="$(mktemp -d)"
STAGED_BINARY=""

cleanup() {
  rm -rf "$TMP_DIR"
  if [[ -n "$STAGED_BINARY" && -e "$STAGED_BINARY" ]]; then
    rm -f "$STAGED_BINARY"
  fi
}
trap cleanup EXIT

fail() {
  echo "forge installer: $*" >&2
  exit 1
}

for required_command in awk curl install mktemp tar tr; do
  command -v "$required_command" >/dev/null 2>&1 ||
    fail "required command not found: $required_command"
done

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"

case "$os" in
  linux) platform="linux" ;;
  darwin) platform="macos" ;;
  *) fail "unsupported platform: $os" ;;
esac

case "$arch" in
  x86_64|amd64) target_arch="x86_64" ;;
  aarch64|arm64) target_arch="aarch64" ;;
  *) fail "unsupported architecture: $arch" ;;
esac

asset="forge-${platform}-${target_arch}.tar.gz"

if [[ -n "$RELEASE_BASE_URL" ]]; then
  base_url="${RELEASE_BASE_URL%/}"
  download() {
    curl -fsSL "$1" -o "$2"
  }
elif [[ "$VERSION" == "latest" ]]; then
  base_url="https://github.com/${REPO}/releases/latest/download"
  download() {
    curl -fsSL --proto '=https' --tlsv1.2 "$1" -o "$2"
  }
else
  base_url="https://github.com/${REPO}/releases/download/${VERSION}"
  download() {
    curl -fsSL --proto '=https' --tlsv1.2 "$1" -o "$2"
  }
fi

archive="$TMP_DIR/$asset"
checksums="$TMP_DIR/SHA256SUMS"
download "$base_url/$asset" "$archive"
download "$base_url/SHA256SUMS" "$checksums"

expected_sha256="$(
  awk -v expected_asset="$asset" '$2 == expected_asset { print $1 }' "$checksums"
)"
if [[ ${#expected_sha256} -ne 64 || "$expected_sha256" == *[!0-9A-Fa-f]* ]]; then
  fail "SHA256SUMS does not contain one valid digest for $asset"
fi
expected_sha256="$(printf '%s' "$expected_sha256" | tr '[:upper:]' '[:lower:]')"

if command -v sha256sum >/dev/null 2>&1; then
  actual_sha256="$(sha256sum "$archive" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual_sha256="$(shasum -a 256 "$archive" | awk '{ print $1 }')"
else
  fail "sha256sum or shasum is required to verify the release"
fi

actual_sha256="$(printf '%s' "$actual_sha256" | tr '[:upper:]' '[:lower:]')"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  fail "checksum mismatch for $asset; no files were installed"
fi

extract_dir="$TMP_DIR/extract"
mkdir -p "$extract_dir"
binary_member=""
while IFS= read -r member; do
  case "$member" in
    forge|./forge) binary_member="$member" ;;
  esac
done < <(tar -tzf "$archive")
[[ -n "$binary_member" ]] || fail "verified archive does not contain forge"

tar -xzf "$archive" -C "$extract_dir" "$binary_member"
binary="$extract_dir/forge"
[[ -f "$binary" ]] || fail "forge binary was not extracted from verified archive"

mkdir -p "$BIN_DIR"
STAGED_BINARY="$(mktemp "$BIN_DIR/.forge.install.XXXXXX")"
install -m 0755 "$binary" "$STAGED_BINARY"
mv -f "$STAGED_BINARY" "$BIN_DIR/forge"
STAGED_BINARY=""
echo "Installed forge to $BIN_DIR/forge"
