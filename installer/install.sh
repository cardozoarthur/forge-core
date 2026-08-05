#!/usr/bin/env bash
set -euo pipefail

# foundry-brand-allow: legacy-compat
REPO="${FOUNDRY_REPO:-${FORGE_REPO:-cardozoarthur/foundry-core}}"
# foundry-brand-allow: legacy-compat
VERSION="${FOUNDRY_VERSION:-${FORGE_VERSION:-latest}}"
# foundry-brand-allow: legacy-compat
PREFIX="${FOUNDRY_PREFIX:-${FORGE_PREFIX:-$HOME/.local}}"
# foundry-brand-allow: legacy-compat
BIN_DIR="${FOUNDRY_BIN_DIR:-${FORGE_BIN_DIR:-$PREFIX/bin}}"
# foundry-brand-allow: legacy-compat
RELEASE_BASE_URL="${FOUNDRY_RELEASE_BASE_URL:-${FORGE_RELEASE_BASE_URL:-}}"
# foundry-brand-allow: legacy-compat
TEST_MODE="${FOUNDRY_INSTALLER_TEST_MODE:-${FORGE_INSTALLER_TEST_MODE:-0}}"
SIGSTORE_ISSUER="https://token.actions.githubusercontent.com"
TMP_DIR="$(mktemp -d)"
STAGED_BINARY=""
# foundry-brand-allow: legacy-compat
STAGED_FORGE_SHIM=""

cleanup() {
  rm -rf -- "$TMP_DIR"
  if [[ -n "$STAGED_BINARY" && -e "$STAGED_BINARY" ]]; then
    rm -f -- "$STAGED_BINARY"
  fi
  # foundry-brand-allow: legacy-compat
  if [[ -n "$STAGED_FORGE_SHIM" && -e "$STAGED_FORGE_SHIM" ]]; then
    # foundry-brand-allow: legacy-compat
    rm -f -- "$STAGED_FORGE_SHIM"
  fi
}
trap cleanup EXIT

fail() {
  echo "foundry installer: $*" >&2
  exit 1
}

for required_command in awk cosign curl install mktemp tar tr; do
  command -v "$required_command" >/dev/null 2>&1 ||
    fail "required command not found: $required_command"
done

[[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] ||
  fail "FOUNDRY_REPO must be an exact GitHub owner/repository pair"

release_version_is_valid() {
  [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]
}

if [[ -n "$RELEASE_BASE_URL" ]]; then
  [[ "$VERSION" != "latest" ]] ||
    fail "FOUNDRY_VERSION must be explicit when FOUNDRY_RELEASE_BASE_URL is set"
  resolved_version="$VERSION"
  release_version_is_valid "$resolved_version" ||
    fail "FOUNDRY_VERSION must be a supported v-prefixed semantic version"
  base_url="${RELEASE_BASE_URL%/}"
else
  if [[ "$VERSION" == "latest" ]]; then
    latest_url="$(
      curl -fsSL --proto '=https' --tlsv1.2 \
        -o /dev/null -w '%{url_effective}' \
        "https://github.com/${REPO}/releases/latest"
    )" || fail "could not resolve the latest immutable release tag"
    resolved_version="${latest_url##*/}"
  else
    resolved_version="$VERSION"
  fi
  release_version_is_valid "$resolved_version" ||
    fail "resolved release version is not a supported v-prefixed semantic version"
  base_url="https://github.com/${REPO}/releases/download/${resolved_version}"
fi

case "$base_url" in
  https://*)
    download() {
      curl -fsSL --proto '=https' --tlsv1.2 "$1" -o "$2"
    }
    ;;
  http://*)
    [[ "$TEST_MODE" == "1" ]] ||
      fail "plain HTTP release URLs are allowed only with FOUNDRY_INSTALLER_TEST_MODE=1"
    download() {
      curl -fsSL --proto '=http,https' "$1" -o "$2"
    }
    ;;
  *)
    fail "release URL must use HTTPS"
    ;;
esac

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

asset="foundry-${platform}-${target_arch}.tar.gz"
checksums="$TMP_DIR/SHA256SUMS"
sigstore_bundle="$TMP_DIR/SHA256SUMS.sigstore.json"

download "$base_url/SHA256SUMS" "$checksums"
download "$base_url/SHA256SUMS.sigstore.json" "$sigstore_bundle"

sigstore_identity="https://github.com/${REPO}/.github/workflows/release.yml@refs/tags/${resolved_version}"
if ! cosign verify-blob \
  --bundle "$sigstore_bundle" \
  --certificate-identity "$sigstore_identity" \
  --certificate-oidc-issuer "$SIGSTORE_ISSUER" \
  "$checksums" >/dev/null; then
  fail "Sigstore verification failed for SHA256SUMS; no archive was trusted"
fi

expected_sha256="$(
  awk -v expected_asset="$asset" '$2 == expected_asset { print $1 }' "$checksums"
)"
if [[ ${#expected_sha256} -ne 64 || "$expected_sha256" == *[!0-9A-Fa-f]* ]]; then
  fail "verified SHA256SUMS does not contain one valid digest for $asset"
fi
expected_sha256="$(printf '%s' "$expected_sha256" | tr '[:upper:]' '[:lower:]')"

archive="$TMP_DIR/$asset"
download "$base_url/$asset" "$archive"

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
# foundry-brand-allow: legacy-compat
forge_shim_member=""
while IFS= read -r member; do
  case "$member" in
    foundry|./foundry) binary_member="$member" ;;
    # foundry-brand-allow: legacy-compat
    forge|./forge) forge_shim_member="$member" ;;
  esac
done < <(tar -tzf "$archive")
[[ -n "$binary_member" ]] || fail "verified archive does not contain foundry"
# foundry-brand-allow: legacy-compat
[[ -n "$forge_shim_member" ]] || fail "verified archive does not contain the temporary forge compatibility shim"

# foundry-brand-allow: legacy-compat
tar -xzf "$archive" -C "$extract_dir" "$binary_member" "$forge_shim_member"
binary="$extract_dir/foundry"
[[ -f "$binary" ]] ||
  fail "foundry binary was not extracted from the verified archive"
# foundry-brand-allow: legacy-compat
forge_shim="$extract_dir/forge"
# foundry-brand-allow: legacy-compat
[[ -f "$forge_shim" ]] ||
  # foundry-brand-allow: legacy-compat
  fail "forge compatibility shim was not extracted from the verified archive"

mkdir -p "$BIN_DIR"
STAGED_BINARY="$(mktemp "$BIN_DIR/.foundry.install.XXXXXX")"
install -m 0755 "$binary" "$STAGED_BINARY"
# foundry-brand-allow: legacy-compat
STAGED_FORGE_SHIM="$(mktemp "$BIN_DIR/.forge-compat.install.XXXXXX")"
# foundry-brand-allow: legacy-compat
install -m 0755 "$forge_shim" "$STAGED_FORGE_SHIM"
mv -f -- "$STAGED_BINARY" "$BIN_DIR/foundry"
STAGED_BINARY=""
# foundry-brand-allow: legacy-compat
mv -f -- "$STAGED_FORGE_SHIM" "$BIN_DIR/forge"
# foundry-brand-allow: legacy-compat
STAGED_FORGE_SHIM=""
echo "Installed foundry to $BIN_DIR/foundry"
# foundry-brand-allow: legacy-compat
echo "Installed temporary forge compatibility shim to $BIN_DIR/forge" >&2
