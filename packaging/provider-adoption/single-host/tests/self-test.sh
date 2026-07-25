#!/usr/bin/env bash
set -euo pipefail

test_dir="$(
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P
)"
readonly test_dir
package_root="$(
  cd -- "$test_dir/.." && pwd -P
)"
readonly package_root

readonly -a shell_files=(
  "$package_root/bin/forge-directory-offhost-uploader"
  "$package_root/bin/forge-production-alert"
  "$test_dir/directory-offhost-self-test.sh"
  "$test_dir/directory-stubs/findmnt"
  "$test_dir/alert-self-test.sh"
  "$test_dir/stubs/curl"
  "$test_dir/stubs/df"
  "$test_dir/stubs/forge-admin"
  "$test_dir/stubs/stat"
  "$test_dir/stubs/systemctl"
)

cloud_specific_path="$(
  /usr/bin/find "$package_root" \
    \( -iname '*aws*' -o -iname '*s3*' \) \
    -print \
    -quit
)"
[[ -z "$cloud_specific_path" ]] || {
  printf 'cloud-specific component leaked into Forge Core bundle: %s\n' \
    "$cloud_specific_path" >&2
  exit 1
}

for shell_file in "${shell_files[@]}"; do
  [[ -x "$shell_file" ]] || {
    printf 'non-executable shell component: %s\n' "$shell_file" >&2
    exit 1
  }
  /usr/bin/bash -n "$shell_file"
  if /usr/bin/grep -Eq '(^|[[:space:]])rtk([[:space:]]|$)' "$shell_file"; then
    printf 'distributed component depends on development-only rtk: %s\n' \
      "$shell_file" >&2
    exit 1
  fi
done

/usr/bin/grep -Fxq \
  'LoadCredential=forge-telegram-alert:/etc/forge/credentials/forge-telegram-alert' \
  "$package_root/systemd/forge-production-alert.service"

/usr/bin/bash "$test_dir/directory-offhost-self-test.sh"
/usr/bin/bash "$test_dir/alert-self-test.sh"

printf 'single-host provider-adoption self-test: PASS\n'
