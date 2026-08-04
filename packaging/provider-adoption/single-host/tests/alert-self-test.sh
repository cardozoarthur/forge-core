#!/usr/bin/env bash
set -euo pipefail

umask 077

test_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly test_dir
package_root="$(cd -- "${test_dir}/.." && pwd)"
readonly package_root
readonly operator="${package_root}/bin/forge-production-alert"
readonly stubs="${test_dir}/stubs"
test_root="$(mktemp -d /tmp/forge-production-alert-selftest.XXXXXX)"
readonly test_root
readonly credentials_dir="${test_root}/credentials"
readonly backup_dir="${test_root}/backups"
readonly store_path="${test_root}/forge.sqlite"
readonly curl_capture="${test_root}/curl"
readonly output_log="${test_root}/operator.log"
readonly fake_token='123456:FAKE_TOKEN_FOR_OFFLINE_SELF_TEST_123456'
readonly fake_chat_id='-1001234567890'
readonly now_epoch='200000'

cleanup() {
  if [[ "$test_root" == /tmp/forge-production-alert-selftest.* && -d "$test_root" ]]; then
    rm -rf -- "$test_root"
  fi
}
trap cleanup EXIT

fail() {
  printf 'self-test: %s\n' "$1" >&2
  exit 1
}

curl_count() {
  local count_file="${curl_capture}/count"
  if [[ -f "$count_file" ]]; then
    /usr/bin/sed -n '1p' "$count_file"
  else
    printf '0\n'
  fi
}

run_operator() {
  local state_dir="$1"
  local failure_category="$2"
  shift 2

  CREDENTIALS_DIRECTORY="$credentials_dir" \
    FORGE_ALERT_OFFLINE_SELF_TEST=1 \
    FORGE_ALERT_STATE_DIR="$state_dir" \
    FORGE_STORE_PATH="$store_path" \
    FORGE_BACKUP_DIR="$backup_dir" \
    FORGE_NOW_EPOCH_OVERRIDE="$now_epoch" \
    FORGE_ALERT_HOST_OVERRIDE='forge-selftest' \
    SYSTEMCTL_BIN="${stubs}/systemctl" \
    FORGE_ADMIN_BIN="${stubs}/forge-admin" \
    CURL_BIN="${stubs}/curl" \
    STAT_BIN="${stubs}/stat" \
    DF_BIN="${stubs}/df" \
    DATE_BIN='/usr/bin/date' \
    HOSTNAME_BIN='/usr/bin/hostname' \
    FLOCK_BIN='/usr/bin/flock' \
    TEST_FAILURE_CATEGORY="$failure_category" \
    TEST_NOW_EPOCH="$now_epoch" \
    TEST_CURL_CAPTURE_DIR="$curl_capture" \
    "$operator" "$@" >>"$output_log" 2>&1
}

expect_failure() {
  local state_dir="$1"
  local category="$2"
  if run_operator "$state_dir" "$category"; then
    fail "category ${category} unexpectedly passed"
  fi
}

mkdir -p -- "$credentials_dir" "$backup_dir" "$curl_capture"
chmod 0700 -- "$credentials_dir" "$backup_dir" "$curl_capture"
printf '%s\n%s\n' "$fake_token" "$fake_chat_id" \
  >"${credentials_dir}/forge-telegram-alert"
chmod 0600 -- "${credentials_dir}/forge-telegram-alert"
printf 'offline-store-fixture\n' >"$store_path"
printf '%s %s %s %s %s\n' \
  'forge-offhost-v2' \
  'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' \
  'selftest-generation' \
  'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc' \
  >"${backup_dir}/forge-20260725T000000Z.sqlite.offhost-verified"

for executable in "$operator" "${stubs}"/*; do
  [[ -x "$executable" ]] || fail "non-executable test component: $executable"
  /usr/bin/bash -n "$executable"
done

for category in \
  forge_ops \
  forge_runtime \
  request_supervisor \
  store_check \
  backup_timer \
  backup_service \
  disk_space \
  offhost_marker; do
  run_operator "${test_root}/test-alert-state" "" --test-alert "$category"
done
[[ "$(curl_count)" == "8" ]] ||
  fail 'eight offline test alerts were not captured'

for category in \
  forge_ops \
  forge_runtime \
  request_supervisor \
  store_check \
  backup_timer \
  backup_service \
  disk_space \
  offhost_marker; do
  state_dir="${test_root}/state-${category}"
  before="$(curl_count)"

  run_operator "$state_dir" ""
  [[ "$(curl_count)" == "$before" ]] ||
    fail "healthy initialization sent an alert for ${category}"

  expect_failure "$state_dir" "$category"
  after_failure="$(curl_count)"
  [[ "$after_failure" == "$((before + 1))" ]] ||
    fail "first ${category} failure did not send exactly one alert"

  expect_failure "$state_dir" "$category"
  [[ "$(curl_count)" == "$after_failure" ]] ||
    fail "repeated ${category} failure was not deduplicated"

  run_operator "$state_dir" ""
  [[ "$(curl_count)" == "$((after_failure + 1))" ]] ||
    fail "${category} recovery did not send exactly one notification"
done

if /usr/bin/grep -Fq -- "$fake_token" "${curl_capture}/argv" ||
  /usr/bin/grep -Fq -- "$fake_chat_id" "${curl_capture}/argv"; then
  fail 'curl argv exposed a credential'
fi
if /usr/bin/grep -Fq -- "$fake_token" "$output_log" ||
  /usr/bin/grep -Fq -- "$fake_chat_id" "$output_log"; then
  fail 'operator output exposed a credential'
fi

for config in "${curl_capture}"/config-*; do
  /usr/bin/grep -Fq -- "bot${fake_token}/sendMessage" "$config" ||
    fail 'curl stub did not receive the token through stdin config'
  /usr/bin/grep -Fq -- "chat_id=${fake_chat_id}" "$config" ||
    fail 'curl stub did not receive the chat id through stdin config'
done

printf 'self-test: PASS (eight categories, transition deduplication, recovery, secret-free argv/output)\n'
