#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "forge systemd runtime self-test: $*" >&2
  exit 1
}

tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
systemd_dir="$(cd -- "$tests_dir/.." && pwd)"
runtime_unit="$systemd_dir/forge-runtime.service"
request_supervisor_unit="$systemd_dir/forge-request-supervisor.service"
backup_unit="$systemd_dir/forge-backup.service"
admin_wrapper="$systemd_dir/forge-admin"
installer="$systemd_dir/install-service.sh"
directory_offhost_self_test="$tests_dir/directory-offhost-self-test.sh"

[[ -f "$runtime_unit" ]] || fail "missing forge-runtime.service"
[[ -f "$request_supervisor_unit" ]] ||
  fail "missing forge-request-supervisor.service"
[[ -f "$backup_unit" ]] || fail "missing forge-backup.service"
[[ -f "$admin_wrapper" ]] || fail "missing forge-admin"
[[ -f "$installer" ]] || fail "missing install-service.sh"
[[ -x "$directory_offhost_self_test" ]] ||
  fail "missing executable directory-offhost-self-test.sh"

runtime_exec_start="$(grep -F 'ExecStart=' "$runtime_unit")"
[[ "$runtime_exec_start" == *' events runtime-daemon '* ]] ||
  fail "runtime service must execute the Forge runtime daemon"

for required_flag in \
  '--execute' \
  '--dispatch-activations' \
  '--continuous' \
  '--recover-stale-services' \
  '--scan-schedules'
do
  [[ "$runtime_exec_start" == *"$required_flag"* ]] ||
    fail "runtime service is missing $required_flag"
done

[[ "$runtime_exec_start" != *' request drive-loop '* ]] ||
  fail "runtime service must not launch an unclaimed per-run drive loop"

request_supervisor_exec_start="$(grep -F 'ExecStart=' "$request_supervisor_unit")"
[[ "$request_supervisor_exec_start" == *' request supervise '* ]] ||
  fail "request supervisor service must use the transactional supervisor"
[[ "$request_supervisor_exec_start" == *' --continuous '* &&
  "$request_supervisor_exec_start" == *' --max-cycles 0 '* ]] ||
  fail "request supervisor service must run the unbounded supervised loop"
[[ "$request_supervisor_exec_start" != *' request drive-loop '* ]] ||
  fail "request supervisor must not launch an unclaimed per-run drive loop"

backup_read_write_paths="$(grep -F 'ReadWritePaths=' "$backup_unit")"
[[ "$backup_read_write_paths" == 'ReadWritePaths=/var/backups/forge' ]] ||
  fail "backup service must grant writes only to the local backup directory"
grep -Fxq 'ReadOnlyPaths=/var/lib/forge' "$backup_unit" ||
  fail "backup service must keep the complete live state directory read-only"

grep -Fxq '  --property="ReadWritePaths=/var/lib/forge -/var/backups/forge" \' \
  "$admin_wrapper" ||
  fail "admin wrapper must tolerate a deliberately absent local backup directory"

for installer_contract in \
  '/etc/systemd/system/forge-runtime.service' \
  '/etc/systemd/system/forge-request-supervisor.service' \
  'read_unit_enable_state forge-runtime.service' \
  'systemctl stop forge-runtime.service' \
  'systemctl start forge-runtime.service' \
  'systemctl start forge-request-supervisor.service' \
  'systemctl --no-pager --full status forge-runtime.service'
do
  grep -Fq "$installer_contract" "$installer" ||
    fail "installer is missing runtime contract: $installer_contract"
done

bash -n "$installer"
bash "$directory_offhost_self_test"

printf 'forge systemd runtime self-test: PASS\n'
