#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=C

fail() {
  echo "foundry systemd installer: $*" >&2
  exit 1
}

staged_backup_dropin=""
staged_backup_mount_identity=""

audit_no_symlink_components() {
  local candidate="$1"
  local component="$candidate"

  while true; do
    [[ ! -L "$component" ]] ||
      fail "directory destination path component must not be a symbolic link: $component"
    [[ "$component" = "/" ]] && break
    component="${component%/*}"
    [[ -n "$component" ]] || component="/"
  done
}

audit_directory_offhost_permissions() {
  local candidate="$1"
  local account="$2"
  local account_uid=""
  local component=""
  local component_uid=""
  local component_mode=""

  account_uid="$(id -u "$account")" ||
    fail "cannot resolve directory destination owner account: $account"
  [[ "$(stat -c '%u' -- "$candidate")" -eq "$account_uid" ]] ||
    fail "directory destination must be owned by $account: $candidate"
  [[ "$(stat -c '%a' -- "$candidate")" = 700 ]] ||
    fail "directory destination mode must be exactly 0700: $candidate"

  component="${candidate%/*}"
  [[ -n "$component" ]] || component="/"
  while true; do
    [[ ! -L "$component" ]] ||
      fail "directory destination ancestor must not be a symbolic link: $component"
    component_uid="$(stat -c '%u' -- "$component")"
    [[ "$component_uid" -eq 0 || "$component_uid" -eq "$account_uid" ]] ||
      fail "directory destination ancestor must be owned by root or $account: $component"
    component_mode="$(stat -c '%a' -- "$component")"
    (( (8#$component_mode & 0022) == 0 )) ||
      fail "directory destination ancestor must not be writable by group or other: $component"
    [[ "$component" = "/" ]] && break
    component="${component%/*}"
    [[ -n "$component" ]] || component="/"
  done
}

resolve_directory_offhost_path() {
  local destination="$1"
  local account="${2:-}"
  local destination_path=""
  local canonical_path=""

  case "$destination" in
    file://*) ;;
    *)
      return 0
      ;;
  esac

  [[ "$destination" == file:///* ]] ||
    fail "directory destination must use file:///absolute/path"
  destination_path="${destination#file://}"
  [[ "$destination_path" =~ ^/[A-Za-z0-9._/@:+,-]+$ &&
    "$destination_path" != *"//"* &&
    "$destination_path" != *"/./"* &&
    "$destination_path" != *"/../"* &&
    "$destination_path" != */"." &&
    "$destination_path" != */".." &&
    "$destination_path" != */ ]] ||
    fail "directory destination must be an unencoded canonical file URI path"
  [[ -d "$destination_path" && ! -L "$destination_path" ]] ||
    fail "directory destination must be an existing non-symlink directory: $destination_path"
  canonical_path="$(realpath -e -- "$destination_path")" ||
    fail "cannot resolve directory destination: $destination_path"
  [[ "$canonical_path" = "$destination_path" ]] ||
    fail "directory destination must be canonical and contain no symlink or traversal: $destination_path"
  audit_no_symlink_components "$destination_path"
  if [[ -n "$account" ]]; then
    audit_directory_offhost_permissions "$destination_path" "$account"
  fi
  case "$destination_path" in
    /var/lib/foundry | /var/lib/foundry/* | /var/backups/foundry | /var/backups/foundry/*)
      fail "directory destination must be outside Foundry state and local backup directories"
      ;;
  esac

  printf '%s\n' "$destination_path"
}

probe_directory_write_contract() {
  local account="$1"
  local destination_path="$2"
  local account_uid=""
  local probe_program=""

  account_uid="$(id -u "$account")" ||
    fail "cannot resolve service account for directory destination probe: $account"
  # shellcheck disable=SC2016 # This is a literal program executed by bash -c.
  probe_program='
set -euo pipefail
directory="$1"
probe_file=""
probe_link=""
cleanup() {
  if [[ -n "$probe_link" ]]; then
    rm -f -- "$probe_link"
  fi
  if [[ -n "$probe_file" ]]; then
    rm -f -- "$probe_file"
  fi
}
trap cleanup EXIT
probe_file="$(mktemp "$directory/.foundry-install-write-probe.XXXXXX")"
probe_link="$probe_file.link"
printf "foundry-directory-write-probe\n" >"$probe_file"
sync -f -- "$probe_file"
ln -- "$probe_file" "$probe_link"
[[ "$probe_file" -ef "$probe_link" ]]
sync -f -- "$probe_link"
rm -f -- "$probe_link" "$probe_file"
probe_link=""
probe_file=""
sync -f -- "$directory"
trap - EXIT
'

  if [[ "$account_uid" -eq "$EUID" ]]; then
    bash -c "$probe_program" foundry-directory-write-probe "$destination_path" ||
      fail "directory destination does not satisfy the write, hard-link, and durable-sync contract for user $account: $destination_path"
  else
    runuser --user "$account" -- \
      bash -c "$probe_program" foundry-directory-write-probe "$destination_path" ||
      fail "directory destination does not satisfy the write, hard-link, and durable-sync contract for user $account: $destination_path"
  fi
}

read_mount_field() {
  local destination_path="$1"
  local field="$2"
  local -a field_lines=()

  mapfile -t field_lines < <(
    findmnt \
      --noheadings \
      --raw \
      --first-only \
      --output "$field" \
      --target "$destination_path"
  )
  [[ ${#field_lines[@]} -eq 1 && -n "${field_lines[0]}" ]] ||
    fail "cannot resolve mount $field for directory destination: $destination_path"
  [[ "${field_lines[0]}" != *$'\r'* ]] ||
    fail "mount $field contains a carriage return: $destination_path"

  printf '%s' "${field_lines[0]}"
}

capture_directory_mount_identity() {
  local destination_path="$1"
  local mount_target=""
  local mount_source=""
  local mount_fstype=""
  local mount_fsid=""
  local confirmed_target=""
  local confirmed_source=""
  local confirmed_fstype=""
  local confirmed_fsid=""

  command -v findmnt >/dev/null 2>&1 ||
    fail "required command not found for file destination: findmnt"
  mount_target="$(read_mount_field "$destination_path" TARGET)"
  mount_source="$(read_mount_field "$destination_path" SOURCE)"
  mount_fstype="$(read_mount_field "$destination_path" FSTYPE)"
  mount_fsid="$(stat -f -c '%i' -- "$destination_path")" ||
    fail "cannot resolve filesystem identity for directory destination: $destination_path"

  [[ "$mount_target" = /* && "$mount_target" != "/" ]] ||
    fail "directory destination must be backed by a dedicated mount, not the host root filesystem: $destination_path"
  [[ "$mount_target" =~ ^/[A-Za-z0-9._/@:+,-]+$ &&
    "$mount_target" != *"//"* &&
    "$mount_target" != *"/./"* &&
    "$mount_target" != *"/../"* &&
    "$mount_target" != */"." &&
    "$mount_target" != */".." &&
    "$mount_target" != */ ]] ||
    fail "directory destination mount target must be a canonical absolute path"
  case "$destination_path" in
    "$mount_target"/*) ;;
    *)
      fail "directory destination must be a subdirectory of its resolved mount target: $mount_target"
      ;;
  esac
  [[ "$mount_source" != *$'\n'* &&
    "$mount_source" != *$'\r'* &&
    -n "$mount_source" ]] ||
    fail "directory destination mount source is invalid"
  [[ "$mount_fstype" =~ ^[A-Za-z0-9._+-]+$ ]] ||
    fail "directory destination mount filesystem type is invalid"
  [[ "$mount_fsid" =~ ^[0-9A-Fa-f]+$ ]] ||
    fail "directory destination filesystem identity is invalid"

  confirmed_target="$(read_mount_field "$destination_path" TARGET)"
  confirmed_source="$(read_mount_field "$destination_path" SOURCE)"
  confirmed_fstype="$(read_mount_field "$destination_path" FSTYPE)"
  confirmed_fsid="$(stat -f -c '%i' -- "$destination_path")" ||
    fail "cannot confirm filesystem identity for directory destination: $destination_path"
  [[ "$mount_target" = "$confirmed_target" &&
    "$mount_source" = "$confirmed_source" &&
    "$mount_fstype" = "$confirmed_fstype" &&
    "$mount_fsid" = "$confirmed_fsid" ]] ||
    fail "directory destination mount changed while its identity was captured"

  printf '%s\n' \
    "foundry-directory-mount-v1" \
    "target=$mount_target" \
    "source=$mount_source" \
    "fstype=$mount_fstype" \
    "fsid=$mount_fsid"
}

render_backup_directory_dropin() {
  local destination_path="$1"

  printf \
    '[Unit]\nRequiresMountsFor=%s\n\n[Service]\nReadWritePaths=%s\n' \
    "$destination_path" \
    "$destination_path"
}

reconcile_backup_directory_dropin() {
  local destination_file="$1"
  local destination_path="$2"

  if [[ -z "$destination_path" ]]; then
    rm -f -- "$destination_file"
    return 0
  fi

  staged_backup_dropin="$(mktemp "$destination_file.XXXXXX")"
  render_backup_directory_dropin "$destination_path" >"$staged_backup_dropin"
  chmod 0644 "$staged_backup_dropin"
  if [[ "$EUID" -eq 0 ]]; then
    chown root:root "$staged_backup_dropin"
  fi
  mv "$staged_backup_dropin" "$destination_file"
  staged_backup_dropin=""
}

if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
  return 0
fi

[[ "$EUID" -eq 0 ]] || fail "run as root"
[[ $# -eq 4 ]] ||
  fail "usage: $0 /absolute/path/to/verified/foundry /absolute/path/to/offhost-uploader NON_SECRET_OFFHOST_DESTINATION OFFHOST_GENERATION"

binary="$1"
offhost_command="$2"
offhost_destination="$3"
offhost_generation="$4"
[[ "$binary" = /* ]] || fail "binary path must be absolute"
[[ -f "$binary" && -x "$binary" ]] || fail "binary is not an executable file: $binary"

[[ "$offhost_command" = /* ]] ||
  fail "off-host uploader path must be absolute"
[[ -f "$offhost_command" && -x "$offhost_command" ]] ||
  fail "off-host uploader is not an executable file: $offhost_command"
[[ ! -L "$offhost_command" ]] ||
  fail "off-host uploader must not be a symbolic link: $offhost_command"
case "$offhost_command" in
  /var/lib/foundry/* | /var/backups/foundry/*)
    fail "off-host uploader must be outside Foundry writable directories"
    ;;
esac
[[ -n "$offhost_destination" ]] ||
  fail "off-host destination must not be empty"
[[ "$offhost_destination" != *$'\n'* && "$offhost_destination" != *$'\r'* ]] ||
  fail "off-host destination must be a single line"
[[ "$offhost_generation" =~ ^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$ ]] ||
  fail "off-host generation must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}"

for required_command in bash chmod chown cp curl date env find getent grep groupadd id install ln mktemp mv nologin openssl realpath rm runuser sha256sum sleep stat sync systemctl tr useradd wc; do
  command -v "$required_command" >/dev/null 2>&1 ||
    fail "required command not found: $required_command"
done

audit_root_owned_path() {
  local candidate="$1"
  local canonical_candidate=""
  local component=""
  local component_mode=""

  canonical_candidate="$(realpath -e -- "$candidate")" ||
    fail "cannot resolve path: $candidate"
  [[ "$candidate" = "$canonical_candidate" ]] ||
    fail "path must be canonical and contain no symlink: $candidate"

  component="$canonical_candidate"
  while true; do
    [[ ! -L "$component" ]] ||
      fail "path component must not be a symlink: $component"
    [[ "$(stat -c '%u' -- "$component")" -eq 0 ]] ||
      fail "path component must be owned by root: $component"
    component_mode="$(stat -c '%a' -- "$component")"
    (( (8#$component_mode & 0022) == 0 )) ||
      fail "path component must not be writable by group or other: $component"

    [[ "$component" = "/" ]] && break
    component="${component%/*}"
    [[ -n "$component" ]] || component="/"
  done
}

audit_root_owned_path "$offhost_command"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if ! getent group foundry >/dev/null; then
  groupadd --system foundry
fi
if ! id -u foundry >/dev/null 2>&1; then
  useradd \
    --system \
    --gid foundry \
    --home-dir /var/lib/foundry \
    --shell "$(command -v nologin)" \
    foundry
fi

directory_offhost_path="$(
  resolve_directory_offhost_path "$offhost_destination" foundry
)"
directory_mount_identity=""

if [[ -n "$directory_offhost_path" ]]; then
  probe_directory_write_contract foundry "$directory_offhost_path"
  directory_mount_identity="$(
    capture_directory_mount_identity "$directory_offhost_path"
  )"
fi

install -d -m 0700 -o foundry -g foundry /var/lib/foundry
install -d -m 0700 -o foundry -g foundry /var/lib/foundry/workspace
install -d -m 0700 -o foundry -g foundry /var/backups/foundry
install -d -m 0750 -o root -g foundry /etc/foundry
install -d -m 0755 -o root -g root /usr/local/libexec

directory_offhost_dropin="/etc/systemd/system/foundry-backup.service.d/20-directory-offhost.conf"
managed_paths=(
  /etc/foundry/secret.key
  /etc/foundry/ops-token
  /etc/foundry/backup-offhost-command
  /etc/foundry/backup-offhost-destination
  /etc/foundry/backup-offhost-generation
  /etc/foundry/backup-offhost-mount-identity
  /usr/local/libexec/foundry-backup
  /usr/local/sbin/foundry-admin
  /usr/local/sbin/foundry-restore-drill
  /etc/systemd/system/foundry-ops.service
  /etc/systemd/system/foundry-runtime.service
  /etc/systemd/system/foundry-request-supervisor.service
  /etc/systemd/system/foundry-backup.service
  "$directory_offhost_dropin"
  /etc/systemd/system/foundry-backup.timer
  /usr/local/bin/foundry
)
declare -a managed_path_had_previous=()
rollback_root=""
transaction_active=false
transaction_committed=false
staged_secret=""
staged_token=""
staged_backup_config=""
staged_backup_dropin=""
staged_binary=""
ops_probe_config=""

read_unit_enable_state() {
  local unit="$1"
  local state=""

  state="$(systemctl is-enabled "$unit" 2>/dev/null || true)"
  [[ -n "$state" ]] || state="not-found"
  printf '%s\n' "$state"
}

foundry_ops_enable_state="$(read_unit_enable_state foundry-ops.service)"
foundry_runtime_enable_state="$(read_unit_enable_state foundry-runtime.service)"
foundry_request_supervisor_enable_state="$(read_unit_enable_state foundry-request-supervisor.service)"
foundry_backup_enable_state="$(read_unit_enable_state foundry-backup.service)"
foundry_timer_enable_state="$(read_unit_enable_state foundry-backup.timer)"
foundry_ops_was_active=false
foundry_runtime_was_active=false
foundry_request_supervisor_was_active=false
foundry_backup_was_active=false
foundry_timer_was_active=false
if systemctl is-active --quiet foundry-ops.service; then
  foundry_ops_was_active=true
fi
if systemctl is-active --quiet foundry-runtime.service; then
  foundry_runtime_was_active=true
fi
if systemctl is-active --quiet foundry-request-supervisor.service; then
  foundry_request_supervisor_was_active=true
fi
if systemctl is-active --quiet foundry-backup.service; then
  foundry_backup_was_active=true
fi
if systemctl is-active --quiet foundry-backup.timer; then
  foundry_timer_was_active=true
fi

cleanup_transaction_artifacts() {
  local staged_path=""

  for staged_path in \
    "$staged_secret" \
    "$staged_token" \
    "$staged_backup_config" \
    "$staged_backup_dropin" \
    "$staged_backup_mount_identity" \
    "$staged_binary" \
    "$ops_probe_config"; do
    if [[ -n "$staged_path" ]]; then
      rm -f -- "$staged_path"
    fi
  done
  if [[ -n "$rollback_root" &&
    "$rollback_root" = /var/tmp/foundry-install-rollback.* &&
    -d "$rollback_root" && ! -L "$rollback_root" ]]; then
    rm -rf -- "$rollback_root"
  fi
}

restore_unit_enable_state() {
  local unit="$1"
  local state="$2"

  case "$state" in
    enabled)
      systemctl enable "$unit"
      ;;
    enabled-runtime)
      systemctl enable --runtime "$unit"
      ;;
    disabled)
      systemctl disable "$unit"
      ;;
    masked)
      systemctl mask "$unit"
      ;;
    masked-runtime)
      systemctl mask --runtime "$unit"
      ;;
    not-found | static | indirect | generated | transient | linked | linked-runtime | alias)
      return 0
      ;;
    *)
      echo "foundry systemd installer: cannot restore unknown enable state '$state' for $unit" >&2
      return 1
      ;;
  esac
}

rollback_installation() {
  local index=0
  local path=""
  local rollback_failed=false

  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service \
    >/dev/null 2>&1 || true
  systemctl stop foundry-backup.service \
    >/dev/null 2>&1 || true

  for index in "${!managed_paths[@]}"; do
    path="${managed_paths[$index]}"
    if ! rm -f -- "$path"; then
      echo "foundry systemd installer: rollback could not remove $path" >&2
      rollback_failed=true
      continue
    fi
    if [[ "${managed_path_had_previous[$index]}" = 1 ]] &&
      ! cp -a -- "$rollback_root/$index" "$path"; then
      echo "foundry systemd installer: rollback could not restore $path" >&2
      rollback_failed=true
    fi
  done

  systemctl daemon-reload >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    foundry-ops.service "$foundry_ops_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    foundry-runtime.service "$foundry_runtime_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    foundry-request-supervisor.service "$foundry_request_supervisor_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    foundry-backup.service "$foundry_backup_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    foundry-backup.timer "$foundry_timer_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true

  if [[ "$foundry_ops_was_active" = true ]] &&
    ! systemctl start foundry-ops.service >/dev/null 2>&1; then
    rollback_failed=true
  fi
  if [[ "$foundry_runtime_was_active" = true ]] &&
    ! systemctl start foundry-runtime.service >/dev/null 2>&1; then
    rollback_failed=true
  fi
  if [[ "$foundry_request_supervisor_was_active" = true ]] &&
    ! systemctl start foundry-request-supervisor.service >/dev/null 2>&1; then
    rollback_failed=true
  fi
  if [[ "$foundry_backup_was_active" = true ]] &&
    ! systemctl start foundry-backup.service >/dev/null 2>&1; then
    rollback_failed=true
  fi
  if [[ "$foundry_timer_was_active" = true ]] &&
    ! systemctl start foundry-backup.timer >/dev/null 2>&1; then
    rollback_failed=true
  fi

  if [[ "$rollback_failed" = true ]]; then
    echo "foundry systemd installer: transactional rollback was incomplete; keep services isolated and repair manually" >&2
    return 1
  fi
  echo "foundry systemd installer: restored previous files and systemd state" >&2
}

handle_transaction_exit() {
  local status=$?

  trap - EXIT
  if [[ "$transaction_active" = true &&
    "$transaction_committed" = false ]]; then
    if ! rollback_installation; then
      status=1
    fi
  fi
  cleanup_transaction_artifacts
  exit "$status"
}
trap handle_transaction_exit EXIT

rollback_root="$(mktemp -d /var/tmp/foundry-install-rollback.XXXXXX)"
chmod 0700 "$rollback_root"
chown root:root "$rollback_root"
for index in "${!managed_paths[@]}"; do
  path="${managed_paths[$index]}"
  if [[ -e "$path" || -L "$path" ]]; then
    cp -a -- "$path" "$rollback_root/$index"
    managed_path_had_previous[index]=1
  else
    managed_path_had_previous[index]=0
  fi
done
transaction_active=true

secret_key="/etc/foundry/secret.key"
if [[ ! -e "$secret_key" ]]; then
  staged_secret="$(mktemp /etc/foundry/.secret.key.XXXXXX)"
  openssl rand -hex 32 >"$staged_secret"
  chmod 0600 "$staged_secret"
  chown root:root "$staged_secret"
  mv "$staged_secret" "$secret_key"
  staged_secret=""
fi
[[ -f "$secret_key" ]] || fail "secret key is not a regular file: $secret_key"
secret_key_value="$(tr -d '\r\n' <"$secret_key")"
[[ ${#secret_key_value} -eq 64 && "$secret_key_value" != *[!0-9A-Fa-f]* ]] ||
  fail "$secret_key must contain exactly 64 hexadecimal characters"
chmod 0600 "$secret_key"
chown root:root "$secret_key"

ops_token="/etc/foundry/ops-token"
if [[ ! -e "$ops_token" ]]; then
  staged_token="$(mktemp /etc/foundry/.ops-token.XXXXXX)"
  openssl rand -hex 32 >"$staged_token"
  chmod 0600 "$staged_token"
  chown root:root "$staged_token"
  mv "$staged_token" "$ops_token"
  staged_token=""
fi
[[ -f "$ops_token" ]] || fail "Ops token is not a regular file: $ops_token"
ops_token_value="$(tr -d '\r\n' <"$ops_token")"
[[ ${#ops_token_value} -ge 32 && ${#ops_token_value} -le 4096 ]] ||
  fail "$ops_token must contain between 32 and 4096 visible ASCII bytes"
[[ "$ops_token_value" != *[![:graph:]]* ]] ||
  fail "$ops_token contains non-visible or non-ASCII bytes"
chmod 0600 "$ops_token"
chown root:root "$ops_token"

cleanup_ops_probe_config() {
  if [[ -n "$ops_probe_config" ]]; then
    rm -f -- "$ops_probe_config"
  fi
}

wait_for_ops_ready() {
  local deadline="$((SECONDS + 30))"
  local response_code=""

  while ((SECONDS < deadline)); do
    if systemctl is-failed --quiet foundry-ops.service; then
      return 1
    fi
    if systemctl is-active --quiet foundry-ops.service; then
      response_code="$(
        curl \
          --disable \
          --noproxy '*' \
          --proto '=http' \
          --config "$ops_probe_config" \
          --silent \
          --output /dev/null \
          --write-out '%{http_code}' \
          --connect-timeout 1 \
          --max-time 2 \
          http://127.0.0.1:8765/api/snapshot ||
          true
      )"
      if [[ "$response_code" = 200 ]]; then
        return 0
      fi
    fi
    sleep 1
  done

  return 1
}

wait_for_runtime_ready() {
  local deadline="$((SECONDS + 30))"
  local stable_checks=0

  while ((SECONDS < deadline)); do
    if systemctl is-failed --quiet foundry-runtime.service; then
      return 1
    fi
    if systemctl is-active --quiet foundry-runtime.service; then
      stable_checks="$((stable_checks + 1))"
      if ((stable_checks >= 3)); then
        return 0
      fi
    else
      stable_checks=0
    fi
    sleep 1
  done

  return 1
}

wait_for_request_supervisor_ready() {
  local deadline="$((SECONDS + 30))"
  local stable_checks=0

  while ((SECONDS < deadline)); do
    if systemctl is-failed --quiet foundry-request-supervisor.service; then
      return 1
    fi
    if systemctl is-active --quiet foundry-request-supervisor.service; then
      stable_checks="$((stable_checks + 1))"
      if ((stable_checks >= 3)); then
        return 0
      fi
    else
      stable_checks=0
    fi
    sleep 1
  done

  return 1
}

install_backup_config() {
  local destination="$1"
  local value="$2"

  staged_backup_config="$(mktemp /etc/foundry/.backup-config.XXXXXX)"
  printf '%s\n' "$value" >"$staged_backup_config"
  chmod 0600 "$staged_backup_config"
  chown root:root "$staged_backup_config"
  mv "$staged_backup_config" "$destination"
  staged_backup_config=""
}

install_backup_mount_identity() {
  local destination="$1"
  local value="$2"

  staged_backup_mount_identity="$(
    mktemp /etc/foundry/.backup-mount-identity.XXXXXX
  )"
  printf '%s\n' "$value" >"$staged_backup_mount_identity"
  chmod 0644 "$staged_backup_mount_identity"
  chown root:root "$staged_backup_mount_identity"
  mv "$staged_backup_mount_identity" "$destination"
  staged_backup_mount_identity=""
}

if [[ "$foundry_timer_was_active" = true ]]; then
  systemctl stop foundry-backup.timer
fi
if [[ "$foundry_ops_was_active" = true ]]; then
  systemctl stop foundry-ops.service
fi
if [[ "$foundry_runtime_was_active" = true ]]; then
  systemctl stop foundry-runtime.service
fi
if systemctl is-active --quiet foundry-request-supervisor.service; then
  systemctl stop foundry-request-supervisor.service
fi
if [[ "$foundry_backup_was_active" = true ]]; then
  systemctl stop foundry-backup.service
fi

if [[ -n "$directory_offhost_path" ]]; then
  install -d -m 0755 -o root -g root \
    /etc/systemd/system/foundry-backup.service.d
fi
reconcile_backup_directory_dropin \
  "$directory_offhost_dropin" \
  "$directory_offhost_path"

install_backup_config /etc/foundry/backup-offhost-command "$offhost_command"
install_backup_config /etc/foundry/backup-offhost-destination "$offhost_destination"
install_backup_config /etc/foundry/backup-offhost-generation "$offhost_generation"
if [[ -n "$directory_mount_identity" ]]; then
  install_backup_mount_identity \
    /etc/foundry/backup-offhost-mount-identity \
    "$directory_mount_identity"
else
  rm -f -- /etc/foundry/backup-offhost-mount-identity
fi

install -m 0755 -o root -g root "$script_dir/foundry-backup" /usr/local/libexec/foundry-backup
install -m 0755 -o root -g root "$script_dir/foundry-admin" /usr/local/sbin/foundry-admin
install -m 0755 -o root -g root "$script_dir/foundry-restore-drill" /usr/local/sbin/foundry-restore-drill
install -m 0644 -o root -g root "$script_dir/foundry-ops.service" /etc/systemd/system/foundry-ops.service
install -m 0644 -o root -g root "$script_dir/foundry-runtime.service" /etc/systemd/system/foundry-runtime.service
install -m 0644 -o root -g root "$script_dir/foundry-request-supervisor.service" /etc/systemd/system/foundry-request-supervisor.service
install -m 0644 -o root -g root "$script_dir/foundry-backup.service" /etc/systemd/system/foundry-backup.service
install -m 0644 -o root -g root "$script_dir/foundry-backup.timer" /etc/systemd/system/foundry-backup.timer

staged_binary="$(mktemp /usr/local/bin/.foundry.XXXXXX)"
install -m 0755 -o root -g root "$binary" "$staged_binary"
mv "$staged_binary" /usr/local/bin/foundry
staged_binary=""

systemctl daemon-reload
systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service

ops_probe_config="$(mktemp /etc/foundry/.ops-probe.XXXXXX)"
ops_token_for_curl="${ops_token_value//\\/\\\\}"
ops_token_for_curl="${ops_token_for_curl//\"/\\\"}"
printf 'header = "Authorization: Bearer %s"\n' \
  "$ops_token_for_curl" >"$ops_probe_config"
chmod 0600 "$ops_probe_config"
chown root:root "$ops_probe_config"
unset ops_token_for_curl ops_token_value

if ! systemctl start foundry-ops.service; then
  fail "Foundry Ops failed during store initialization; services remain disabled"
fi

store_ready=false
if wait_for_ops_ready && [[ -f /var/lib/foundry/foundry.sqlite ]]; then
  store_ready=true
fi
systemctl stop foundry-ops.service

[[ "$store_ready" = true ]] ||
  fail "Foundry store or authenticated Ops snapshot did not become ready; Ops, runtime, request supervisor and backup timer remain disabled"

if ! systemctl start foundry-backup.service; then
  systemctl stop foundry-ops.service
  fail "initial off-host recovery challenge failed; Ops, runtime, request supervisor and backup timer remain disabled"
fi

systemctl enable foundry-ops.service foundry-runtime.service foundry-request-supervisor.service foundry-backup.timer
if ! systemctl start foundry-ops.service; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "Foundry Ops failed after backup promotion; services were disabled again"
fi
if ! wait_for_ops_ready; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "authenticated Foundry Ops snapshot failed after backup promotion; services were disabled again"
fi
if ! systemctl start foundry-runtime.service; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "Foundry runtime failed after backup promotion; services were disabled again"
fi
if ! wait_for_runtime_ready; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "Foundry runtime did not remain active after startup; services were disabled again"
fi
if ! systemctl start foundry-request-supervisor.service; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "Foundry request supervisor failed after backup promotion; services were disabled again"
fi
if ! wait_for_request_supervisor_ready; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "Foundry request supervisor did not remain active after startup; services were disabled again"
fi
if ! systemctl start foundry-backup.timer; then
  systemctl disable --now foundry-request-supervisor.service foundry-backup.timer foundry-runtime.service foundry-ops.service
  fail "backup timer failed after promotion; Ops, runtime, request supervisor and timer were disabled again"
fi
cleanup_ops_probe_config
ops_probe_config=""
systemctl --no-pager --full status foundry-ops.service
systemctl --no-pager --full status foundry-runtime.service
systemctl --no-pager --full status foundry-request-supervisor.service
systemctl --no-pager --full status foundry-backup.timer
transaction_committed=true
cleanup_transaction_artifacts
transaction_active=false
trap - EXIT
