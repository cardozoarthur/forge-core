#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_DIR="$(mktemp -d)"
RELEASE_DIR="$TEST_DIR/release"
STUB_DIR="$TEST_DIR/stubs"
INSTALL_DIR="$TEST_DIR/install"
DOWNLOAD_LOG="$TEST_DIR/download.log"
VERSION="v0.6.0"
REPO="cardozoarthur/foundry-core"
ISSUER="https://token.actions.githubusercontent.com"
IDENTITY="https://github.com/$REPO/.github/workflows/release.yml@refs/tags/$VERSION"

cleanup() {
  rm -rf -- "$TEST_DIR"
}
trap cleanup EXIT

mkdir -p "$RELEASE_DIR" "$STUB_DIR" "$INSTALL_DIR"
install -m 0755 "$ROOT_DIR/installer/tests/stubs/cosign" "$STUB_DIR/cosign"
install -m 0755 "$ROOT_DIR/installer/tests/stubs/curl" "$STUB_DIR/curl"

case "$(uname -m)" in
  x86_64|amd64) target_arch="x86_64" ;;
  aarch64|arm64) target_arch="aarch64" ;;
  *)
    echo "installer self-test: unsupported test architecture" >&2
    exit 1
    ;;
esac
asset="foundry-linux-${target_arch}.tar.gz"

fixture_dir="$TEST_DIR/archive"
mkdir -p "$fixture_dir"
printf '#!/usr/bin/env bash\nprintf "foundry 0.6.0\\n"\n' >"$fixture_dir/foundry"
chmod 0755 "$fixture_dir/foundry"
# foundry-brand-allow: legacy-compat
cat >"$fixture_dir/forge" <<'LEGACY_SHIM'
#!/usr/bin/env bash
# foundry-brand-allow: legacy-compat
printf 'warning: `forge` is deprecated; use `foundry`\n' >&2
exec "$(dirname -- "$0")/foundry" "$@"
LEGACY_SHIM
# foundry-brand-allow: legacy-compat
chmod 0755 "$fixture_dir/forge"
# foundry-brand-allow: legacy-compat
tar -czf "$RELEASE_DIR/$asset" -C "$fixture_dir" foundry forge
asset_sha256="$(sha256sum "$RELEASE_DIR/$asset" | awk '{ print $1 }')"
printf '%s %s\n' "$asset_sha256" "$asset" >"$RELEASE_DIR/SHA256SUMS"

write_bundle() {
  local issuer="${1:-$ISSUER}"
  local identity="${2:-$IDENTITY}"
  local subject_sha256="${3:-}"
  if [[ -z "$subject_sha256" ]]; then
    subject_sha256="$(sha256sum "$RELEASE_DIR/SHA256SUMS" | awk '{ print $1 }')"
  fi
  {
    printf 'issuer=%s\n' "$issuer"
    printf 'identity=%s\n' "$identity"
    printf 'subject_sha256=%s\n' "$subject_sha256"
  } >"$RELEASE_DIR/SHA256SUMS.sigstore.json"
}

run_installer() {
  PATH="$STUB_DIR:$PATH" \
    FOUNDRY_TEST_RELEASE_DIR="$RELEASE_DIR" \
    FOUNDRY_TEST_DOWNLOAD_LOG="$DOWNLOAD_LOG" \
    FOUNDRY_REPO="$REPO" \
    FOUNDRY_VERSION="$VERSION" \
    FOUNDRY_RELEASE_BASE_URL="${1:-https://release.invalid/$VERSION}" \
    FOUNDRY_BIN_DIR="$INSTALL_DIR" \
    FOUNDRY_INSTALLER_TEST_MODE="${2:-0}" \
    bash "$ROOT_DIR/installer/install.sh"
}

run_legacy_installer() {
  local -a legacy_env=(
    # foundry-brand-allow: legacy-compat
    "FORGE_REPO=$REPO"
    # foundry-brand-allow: legacy-compat
    "FORGE_VERSION=$VERSION"
    # foundry-brand-allow: legacy-compat
    "FORGE_RELEASE_BASE_URL=https://release.invalid/$VERSION"
    # foundry-brand-allow: legacy-compat
    "FORGE_BIN_DIR=$INSTALL_DIR"
  )

  env \
    -u FOUNDRY_REPO \
    -u FOUNDRY_VERSION \
    -u FOUNDRY_RELEASE_BASE_URL \
    -u FOUNDRY_BIN_DIR \
    PATH="$STUB_DIR:$PATH" \
    FOUNDRY_TEST_RELEASE_DIR="$RELEASE_DIR" \
    FOUNDRY_TEST_DOWNLOAD_LOG="$DOWNLOAD_LOG" \
    "${legacy_env[@]}" \
    bash "$ROOT_DIR/installer/install.sh"
}

expect_failure_before_archive() {
  local label="$1"
  local base_url="${2:-https://release.invalid/$VERSION}"
  local test_mode="${3:-0}"
  : >"$DOWNLOAD_LOG"
  rm -f -- "$INSTALL_DIR/foundry"
  # foundry-brand-allow: legacy-compat
  rm -f -- "$INSTALL_DIR/forge"
  if run_installer "$base_url" "$test_mode" >/dev/null 2>&1; then
    echo "installer self-test: $label unexpectedly succeeded" >&2
    exit 1
  fi
  [[ ! -e "$INSTALL_DIR/foundry" ]] || {
    echo "installer self-test: $label installed a binary" >&2
    exit 1
  }
  # foundry-brand-allow: legacy-compat
  [[ ! -e "$INSTALL_DIR/forge" ]] || {
    # foundry-brand-allow: legacy-compat
    echo "installer self-test: $label installed the forge compatibility shim" >&2
    exit 1
  }
  if grep -Fx "$asset" "$DOWNLOAD_LOG" >/dev/null; then
    echo "installer self-test: $label downloaded an archive before trust" >&2
    exit 1
  fi
}

write_bundle
: >"$DOWNLOAD_LOG"
run_installer >/dev/null
[[ -x "$INSTALL_DIR/foundry" ]]
# foundry-brand-allow: legacy-compat
[[ -x "$INSTALL_DIR/forge" ]]
# foundry-brand-allow: legacy-compat
shim_stderr="$TEST_DIR/forge-shim.stderr"
# foundry-brand-allow: legacy-compat
shim_stdout="$("$INSTALL_DIR/forge" --version 2>"$shim_stderr")"
[[ "$shim_stdout" = "foundry 0.6.0" ]]
# foundry-brand-allow: legacy-compat
grep -Fq 'warning: `forge` is deprecated; use `foundry`' "$shim_stderr"

rm -f -- "$INSTALL_DIR/foundry"
# foundry-brand-allow: legacy-compat
rm -f -- "$INSTALL_DIR/forge"
run_legacy_installer >/dev/null
[[ -x "$INSTALL_DIR/foundry" ]]
# foundry-brand-allow: legacy-compat
[[ -x "$INSTALL_DIR/forge" ]]

rm -f -- "$RELEASE_DIR/SHA256SUMS.sigstore.json"
expect_failure_before_archive "missing Sigstore bundle"

write_bundle "$ISSUER" "$IDENTITY" "$(printf '0%.0s' {1..64})"
expect_failure_before_archive "adulterated Sigstore bundle"

write_bundle "https://issuer.invalid" "$IDENTITY"
expect_failure_before_archive "wrong Sigstore issuer"

write_bundle "$ISSUER" \
  "https://github.com/$REPO/.github/workflows/other.yml@refs/tags/$VERSION"
expect_failure_before_archive "wrong Sigstore workflow identity"

write_bundle "$ISSUER" \
  "https://github.com/$REPO/.github/workflows/release.yml@refs/tags/v0.6.1"
expect_failure_before_archive "wrong Sigstore release tag"

write_bundle
expect_failure_before_archive "plain HTTP outside test mode" \
  "http://127.0.0.1:1/$VERSION"

: >"$DOWNLOAD_LOG"
rm -f -- "$INSTALL_DIR/foundry"
run_installer "http://release.invalid/$VERSION" 1 >/dev/null
[[ -x "$INSTALL_DIR/foundry" ]]

echo "installer supply-chain self-test: PASS"
