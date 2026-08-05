#!/usr/bin/env bash
set -euo pipefail
umask 077

fail() {
  printf 'production-readiness self-test: %s\n' "$*" >&2
  exit 1
}

bundle_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
bounded_drill="$bundle_dir/foundry-bounded-load-drill"
upgrade_drill="$bundle_dir/foundry-upgrade-rollback-drill"
[[ -x "$bounded_drill" ]] || fail "bounded-load drill is not executable"
[[ -x "$upgrade_drill" ]] || fail "upgrade/rollback drill is not executable"

test_root="$(mktemp -d /tmp/foundry-production-readiness-test.XXXXXX)"
chmod 0700 "$test_root"
printf '%s\n' "foundry-production-readiness-test-v1" \
  >"$test_root/.foundry-production-readiness-test-root"

cleanup() {
  if [[ "$test_root" = /tmp/foundry-production-readiness-test.* &&
    -f "$test_root/.foundry-production-readiness-test-root" ]]; then
    rm -rf -- "$test_root"
  fi
}
trap cleanup EXIT HUP INT TERM

mkdir -m 0700 \
  "$test_root/bin" \
  "$test_root/stubs" \
  "$test_root/evidence" \
  "$test_root/evidence-bounded-interrupted" \
  "$test_root/evidence-symlink" \
  "$test_root/evidence-upgrade" \
  "$test_root/evidence-upgrade-interrupted" \
  "$test_root/evidence-newer-previous"

source_store="$test_root/foundry.sqlite"
vault_key_file="$test_root/secret.key"
printf '%064d\n' 0 >"$vault_key_file"
chmod 0600 "$vault_key_file"
python3 - "$source_store" <<'PY'
import os
import sqlite3
import sys

path = sys.argv[1]
connection = sqlite3.connect(path)
connection.execute(
    "CREATE TABLE workflows (id TEXT PRIMARY KEY, goal TEXT NOT NULL)"
)
connection.execute(
    "INSERT INTO workflows(id, goal) VALUES (?, ?)",
    ("wf_bounded_load_canary", "offline bounded-load canary"),
)
connection.commit()
connection.close()
os.chmod(path, 0o600)
PY
source_store_sha256="$(sha256sum "$source_store" | awk '{print $1}')"

fake_foundry="$test_root/bin/foundry"
cat >"$fake_foundry" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1-}" = "--version" ]]; then
  printf 'foundry 0.6.0\n'
  exit 0
fi

store=""
if [[ "${1-}" = "--store" ]]; then
  store="${2-}"
  shift 2
fi

case "${1-} ${2-}" in
  "store check")
    python3 - "$store" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
result = connection.execute("PRAGMA quick_check").fetchone()
connection.close()
if result != ("ok",):
    raise SystemExit(1)
PY
    printf '{"status":"ok"}\n'
    ;;
  "ops serve")
    if [[ -n "${FOUNDRY_SELF_TEST_OPS_STARTED_FILE:-}" ]]; then
      : >"$FOUNDRY_SELF_TEST_OPS_STARTED_FILE"
    fi
    trap 'exit 0' TERM INT
    while :; do sleep 1; done
    ;;
  *)
    if [[ "${1-}" = "inspect" && "${2-}" = "wf_bounded_load_canary" ]]; then
      python3 - "$store" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
row = connection.execute(
    "SELECT id FROM workflows WHERE id = ?", ("wf_bounded_load_canary",)
).fetchone()
connection.close()
if row != ("wf_bounded_load_canary",):
    raise SystemExit(1)
PY
      printf '{"workflow_id":"wf_bounded_load_canary"}\n'
    else
      printf 'unsupported fake Foundry command: %s\n' "$*" >&2
      exit 2
    fi
    ;;
esac
EOF
chmod 0755 "$fake_foundry"

fake_upgrade_foundry="$test_root/bin/fake-upgrade-foundry"
cat >"$fake_upgrade_foundry" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

binary_name="${0##*/}"
case "$binary_name" in
  candidate-foundry) version="0.6.0"; candidate=1 ;;
  previous-foundry) version="0.5.3"; candidate=0 ;;
  newer-foundry) version="0.6.0"; candidate=0 ;;
  *) printf 'unexpected fake binary name: %s\n' "$binary_name" >&2; exit 2 ;;
esac

if [[ "${1-}" = "--version" ]]; then
  printf 'foundry %s\n' "$version"
  exit 0
fi

store=""
if [[ "${1-}" = "--store" ]]; then
  store="${2-}"
  shift 2
fi

migrate_candidate() {
  [[ "$candidate" = 1 ]] || return 0
  python3 - "$store" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(sys.argv[1])
connection.execute(
    "CREATE TABLE IF NOT EXISTS candidate_schema_marker "
    "(id INTEGER PRIMARY KEY, applied INTEGER NOT NULL)"
)
connection.execute(
    "INSERT OR REPLACE INTO candidate_schema_marker(id, applied) VALUES (1, 1)"
)
connection.commit()
connection.close()
PY
}

case "${1-} ${2-}" in
  "store check")
    migrate_candidate
    python3 - "$store" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
result = connection.execute("PRAGMA quick_check").fetchone()
connection.close()
if result != ("ok",):
    raise SystemExit(1)
PY
    printf '{"status":"ok"}\n'
    ;;
  "store restore")
    source=""
    shift 2
    while (($# > 0)); do
      case "$1" in
        --source) source="${2-}"; shift 2 ;;
        *) shift ;;
      esac
    done
    [[ -n "$source" ]] || exit 2
    python3 - "$source" "$store" <<'PY'
import sqlite3
import sys
source = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
destination = sqlite3.connect(sys.argv[2])
source.backup(destination)
destination.commit()
destination.close()
source.close()
PY
    printf '{"status":"restored"}\n'
    ;;
  "ops serve")
    migrate_candidate
    if [[ -n "${FOUNDRY_SELF_TEST_OPS_STARTED_FILE:-}" ]]; then
      : >"$FOUNDRY_SELF_TEST_OPS_STARTED_FILE"
    fi
    trap 'exit 0' TERM INT
    while :; do sleep 1; done
    ;;
  *)
    if [[ "${1-}" = "inspect" && "${2-}" = "wf_bounded_load_canary" ]]; then
      migrate_candidate
      python3 - "$store" <<'PY'
import sqlite3
import sys
connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
row = connection.execute(
    "SELECT id FROM workflows WHERE id = ?", ("wf_bounded_load_canary",)
).fetchone()
connection.close()
if row != ("wf_bounded_load_canary",):
    raise SystemExit(1)
PY
      printf '{"workflow_id":"wf_bounded_load_canary"}\n'
    else
      printf 'unsupported fake Foundry command: %s\n' "$*" >&2
      exit 2
    fi
    ;;
esac
EOF
chmod 0755 "$fake_upgrade_foundry"
cp --no-clobber -- "$fake_upgrade_foundry" "$test_root/bin/candidate-foundry"
cp --no-clobber -- "$fake_upgrade_foundry" "$test_root/bin/previous-foundry"
cp --no-clobber -- "$fake_upgrade_foundry" "$test_root/bin/newer-foundry"
printf '%s\n' '# candidate fixture identity' >>"$test_root/bin/candidate-foundry"
printf '%s\n' '# previous fixture identity' >>"$test_root/bin/previous-foundry"
printf '%s\n' '# newer fixture identity' >>"$test_root/bin/newer-foundry"
chmod 0755 \
  "$test_root/bin/candidate-foundry" \
  "$test_root/bin/previous-foundry" \
  "$test_root/bin/newer-foundry"

cat >"$test_root/stubs/curl" <<'EOF'
#!/usr/bin/env bash
if [[ -n "${FOUNDRY_SELF_TEST_CURL_DELAY_SECONDS:-}" ]]; then
  sleep "$FOUNDRY_SELF_TEST_CURL_DELAY_SECONDS"
fi
case " $* " in
  *time_total*) printf '200 0.001' ;;
  *) printf '200' ;;
esac
EOF
chmod 0755 "$test_root/stubs/curl"

assert_test_mode_rejected_by_production_mode() {
  local script="$1"
  local script_label="${script##*/}"
  local production_value="$2"
  local output="$test_root/${script_label}-production-${production_value}.stdout"

  if PATH="$test_root/stubs:$PATH" \
    FOUNDRY_PRODUCTION_READINESS_TEST_MODE=1 \
    FOUNDRY_PRODUCTION_READINESS_TEST_ROOT="$test_root" \
    FOUNDRY_PRODUCTION_MODE="$production_value" \
    "$script" >"$output" 2>&1; then
    fail "$script_label accepted test mode with FOUNDRY_PRODUCTION_MODE=$production_value"
  fi
  grep -F 'test mode is forbidden when FOUNDRY_PRODUCTION_MODE is enabled' \
    "$output" >/dev/null ||
    fail "$script_label did not fail at the production/test-mode boundary"
}

assert_unknown_production_mode_rejected() {
  local script="$1"
  local script_label="${script##*/}"
  local output="$test_root/${script_label}-production-unknown.stdout"

  if PATH="$test_root/stubs:$PATH" \
    FOUNDRY_PRODUCTION_READINESS_TEST_MODE=0 \
    FOUNDRY_PRODUCTION_MODE=enabled \
    "$script" >"$output" 2>&1; then
    fail "$script_label accepted an unknown FOUNDRY_PRODUCTION_MODE value"
  fi
  grep -F 'FOUNDRY_PRODUCTION_MODE must be one of 0,false,no,off,1,true,yes,on' \
    "$output" >/dev/null ||
    fail "$script_label did not reject an unknown FOUNDRY_PRODUCTION_MODE value"
}

wait_for_file() {
  local path="$1"
  local process_id="$2"
  local _attempt=""

  for _attempt in $(seq 1 500); do
    [[ -f "$path" && ! -L "$path" ]] && return 0
    kill -0 "$process_id" 2>/dev/null || return 1
    sleep 0.01
  done
  return 1
}

wait_for_stage_dir() {
  local output_dir="$1"
  local stage_pattern="$2"
  local process_id="$3"
  local _attempt=""
  local found=""

  for _attempt in $(seq 1 500); do
    found="$(
      find "$output_dir" -mindepth 1 -maxdepth 1 \
        -name "$stage_pattern" -print -quit
    )"
    [[ -z "$found" ]] || return 0
    kill -0 "$process_id" 2>/dev/null || return 1
    sleep 0.01
  done
  return 1
}

resolve_drill_pid() {
  local runner_pid="$1"
  local expected_script="$2"
  local process_id=""
  local command_line=""
  local child_list=""
  local children_file="/proc/$runner_pid/task/$runner_pid/children"

  command_line="$(
    tr '\0' ' ' <"/proc/$runner_pid/cmdline" 2>/dev/null || true
  )"
  if [[ "$command_line" = *"$expected_script"* ]]; then
    printf '%s\n' "$runner_pid"
    return 0
  fi

  [[ ! -r "$children_file" ]] || child_list="$(<"$children_file")"
  for process_id in $child_list; do
    [[ "$process_id" =~ ^[0-9]+$ ]] || continue
    command_line="$(
      tr '\0' ' ' <"/proc/$process_id/cmdline" 2>/dev/null || true
    )"
    if [[ "$command_line" = *"$expected_script"* ]]; then
      printf '%s\n' "$process_id"
      return 0
    fi
  done
  return 1
}

assert_term_cleanup() {
  local runner="$1"
  local label="$2"
  local output_dir="$3"
  local stage_pattern="$4"
  local report_name="$5"
  local expected_script="$6"
  local started_file="$test_root/$label-ops-started"
  local stdout_file="$test_root/$label-interrupted.stdout"
  local runner_pid=""
  local drill_pid=""
  local drill_status=0
  local staged_path=""

  FOUNDRY_SELF_TEST_OPS_STARTED_FILE="$started_file" \
    FOUNDRY_SELF_TEST_CURL_DELAY_SECONDS=1 \
    "$runner" "$output_dir" >"$stdout_file" 2>&1 &
  runner_pid="$!"

  if ! wait_for_stage_dir "$output_dir" "$stage_pattern" "$runner_pid" ||
    ! wait_for_file "$started_file" "$runner_pid"; then
    kill -TERM "$runner_pid" 2>/dev/null || true
    wait "$runner_pid" 2>/dev/null || true
    fail "$label drill did not reach the interruptible staged state"
  fi

  drill_pid="$(resolve_drill_pid "$runner_pid" "$expected_script")" ||
    fail "could not resolve the $label drill process"
  kill -TERM "$drill_pid" ||
    fail "could not interrupt $label drill"
  if wait "$runner_pid"; then
    fail "$label drill returned success after SIGTERM"
  else
    drill_status=$?
  fi
  [[ "$drill_status" -eq 143 ]] ||
    fail "$label drill returned $drill_status instead of 143 after SIGTERM"
  [[ ! -e "$output_dir/$report_name" && ! -L "$output_dir/$report_name" ]] ||
    fail "$label drill published a report after interruption"
  staged_path="$(
    find "$output_dir" -mindepth 1 -maxdepth 1 \
      -name "$stage_pattern" -print -quit
  )"
  [[ -z "$staged_path" ]] ||
    fail "$label drill left a staging path after interruption"
}

for drill in "$bounded_drill" "$upgrade_drill"; do
  for production_value in 1 true TRUE yes YeS on ON; do
    assert_test_mode_rejected_by_production_mode "$drill" "$production_value"
  done
  assert_unknown_production_mode_rejected "$drill"
done

run_bounded() {
  local output_dir="$1"
  PATH="$test_root/stubs:$PATH" \
    FOUNDRY_PRODUCTION_READINESS_TEST_MODE=1 \
    FOUNDRY_PRODUCTION_READINESS_TEST_ROOT="$test_root" \
    FOUNDRY_SECRET_VAULT_KEY_FILE="$vault_key_file" \
    "$bounded_drill" \
    --foundry "$fake_foundry" \
    --store "$source_store" \
    --release-version 0.6.0 \
    --canary-workflow-id wf_bounded_load_canary \
    --output-dir "$output_dir" \
    --operations 100 \
    --concurrency 4 \
    --max-duration-seconds 10 \
    --max-rss-bytes 67108864 \
    --ops-port 18767
}

assert_term_cleanup \
  run_bounded \
  bounded \
  "$test_root/evidence-bounded-interrupted" \
  '.bounded-load-stage.*' \
  bounded-load-report.json \
  "$bounded_drill"

run_bounded "$test_root/evidence" >"$test_root/bounded.stdout"
bounded_report="$test_root/evidence/bounded-load-report.json"
[[ -f "$bounded_report" && ! -L "$bounded_report" ]] ||
  fail "bounded-load report is missing or unsafe"
[[ "$(stat -c '%a' -- "$bounded_report")" = 600 ]] ||
  fail "bounded-load report mode is not 0600"

python3 - "$bounded_report" <<'PY'
import json
import sys

expected_top_level = {
    "schema_version",
    "kind",
    "status",
    "subject_version",
    "observed_at_epoch",
    "execution_mode",
    "producer",
    "claims",
    "evidence",
}
expected_claims = {
    "duration_seconds",
    "concurrency",
    "operation_count",
    "error_count",
    "p95_latency_millis",
    "max_rss_bytes",
    "max_rss_limit_bytes",
    "timeout_enforced",
    "resource_limit_enforced",
    "store_check_passed",
    "crash_restart_verified",
}
expected_evidence = {
    "collector_schema_version",
    "binary_sha256",
    "canary_workflow_id",
}
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    report = json.load(handle)
assert set(report) == expected_top_level
assert report["schema_version"] == "foundry.milestone.production_source_evidence.bounded_load.v1"
assert report["kind"] == "bounded_load"
assert report["status"] == "passed"
assert report["subject_version"] == "0.6.0"
assert report["execution_mode"] == "test"
assert report["producer"] == "foundry-bounded-load-drill"
assert isinstance(report["observed_at_epoch"], int) and report["observed_at_epoch"] > 0
claims = report["claims"]
evidence = report["evidence"]
assert set(claims) == expected_claims
assert set(evidence) == expected_evidence
assert claims["operation_count"] == 100
assert claims["error_count"] == 0
assert claims["p95_latency_millis"] == 1
assert 0 < claims["max_rss_bytes"] <= claims["max_rss_limit_bytes"]
assert claims["timeout_enforced"] is True
assert claims["resource_limit_enforced"] is True
assert claims["store_check_passed"] is True
assert claims["crash_restart_verified"] is True
assert evidence["collector_schema_version"] == "foundry.production_readiness.bounded_load_drill.v1"
assert len(evidence["binary_sha256"]) == 64
assert evidence["canary_workflow_id"] == "wf_bounded_load_canary"
PY
[[ "$(sha256sum "$source_store" | awk '{print $1}')" = "$source_store_sha256" ]] ||
  fail "bounded-load drill mutated the source store"
if grep -F 'offline bounded-load canary' "$bounded_report" >/dev/null; then
  fail "bounded-load report leaked canary content"
fi

if run_bounded "$test_root/evidence" >"$test_root/overwrite.stdout" 2>&1; then
  fail "bounded-load drill overwrote an existing report"
fi

ln -s "$test_root/elsewhere.json" \
  "$test_root/evidence-symlink/bounded-load-report.json"
if run_bounded "$test_root/evidence-symlink" >"$test_root/symlink.stdout" 2>&1; then
  fail "bounded-load drill accepted a symlink output"
fi

run_upgrade() {
  local output_dir="$1"
  PATH="$test_root/stubs:$PATH" \
    FOUNDRY_PRODUCTION_READINESS_TEST_MODE=1 \
    FOUNDRY_PRODUCTION_READINESS_TEST_ROOT="$test_root" \
    FOUNDRY_SECRET_VAULT_KEY_FILE="$vault_key_file" \
    "$upgrade_drill" \
    --candidate "$test_root/bin/candidate-foundry" \
    --previous "$test_root/bin/previous-foundry" \
    --store "$source_store" \
    --release-version 0.6.0 \
    --canary-workflow-id wf_bounded_load_canary \
    --output-dir "$output_dir" \
    --max-duration-seconds 10 \
    --ops-port 18768
}

assert_term_cleanup \
  run_upgrade \
  upgrade \
  "$test_root/evidence-upgrade-interrupted" \
  '.upgrade-rollback-stage.*' \
  upgrade-rollback-report.json \
  "$upgrade_drill"

run_upgrade "$test_root/evidence-upgrade" >"$test_root/upgrade.stdout"
upgrade_report="$test_root/evidence-upgrade/upgrade-rollback-report.json"
[[ -f "$upgrade_report" && ! -L "$upgrade_report" ]] ||
  fail "upgrade/rollback report is missing or unsafe"
[[ "$(stat -c '%a' -- "$upgrade_report")" = 600 ]] ||
  fail "upgrade/rollback report mode is not 0600"

python3 - "$upgrade_report" <<'PY'
import json
import sys

expected_top_level = {
    "schema_version",
    "kind",
    "status",
    "subject_version",
    "observed_at_epoch",
    "execution_mode",
    "producer",
    "claims",
    "evidence",
}
expected_claims = {
    "target_version",
    "simulation_completed",
    "pre_upgrade_backup_verified",
    "upgraded_store_check_passed",
    "upgraded_ops_health_passed",
    "rollback_completed",
    "previous_version_store_check_passed",
    "previous_version_ops_health_passed",
    "target_reinstalled_and_healthy",
}
expected_evidence = {
    "collector_schema_version",
    "previous_version",
    "candidate_binary_sha256",
    "previous_binary_sha256",
    "canary_workflow_id",
    "baseline_backup_sha256",
    "baseline_schema_sha256",
    "upgraded_schema_sha256",
    "rollback_schema_sha256",
    "reinstalled_schema_sha256",
}
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    report = json.load(handle)
assert set(report) == expected_top_level
assert report["schema_version"] == "foundry.milestone.production_source_evidence.upgrade_rollback.v1"
assert report["kind"] == "upgrade_rollback"
assert report["status"] == "passed"
assert report["subject_version"] == "0.6.0"
assert report["execution_mode"] == "test"
assert report["producer"] == "foundry-upgrade-rollback-drill"
assert isinstance(report["observed_at_epoch"], int) and report["observed_at_epoch"] > 0
claims = report["claims"]
evidence = report["evidence"]
assert set(claims) == expected_claims
assert set(evidence) == expected_evidence
assert claims["target_version"] == "0.6.0"
assert evidence["previous_version"] == "0.5.3"
assert evidence["candidate_binary_sha256"] != evidence["previous_binary_sha256"]
assert evidence["canary_workflow_id"] == "wf_bounded_load_canary"
assert evidence["collector_schema_version"] == "foundry.production_readiness.upgrade_rollback_drill.v1"
for field in {
    "candidate_binary_sha256",
    "previous_binary_sha256",
    "baseline_backup_sha256",
    "baseline_schema_sha256",
    "upgraded_schema_sha256",
    "rollback_schema_sha256",
    "reinstalled_schema_sha256",
}:
    assert len(evidence[field]) == 64
assert evidence["rollback_schema_sha256"] == evidence["baseline_schema_sha256"]
assert evidence["reinstalled_schema_sha256"] == evidence["upgraded_schema_sha256"]
for field in expected_claims - {"target_version"}:
    assert claims[field] is True
PY
[[ "$(sha256sum "$source_store" | awk '{print $1}')" = "$source_store_sha256" ]] ||
  fail "upgrade/rollback drill mutated the source store"
if grep -F 'offline bounded-load canary' "$upgrade_report" >/dev/null; then
  fail "upgrade/rollback report leaked canary content"
fi

if run_upgrade "$test_root/evidence-upgrade" >"$test_root/upgrade-overwrite.stdout" 2>&1; then
  fail "upgrade/rollback drill overwrote an existing report"
fi

if PATH="$test_root/stubs:$PATH" \
  FOUNDRY_PRODUCTION_READINESS_TEST_MODE=1 \
  FOUNDRY_PRODUCTION_READINESS_TEST_ROOT="$test_root" \
  FOUNDRY_SECRET_VAULT_KEY_FILE="$vault_key_file" \
  "$upgrade_drill" \
  --candidate "$test_root/bin/candidate-foundry" \
  --previous "$test_root/bin/newer-foundry" \
  --store "$source_store" \
  --release-version 0.6.0 \
  --canary-workflow-id wf_bounded_load_canary \
  --output-dir "$test_root/evidence-newer-previous" \
  --max-duration-seconds 10 \
  --ops-port 18768 \
  >"$test_root/newer-previous.stdout" 2>&1; then
  fail "upgrade/rollback drill accepted a newer previous base version"
fi

printf 'production-readiness self-test: PASS (bounded load, upgrade/rollback)\n'
