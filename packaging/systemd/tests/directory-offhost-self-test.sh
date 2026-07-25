#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'forge systemd directory off-host self-test: %s\n' "$*" >&2
  exit 1
}

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
systemd_dir="$(cd -- "$tests_dir/.." && pwd)"
installer="$systemd_dir/install-service.sh"
findmnt_stub="$tests_dir/stubs/findmnt"

[[ -f "$installer" ]] || fail "missing install-service.sh"
[[ -x "$findmnt_stub" ]] || fail "missing executable findmnt stub"
export PATH="$tests_dir/stubs:$PATH"

# Sourcing exposes only the deterministic validation and drop-in helpers. It
# does not execute the root check or contact systemd.
# shellcheck disable=SC1090,SC1091 # Runtime-resolved project-local helper.
source "$installer"

test_account="$(id -un)"
if [[ "$EUID" -eq 0 ]]; then
  test_parent="/run"
else
  test_parent="$(getent passwd "$(id -u)" | cut -d: -f6)"
fi
[[ -n "$test_parent" && -d "$test_parent" && ! -L "$test_parent" && -w "$test_parent" ]] ||
  fail "secure test parent is unavailable"
test_root="$(mktemp -d "$test_parent/.forge-systemd-directory-test.XXXXXX")"
cleanup() {
  case "${test_root:-}" in
    "$test_parent"/.forge-systemd-directory-test.*)
      chmod -R u+rwX "$test_root" >/dev/null 2>&1 || true
      [[ -d "$test_root" && ! -L "$test_root" ]] && rm -rf -- "$test_root"
      ;;
  esac
}
trap cleanup EXIT

destination="$test_root/provider"
install -d -m 0700 "$destination"
export TEST_DIRECTORY_MOUNT_TARGET="$test_root"
export TEST_DIRECTORY_MOUNT_SOURCE="fixture-remote:/forge"
export TEST_DIRECTORY_MOUNT_FSTYPE="fuse.fixture"
export TEST_DIRECTORY_MOUNT_STATE="up"

resolved_path="$(
  resolve_directory_offhost_path "file://$destination" "$test_account"
)"
[[ "$resolved_path" = "$destination" ]] ||
  fail "canonical file destination did not resolve exactly"

mount_fsid="$(stat -f -c '%i' -- "$destination")"
mount_identity="$(
  capture_directory_mount_identity "$destination"
)"
expected_mount_identity="$(
  printf '%s\n' \
    "forge-directory-mount-v1" \
    "target=$TEST_DIRECTORY_MOUNT_TARGET" \
    "source=$TEST_DIRECTORY_MOUNT_SOURCE" \
    "fstype=$TEST_DIRECTORY_MOUNT_FSTYPE" \
    "fsid=$mount_fsid"
)"
[[ "$mount_identity" = "$expected_mount_identity" ]] ||
  fail "captured mount identity is not exact"

export TEST_DIRECTORY_MOUNT_TARGET="$destination"
if (
  capture_directory_mount_identity "$destination" >/dev/null 2>&1
); then
  fail "destination equal to the mount target was accepted"
fi
export TEST_DIRECTORY_MOUNT_TARGET="$test_root"

export TEST_DIRECTORY_MOUNT_STATE="down"
if (
  capture_directory_mount_identity "$destination" >/dev/null 2>&1
); then
  fail "host root fallback was accepted after simulated mount loss"
fi
export TEST_DIRECTORY_MOUNT_STATE="up"

non_directory_path="$(
  resolve_directory_offhost_path "s3://example-forge/backups"
)"
[[ -z "$non_directory_path" ]] ||
  fail "non-file provider must not receive a filesystem write grant"

expect_destination_rejected() {
  local candidate="$1"
  local label="$2"
  local account="${3:-$test_account}"

  if (resolve_directory_offhost_path "$candidate" "$account" >/dev/null 2>&1); then
    fail "$label was accepted"
  fi
}

expect_destination_rejected \
  "file://example.test$destination" \
  "file URI with an authority"
expect_destination_rejected \
  "file://$test_root/missing" \
  "missing directory"
expect_destination_rejected \
  "file://$destination/../provider" \
  "path traversal"
expect_destination_rejected \
  "file://$destination/" \
  "trailing slash"

ln -s -- "$destination" "$test_root/provider-link"
expect_destination_rejected \
  "file://$test_root/provider-link" \
  "symbolic-link destination"

install -d -m 0700 "$test_root/provider with space"
expect_destination_rejected \
  "file://$test_root/provider with space" \
  "non-encodable systemd path"
if locale -a | grep -Fxq "pt_PT.utf8"; then
  unicode_destination="$test_root/provider-café"
  install -d -m 0700 "$unicode_destination"
  if LC_ALL=pt_PT.utf8 bash -c \
    'source "$1"; resolve_directory_offhost_path "$2"' \
    forge-directory-locale-test \
    "$installer" \
    "file://$unicode_destination" \
    >/dev/null 2>&1; then
    fail "locale-expanded non-ASCII directory destination was accepted"
  fi
fi

chmod 0750 "$destination"
expect_destination_rejected \
  "file://$destination" \
  "destination mode other than 0700"
chmod 0700 "$destination"

install -d -m 0770 "$test_root/unsafe-ancestor"
install -d -m 0700 "$test_root/unsafe-ancestor/provider"
expect_destination_rejected \
  "file://$test_root/unsafe-ancestor/provider" \
  "group-writable ancestor"
chmod 0700 "$test_root/unsafe-ancestor"

if id -u nobody >/dev/null 2>&1 &&
  [[ "$(id -u nobody)" -ne "$(id -u "$test_account")" ]]; then
  expect_destination_rejected \
    "file://$destination" \
    "destination owned by another account" \
    nobody
fi

probe_account="$(id -un)"
if [[ "$EUID" -eq 0 ]]; then
  id -u nobody >/dev/null 2>&1 ||
    fail "root execution requires the unprivileged nobody account"
  probe_account="nobody"
  chmod 0711 "$test_root"
  chown "$(id -u "$probe_account"):$(id -g "$probe_account")" "$destination"
fi

resolved_path="$(
  resolve_directory_offhost_path "file://$destination" "$probe_account"
)"
[[ "$resolved_path" = "$destination" ]] ||
  fail "service-owned private destination did not resolve exactly"

probe_directory_write_contract "$probe_account" "$destination"
if find "$destination" -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
  fail "write-contract probe left an artifact behind"
fi

chmod 0500 "$destination"
if (
  probe_directory_write_contract "$probe_account" "$destination" \
    >/dev/null 2>&1
); then
  fail "read-only directory passed the service-user write contract"
fi
chmod 0700 "$destination"

dropin_dir="$test_root/forge-backup.service.d"
dropin="$dropin_dir/20-directory-offhost.conf"
install -d -m 0700 "$dropin_dir"

reconcile_backup_directory_dropin "$dropin" "$destination"
expected_dropin="$(
  printf \
    '[Unit]\nRequiresMountsFor=%s\n\n[Service]\nReadWritePaths=%s' \
    "$destination" \
    "$destination"
)"
actual_dropin="$(tr -d '\r' <"$dropin")"
[[ "$actual_dropin" = "$expected_dropin" ]] ||
  fail "generated ReadWritePaths drop-in is not exact"
[[ "$(stat -c '%a' -- "$dropin")" = 644 ]] ||
  fail "generated drop-in mode is not 0644"

cp -- "$dropin" "$test_root/previous-dropin"
second_destination="$test_root/provider-two"
install -d -m 0700 "$second_destination"
reconcile_backup_directory_dropin "$dropin" "$second_destination"
grep -Fxq "ReadWritePaths=$second_destination" "$dropin" ||
  fail "directory provider replacement did not update the write grant"
grep -Fxq "RequiresMountsFor=$second_destination" "$dropin" ||
  fail "directory provider replacement did not update the mount dependency"

rm -f -- "$dropin"
cp -- "$test_root/previous-dropin" "$dropin"
grep -Fxq "ReadWritePaths=$destination" "$dropin" ||
  fail "saved directory provider state could not be restored"

reconcile_backup_directory_dropin "$dropin" ""
[[ ! -e "$dropin" && ! -L "$dropin" ]] ||
  fail "switching to a non-file provider did not remove the write grant"

# shellcheck disable=SC2016 # Match the literal managed-path expression.
grep -Fq '"$directory_offhost_dropin"' "$installer" ||
  fail "directory drop-in is not part of the transactional managed path set"
grep -Fq '/etc/forge/backup-offhost-mount-identity' "$installer" ||
  fail "mount identity is not part of the transactional managed path set"

apply_line="$(
  grep -n '^reconcile_backup_directory_dropin \\$' "$installer" |
    tail -n 1 |
    cut -d: -f1
)"
daemon_reload_line="$(
  awk -v start="$apply_line" \
    'NR > start && $0 == "systemctl daemon-reload" { print NR; exit }' \
    "$installer"
)"
initial_backup_line="$(
  awk -v start="$apply_line" \
    'NR > start && $0 == "if ! systemctl start forge-backup.service; then" { print NR; exit }' \
    "$installer"
)"
[[ -n "$apply_line" &&
  -n "$daemon_reload_line" &&
  -n "$initial_backup_line" &&
  "$apply_line" -lt "$daemon_reload_line" &&
  "$daemon_reload_line" -lt "$initial_backup_line" ]] ||
  fail "write grant is not applied and reloaded before the first backup"

bash -n "$installer"
bash -n "$tests_dir/directory-offhost-self-test.sh"

printf 'forge systemd directory off-host self-test: PASS\n'
