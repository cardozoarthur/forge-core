#!/usr/bin/env bash
set -euo pipefail

umask 077

fail() {
  echo "off-host backup smoke: $*" >&2
  exit 1
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly script_dir
readonly backup_script="$script_dir/systemd/forge-backup"
readonly restore_drill_script="$script_dir/systemd/forge-restore-drill"

[[ $# -eq 1 ]] || fail "usage: $0 /path/to/real/forge"
[[ -f "$backup_script" && -x "$backup_script" ]] ||
  fail "backup script is not executable: $backup_script"
[[ -f "$restore_drill_script" && -x "$restore_drill_script" ]] ||
  fail "restore drill is not executable: $restore_drill_script"

forge_source="$(realpath -e -- "$1")" ||
  fail "cannot resolve Forge binary: $1"
readonly forge_source
[[ -f "$forge_source" && -x "$forge_source" ]] ||
  fail "Forge binary is not executable: $forge_source"

for required_command in awk cat chmod cp cut date dirname env find grep install ln mkdir mktemp mv realpath rm sha256sum sleep stat touch wc; do
  command -v "$required_command" >/dev/null 2>&1 ||
    fail "required command not found: $required_command"
done

smoke_root="$(mktemp -d "${TMPDIR:-/tmp}/forge-backup-smoke.XXXXXX")"
cleanup() {
  if [[ "${FORGE_SMOKE_KEEP_ROOT:-0}" = 1 ]]; then
    printf 'off-host backup smoke: retained debug root: %s\n' \
      "${smoke_root:-unset}" >&2
    return
  fi
  if [[ -n "${smoke_root:-}" && "$smoke_root" != "/" ]]; then
    rm -rf -- "$smoke_root"
  fi
}
trap cleanup EXIT

install -d -m 0700 \
  "$smoke_root/bin" \
  "$smoke_root/state" \
  "$smoke_root/backups" \
  "$smoke_root/config" \
  "$smoke_root/uploader-credentials" \
  "$smoke_root/remote"
printf 'forge-backup-smoke-v1\n' >"$smoke_root/.forge-backup-test-root"
printf 'forge-restore-drill-smoke-v1\n' \
  >"$smoke_root/.forge-restore-drill-test-root"
install -m 0755 "$forge_source" "$smoke_root/bin/forge-real"

cat >"$smoke_root/bin/forge" <<'FIXTURE_FORGE'
#!/usr/bin/env bash
set -euo pipefail

[[ -z "${CREDENTIALS_DIRECTORY:-}" &&
  -z "${FORGE_BACKUP_OFFHOST_COMMAND_FILE:-}" &&
  -z "${FORGE_BACKUP_OFFHOST_DESTINATION_FILE:-}" &&
  -z "${FORGE_BACKUP_OFFHOST_GENERATION_FILE:-}" ]] || exit 66

exec "$(dirname -- "$0")/forge-real" "$@"
FIXTURE_FORGE
chmod 0755 "$smoke_root/bin/forge"

initialization_key="$smoke_root/initialization-only.key"
printf 'off-host-smoke-initialization-key\n' |
  sha256sum |
  cut -d' ' -f1 >"$initialization_key"
env \
  -u CREDENTIALS_DIRECTORY \
  -u FORGE_BACKUP_OFFHOST_COMMAND_FILE \
  -u FORGE_BACKUP_OFFHOST_DESTINATION_FILE \
  -u FORGE_BACKUP_OFFHOST_GENERATION_FILE \
  FORGE_SECRET_VAULT_KEY_FILE="$initialization_key" \
  "$smoke_root/bin/forge" \
  --store "$smoke_root/state/forge.sqlite" \
  plan \
  --goal "off-host-backup-recovery-canary" \
  --output json >"$smoke_root/initial-plan.json"
[[ ! -e "$smoke_root/state/forge.sqlite.secret.key" ]] ||
  fail "real Forge created an adjacent fallback vault key"

workflow_id=""
while IFS= read -r plan_line; do
  if [[ "$plan_line" =~ \"workflow_id\"[[:space:]]*:[[:space:]]*\"([^\"]+)\" ]]; then
    workflow_id="${BASH_REMATCH[1]}"
    break
  fi
done <"$smoke_root/initial-plan.json"
[[ "$workflow_id" = wf_* ]] ||
  fail "real Forge plan did not return a workflow id"

cat >"$smoke_root/bin/offhost-uploader" <<'FIXTURE_UPLOADER'
#!/usr/bin/env bash
set -euo pipefail

operation="${1:-}"
[[ -n "$operation" ]] || exit 64
shift
[[ -z "${FORGE_SECRET_VAULT_KEY:-}" &&
  -z "${FORGE_SECRET_VAULT_KEY_FILE:-}" &&
  -z "${FORGE_SECRET_VAULT_PREVIOUS_KEYS:-}" &&
  -z "${FORGE_SECRET_VAULT_PREVIOUS_KEY_FILES:-}" &&
  -z "${FORGE_OPS_BEARER_TOKEN:-}" &&
  -z "${FORGE_OPS_BEARER_TOKEN_FILE:-}" ]] || exit 65
[[ -n "${FIXTURE_EXPECTED_CREDENTIALS_DIRECTORY:-}" ]] || exit 67
[[ "${CREDENTIALS_DIRECTORY:-}" = "$FIXTURE_EXPECTED_CREDENTIALS_DIRECTORY" ]] ||
  exit 67

source_path=""
destination=""
object_name=""
expected_sha256=""
output_path=""
sha256_output=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      [[ $# -ge 2 ]] || exit 64
      source_path="$2"
      shift 2
      ;;
    --destination)
      [[ $# -ge 2 ]] || exit 64
      destination="$2"
      shift 2
      ;;
    --object)
      [[ $# -ge 2 ]] || exit 64
      object_name="$2"
      shift 2
      ;;
    --sha256)
      [[ $# -ge 2 ]] || exit 64
      expected_sha256="$2"
      shift 2
      ;;
    --output)
      [[ $# -ge 2 ]] || exit 64
      output_path="$2"
      shift 2
      ;;
    --sha256-output)
      [[ $# -ge 2 ]] || exit 64
      sha256_output="$2"
      shift 2
      ;;
    *)
      exit 64
      ;;
  esac
done

[[ -n "$destination" && -n "$object_name" ]]
destination_scope="$(printf '%s' "$destination" | sha256sum | cut -d' ' -f1)"
remote_directory="$FIXTURE_REMOTE_DIR/$destination_scope"
remote_path="$remote_directory/$object_name"
mkdir -p -- "$remote_directory"
printf '%s\t%s\t%s\n' "$operation" "$destination" "$object_name" \
  >>"$FIXTURE_UPLOAD_LOG"

if [[ "$operation" = download &&
  -e "${FIXTURE_HANG_DOWNLOAD_FILE:-}" ]]; then
  exec sleep 3600
fi

case "$operation" in
  upload)
    [[ -f "$source_path" && "$expected_sha256" =~ ^[0-9a-f]{64}$ ]]
    read -r source_sha256 _ < <(sha256sum -- "$source_path")
    [[ "$source_sha256" = "$expected_sha256" ]]
    if [[ -e "$remote_path" ]]; then
      read -r remote_sha256 _ < <(sha256sum -- "$remote_path")
      [[ "$remote_sha256" = "$expected_sha256" ]]
    else
      cp -- "$source_path" "$remote_path"
    fi
    printf '%s\n' "$expected_sha256" >"$remote_path.sha256"
    ;;
  verify)
    [[ ! -e "$FIXTURE_FAIL_VERIFY_FILE" ]] || exit 42
    [[ -z "$source_path" && -z "$expected_sha256" ]]
    [[ -f "$remote_path" ]]
    read -r remote_sha256 _ < <(sha256sum -- "$remote_path")
    printf '%s\n' "$remote_sha256"
    ;;
  download)
    [[ -f "$remote_path" && -f "$remote_path.sha256" ]]
    [[ -n "$output_path" && -n "$sha256_output" ]]
    cp -- "$remote_path" "$output_path"
    cp -- "$remote_path.sha256" "$sha256_output"
    ;;
  *)
    exit 64
    ;;
esac
FIXTURE_UPLOADER

chmod 0755 \
  "$smoke_root/bin/forge" \
  "$smoke_root/bin/forge-real" \
  "$smoke_root/bin/offhost-uploader"

readonly upload_log="$smoke_root/upload.log"
readonly fail_verify_file="$smoke_root/fail-verify"
readonly hang_download_file="$smoke_root/hang-download"
readonly uploader_credentials_dir="$smoke_root/uploader-credentials"

run_forge_without_vault() {
  env \
    -u CREDENTIALS_DIRECTORY \
    -u FORGE_BACKUP_OFFHOST_COMMAND_FILE \
    -u FORGE_BACKUP_OFFHOST_DESTINATION_FILE \
    -u FORGE_BACKUP_OFFHOST_GENERATION_FILE \
    -u FORGE_SECRET_VAULT_KEY \
    -u FORGE_SECRET_VAULT_KEY_FILE \
    -u FORGE_SECRET_VAULT_PREVIOUS_KEYS \
    -u FORGE_SECRET_VAULT_PREVIOUS_KEY_FILES \
    "$smoke_root/bin/forge" "$@"
}

run_forge_without_vault \
  --store "$smoke_root/state/forge.sqlite" \
  store check \
  --output json >"$smoke_root/no-vault-check.json"

run_backup() {
  local timestamp="$1"

  env \
    -u FORGE_PRODUCTION_MODE \
    -u FORGE_SECRET_VAULT_KEY \
    -u FORGE_SECRET_VAULT_KEY_FILE \
    -u FORGE_SECRET_VAULT_PREVIOUS_KEYS \
    -u FORGE_SECRET_VAULT_PREVIOUS_KEY_FILES \
    FORGE_BACKUP_TEST_MODE=1 \
    FORGE_BACKUP_TEST_ROOT="$smoke_root" \
    FORGE_BACKUP_TEST_TIMESTAMP="$timestamp" \
    CREDENTIALS_DIRECTORY="$uploader_credentials_dir" \
    FIXTURE_EXPECTED_CREDENTIALS_DIRECTORY="$uploader_credentials_dir" \
    FIXTURE_REMOTE_DIR="$smoke_root/remote" \
    FIXTURE_UPLOAD_LOG="$upload_log" \
    FIXTURE_FAIL_VERIFY_FILE="$fail_verify_file" \
    FIXTURE_HANG_DOWNLOAD_FILE="$hang_download_file" \
    "$backup_script"
}

if run_backup 20260724T010000Z >"$smoke_root/missing-config.log" 2>&1; then
  fail "missing off-host configuration unexpectedly succeeded"
fi
grep -q "off-host command file is not a readable regular file" \
  "$smoke_root/missing-config.log" ||
  fail "missing configuration did not fail closed"
[[ ! -e "$smoke_root/backups/forge-20260724T010000Z.sqlite" ]] ||
  fail "missing configuration created a backup"

for production_value in 1 true yes on; do
  if env \
    FORGE_PRODUCTION_MODE="$production_value" \
    FORGE_BACKUP_TEST_MODE=1 \
    FORGE_BACKUP_TEST_ROOT="$smoke_root" \
    "$backup_script" \
    >"$smoke_root/production-conflict-$production_value.log" 2>&1; then
    fail "production value $production_value accepted test overrides"
  fi
  grep -q \
    "test overrides are forbidden" \
    "$smoke_root/production-conflict-$production_value.log" ||
    fail "production value $production_value did not fail closed"
done

printf '%s\n' "$smoke_root/bin/offhost-uploader" \
  >"$smoke_root/config/offhost-command"
printf 'fixture://primary\n' >"$smoke_root/config/offhost-destination"
printf 'primary-generation-1\n' >"$smoke_root/config/offhost-generation"

ln -s "$smoke_root/bin/offhost-uploader" "$smoke_root/bin/symlink-uploader"
printf '%s\n' "$smoke_root/bin/symlink-uploader" \
  >"$smoke_root/config/offhost-command"
if run_backup 20260724T011000Z >"$smoke_root/symlink-uploader.log" 2>&1; then
  fail "symlink uploader unexpectedly passed the runtime path audit"
fi
grep -Eq "canonical|symlink" "$smoke_root/symlink-uploader.log" ||
  fail "symlink uploader did not fail with a path-audit error"

install -d -m 0777 "$smoke_root/unsafe-uploader-parent"
cp -- \
  "$smoke_root/bin/offhost-uploader" \
  "$smoke_root/unsafe-uploader-parent/offhost-uploader"
chmod 0755 "$smoke_root/unsafe-uploader-parent/offhost-uploader"
printf '%s\n' "$smoke_root/unsafe-uploader-parent/offhost-uploader" \
  >"$smoke_root/config/offhost-command"
if run_backup 20260724T012000Z >"$smoke_root/writable-parent.log" 2>&1; then
  fail "uploader under a writable parent unexpectedly passed the path audit"
fi
grep -q "writable by group or other" "$smoke_root/writable-parent.log" ||
  fail "writable uploader parent did not fail with a path-audit error"

printf '%s\n' "$smoke_root/bin/offhost-uploader" \
  >"$smoke_root/config/offhost-command"

readonly expired_object="forge-20260701T000000Z.sqlite"
readonly expired_backup="$smoke_root/backups/$expired_object"
cp -- "$smoke_root/state/forge.sqlite" "$expired_backup"
touch -d '20 days ago' "$expired_backup"
touch "$fail_verify_file"

if run_backup 20260724T020000Z >"$smoke_root/verify-failure.log" 2>&1; then
  fail "remote verification failure unexpectedly succeeded"
fi
rm -f -- "$fail_verify_file"
[[ -f "$expired_backup" ]] ||
  fail "failed remote verification removed the expired local backup"
[[ ! -e "$expired_backup.offhost-verified" ]] ||
  fail "failed remote verification created a success marker"
[[ ! -e "$smoke_root/backups/forge-20260724T020000Z.sqlite" ]] ||
  fail "failed drain created a new snapshot before clearing pending backups"
[[ "$(wc -l <"$upload_log")" -eq 2 ]] ||
  fail "failed run did not stop immediately after upload and verify"

run_backup 20260724T030000Z >"$smoke_root/retry-success.log"
[[ ! -e "$expired_backup" && ! -e "$expired_backup.offhost-verified" ]] ||
  fail "verified expired backup was not removed by local retention"
[[ -f "$smoke_root/backups/forge-20260724T030000Z.sqlite.offhost-verified" ]] ||
  fail "successful retry did not mark forge-20260724T030000Z.sqlite"
awk -F '\t' -v object="$expired_object" '
  $1 == "verify" && $2 == "fixture://primary" && $3 == object { count++ }
  END { exit(count == 3 ? 0 : 1) }
' "$upload_log" ||
  fail "expired backup was not reverified immediately before deletion"

operations_before="$(wc -l <"$upload_log")"
run_backup 20260724T040000Z >"$smoke_root/marker-success.log"
operations_after="$(wc -l <"$upload_log")"
[[ $((operations_after - operations_before)) -eq 3 ]] ||
  fail "valid markers did not suppress redundant uploads"

printf 'fixture://secondary\n' >"$smoke_root/config/offhost-destination"
operations_before="$(wc -l <"$upload_log")"
run_backup 20260724T050000Z >"$smoke_root/destination-change.log"
operations_after="$(wc -l <"$upload_log")"
[[ $((operations_after - operations_before)) -eq 9 ]] ||
  fail "destination change did not replicate all three retained backups"
awk -F '\t' '
  $1 == "upload" && $2 == "fixture://secondary" { uploads++ }
  $1 == "verify" && $2 == "fixture://secondary" { verifies++ }
  $1 == "download" && $2 == "fixture://secondary" { downloads++ }
  END { exit(uploads == 3 && verifies == 3 && downloads == 3 ? 0 : 1) }
' "$upload_log" ||
  fail "destination change did not challenge every retained backup"

printf 'secondary-generation-2\n' >"$smoke_root/config/offhost-generation"
operations_before="$(wc -l <"$upload_log")"
run_backup 20260724T060000Z >"$smoke_root/generation-change.log"
operations_after="$(wc -l <"$upload_log")"
[[ $((operations_after - operations_before)) -eq 12 ]] ||
  fail "generation change did not rechallenge all four retained backups"

printf '\n# uploader pin rotation\n' >>"$smoke_root/bin/offhost-uploader"
operations_before="$(wc -l <"$upload_log")"
run_backup 20260724T070000Z >"$smoke_root/uploader-pin-change.log"
operations_after="$(wc -l <"$upload_log")"
[[ $((operations_after - operations_before)) -eq 15 ]] ||
  fail "uploader pin change did not rechallenge all five retained backups"

object_epoch="$(( $(date -u +%s) - 60 ))"
selected_timestamp="$(date -u -d "@$object_epoch" +%Y%m%dT%H%M%SZ)"
selected_object="forge-$selected_timestamp.sqlite"
incident_epoch="$((object_epoch + 60))"
run_backup "$selected_timestamp" >"$smoke_root/dynamic-restore-source.log"

run_restore_drill() {
  local drill_output_dir="$1"
  local drill_object="$2"
  local drill_object_epoch="$3"
  local drill_incident_epoch="$4"
  local drill_max_rto="$5"
  local drill_canary_workflow_id="${6:-$workflow_id}"

  env \
    -u FORGE_PRODUCTION_MODE \
    -u FORGE_SECRET_VAULT_KEY \
    -u FORGE_SECRET_VAULT_KEY_FILE \
    -u FORGE_SECRET_VAULT_PREVIOUS_KEYS \
    -u FORGE_SECRET_VAULT_PREVIOUS_KEY_FILES \
    FORGE_RESTORE_DRILL_TEST_MODE=1 \
    FORGE_RESTORE_DRILL_TEST_ROOT="$smoke_root" \
    FORGE_SECRET_VAULT_KEY_FILE="$initialization_key" \
    FORGE_BACKUP_OFFHOST_COMMAND_FILE=/must/not/reach/forge/command \
    FORGE_BACKUP_OFFHOST_DESTINATION_FILE=/must/not/reach/forge/destination \
    FORGE_BACKUP_OFFHOST_GENERATION_FILE=/must/not/reach/forge/generation \
    CREDENTIALS_DIRECTORY="$uploader_credentials_dir" \
    FIXTURE_EXPECTED_CREDENTIALS_DIRECTORY="$uploader_credentials_dir" \
    FIXTURE_REMOTE_DIR="$smoke_root/remote" \
    FIXTURE_UPLOAD_LOG="$upload_log" \
    FIXTURE_FAIL_VERIFY_FILE="$fail_verify_file" \
    FIXTURE_HANG_DOWNLOAD_FILE="$hang_download_file" \
    "$restore_drill_script" \
    --forge "$smoke_root/bin/forge" \
    --uploader "$smoke_root/bin/offhost-uploader" \
    --target "fixture://secondary" \
    --object "$drill_object" \
    --object-epoch "$drill_object_epoch" \
    --incident-epoch "$drill_incident_epoch" \
    --max-rpo-seconds 86400 \
    --max-rto-seconds "$drill_max_rto" \
    --approved-by off-host-backup-smoke \
    --canary-workflow-id "$drill_canary_workflow_id" \
    --output-dir "$drill_output_dir"
}

if env \
  FORGE_PRODUCTION_MODE=true \
  FORGE_RESTORE_DRILL_TEST_MODE=1 \
  FORGE_RESTORE_DRILL_TEST_ROOT="$smoke_root" \
  "$restore_drill_script" --help \
  >"$smoke_root/restore-production-conflict.log" 2>&1; then
  fail "production mode unexpectedly accepted restore-drill test overrides"
fi
grep -q "test mode is forbidden" \
  "$smoke_root/restore-production-conflict.log" ||
  fail "restore-drill production/test conflict did not fail closed"

mismatch_restore_dir="$smoke_root/mismatch-restore"
install -d -m 0700 "$mismatch_restore_dir"
if run_restore_drill \
  "$mismatch_restore_dir" \
  "$selected_object" \
  "$((object_epoch + 1))" \
  "$incident_epoch" \
  1800 \
  >"$smoke_root/object-epoch-mismatch.log" 2>&1; then
  fail "object epoch mismatch unexpectedly passed the restore drill"
fi
grep -q "does not match the canonical object timestamp" \
  "$smoke_root/object-epoch-mismatch.log" ||
  fail "object epoch mismatch did not fail on canonical binding"

noncanonical_restore_dir="$smoke_root/noncanonical-restore"
install -d -m 0700 "$noncanonical_restore_dir"
if run_restore_drill \
  "$noncanonical_restore_dir" \
  "forge-20260230T120000Z.sqlite" \
  "$object_epoch" \
  "$incident_epoch" \
  1800 \
  >"$smoke_root/noncanonical-object.log" 2>&1; then
  fail "non-canonical object name unexpectedly passed the restore drill"
fi
grep -Eq "invalid UTC timestamp|non-canonical" \
  "$smoke_root/noncanonical-object.log" ||
  fail "non-canonical object did not fail closed"

stale_rto_restore_dir="$smoke_root/stale-rto-restore"
install -d -m 0700 "$stale_rto_restore_dir"
stale_incident_epoch="$(( $(date -u +%s) - 3 ))"
if run_restore_drill \
  "$stale_rto_restore_dir" \
  "$selected_object" \
  "$object_epoch" \
  "$stale_incident_epoch" \
  1 \
  >"$smoke_root/stale-rto.log" 2>&1; then
  fail "an already exhausted end-to-end RTO unexpectedly passed"
fi
grep -q "RTO budget is exhausted" "$smoke_root/stale-rto.log" ||
  fail "stale incident time did not fail the end-to-end RTO gate"

hang_restore_dir="$smoke_root/hang-restore"
install -d -m 0700 "$hang_restore_dir"
touch "$hang_download_file"
hang_incident_epoch="$(date -u +%s)"
if run_restore_drill \
  "$hang_restore_dir" \
  "$selected_object" \
  "$object_epoch" \
  "$hang_incident_epoch" \
  3 \
  >"$smoke_root/hung-uploader.log" 2>&1; then
  fail "hung uploader unexpectedly passed the restore drill"
fi
rm -f -- "$hang_download_file"
grep -Eq "off-host download (exceeded|failed).*RTO budget" \
  "$smoke_root/hung-uploader.log" ||
  fail "hung uploader was not stopped by the remaining RTO watchdog"

missing_canary_restore_dir="$smoke_root/missing-canary-restore"
install -d -m 0700 "$missing_canary_restore_dir"
if run_restore_drill \
  "$missing_canary_restore_dir" \
  "$selected_object" \
  "$object_epoch" \
  "$incident_epoch" \
  1800 \
  "${workflow_id}_missing" \
  >"$smoke_root/missing-canary.log" 2>&1; then
  fail "missing workflow-id canary unexpectedly passed the restore drill"
fi
grep -q "canary workflow inspection failed" \
  "$smoke_root/missing-canary.log" ||
  fail "missing workflow-id canary did not fail by exact Forge lookup"

manual_restore_dir="$smoke_root/manual-restore"
install -d -m 0700 "$manual_restore_dir"
run_restore_drill \
  "$manual_restore_dir" \
  "$selected_object" \
  "$object_epoch" \
  "$incident_epoch" \
  1800 \
  >"$smoke_root/restore-drill.log"
grep -q '"status": "passed"' "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not produce a passing report"
grep -q '"rpo_seconds": 60' "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not measure the expected RPO"
grep -Eq '"rto_seconds": [0-9]+' "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not measure end-to-end RTO"
grep -Eq '"hot_script_rto_milliseconds": [0-9]+' \
  "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not retain the monotonic hot-script metric"
grep -Fq "\"canary_workflow_id\": \"$workflow_id\"" \
  "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not record the exact workflow-id canary"
grep -Eq '"forge_sha256": "[0-9a-f]{64}"' \
  "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not record the Forge executable hash"
grep -Eq '"uploader_sha256": "[0-9a-f]{64}"' \
  "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not record the uploader hash"
grep -Fq '"forge_version": "forge ' "$manual_restore_dir/drill-report.json" ||
  fail "restore drill did not record Forge --version"
grep -Fq '"schema_version": "forge.ops.snapshot.v1"' \
  "$manual_restore_dir/ops-snapshot.json" ||
  fail "restore drill did not probe the restored store through Forge Ops"
[[ ! -e "$manual_restore_dir/.ops-token" &&
  ! -e "$manual_restore_dir/.ops-curl.conf" ]] ||
  fail "restore drill retained temporary Ops credentials"

printf '/usr/bin/true\n' >"$smoke_root/config/offhost-command"
if run_backup 20260724T080000Z >"$smoke_root/exit-zero-empty-verify.log" 2>&1; then
  fail "/usr/bin/true unexpectedly satisfied remote verification"
fi
grep -q "remote verification output must contain exactly one lowercase SHA-256" \
  "$smoke_root/exit-zero-empty-verify.log" ||
  fail "empty exit-zero verification did not fail on its output contract"

printf 'off-host backup smoke: ok\n'
