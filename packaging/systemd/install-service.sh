#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "forge systemd installer: $*" >&2
  exit 1
}

[[ "$EUID" -eq 0 ]] || fail "run as root"
[[ $# -eq 4 ]] ||
  fail "usage: $0 /absolute/path/to/verified/forge /absolute/path/to/offhost-uploader NON_SECRET_OFFHOST_DESTINATION OFFHOST_GENERATION"

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
  /var/lib/forge/* | /var/backups/forge/*)
    fail "off-host uploader must be outside Forge writable directories"
    ;;
esac
[[ -n "$offhost_destination" ]] ||
  fail "off-host destination must not be empty"
[[ "$offhost_destination" != *$'\n'* && "$offhost_destination" != *$'\r'* ]] ||
  fail "off-host destination must be a single line"
[[ "$offhost_generation" =~ ^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$ ]] ||
  fail "off-host generation must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}"

for required_command in chmod chown cp curl date env find getent grep groupadd id install mktemp mv nologin openssl realpath rm sha256sum sleep stat systemctl tr useradd wc; do
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

if ! getent group forge >/dev/null; then
  groupadd --system forge
fi
if ! id -u forge >/dev/null 2>&1; then
  useradd \
    --system \
    --gid forge \
    --home-dir /var/lib/forge \
    --shell "$(command -v nologin)" \
    forge
fi

install -d -m 0700 -o forge -g forge /var/lib/forge
install -d -m 0700 -o forge -g forge /var/lib/forge/workspace
install -d -m 0700 -o forge -g forge /var/backups/forge
install -d -m 0750 -o root -g forge /etc/forge
install -d -m 0755 -o root -g root /usr/local/libexec

managed_paths=(
  /etc/forge/secret.key
  /etc/forge/ops-token
  /etc/forge/backup-offhost-command
  /etc/forge/backup-offhost-destination
  /etc/forge/backup-offhost-generation
  /usr/local/libexec/forge-backup
  /usr/local/sbin/forge-admin
  /usr/local/sbin/forge-restore-drill
  /etc/systemd/system/forge-ops.service
  /etc/systemd/system/forge-backup.service
  /etc/systemd/system/forge-backup.timer
  /usr/local/bin/forge
)
declare -a managed_path_had_previous=()
rollback_root=""
transaction_active=false
transaction_committed=false
staged_secret=""
staged_token=""
staged_backup_config=""
staged_binary=""
ops_probe_config=""

read_unit_enable_state() {
  local unit="$1"
  local state=""

  state="$(systemctl is-enabled "$unit" 2>/dev/null || true)"
  [[ -n "$state" ]] || state="not-found"
  printf '%s\n' "$state"
}

forge_ops_enable_state="$(read_unit_enable_state forge-ops.service)"
forge_backup_enable_state="$(read_unit_enable_state forge-backup.service)"
forge_timer_enable_state="$(read_unit_enable_state forge-backup.timer)"
forge_ops_was_active=false
forge_backup_was_active=false
forge_timer_was_active=false
if systemctl is-active --quiet forge-ops.service; then
  forge_ops_was_active=true
fi
if systemctl is-active --quiet forge-backup.service; then
  forge_backup_was_active=true
fi
if systemctl is-active --quiet forge-backup.timer; then
  forge_timer_was_active=true
fi

cleanup_transaction_artifacts() {
  local staged_path=""

  for staged_path in \
    "$staged_secret" \
    "$staged_token" \
    "$staged_backup_config" \
    "$staged_binary" \
    "$ops_probe_config"; do
    if [[ -n "$staged_path" ]]; then
      rm -f -- "$staged_path"
    fi
  done
  if [[ -n "$rollback_root" &&
    "$rollback_root" = /var/tmp/forge-install-rollback.* &&
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
      echo "forge systemd installer: cannot restore unknown enable state '$state' for $unit" >&2
      return 1
      ;;
  esac
}

rollback_installation() {
  local index=0
  local path=""
  local rollback_failed=false

  systemctl disable --now forge-backup.timer forge-ops.service \
    >/dev/null 2>&1 || true
  systemctl stop forge-backup.service \
    >/dev/null 2>&1 || true

  for index in "${!managed_paths[@]}"; do
    path="${managed_paths[$index]}"
    if ! rm -f -- "$path"; then
      echo "forge systemd installer: rollback could not remove $path" >&2
      rollback_failed=true
      continue
    fi
    if [[ "${managed_path_had_previous[$index]}" = 1 ]] &&
      ! cp -a -- "$rollback_root/$index" "$path"; then
      echo "forge systemd installer: rollback could not restore $path" >&2
      rollback_failed=true
    fi
  done

  systemctl daemon-reload >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    forge-ops.service "$forge_ops_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    forge-backup.service "$forge_backup_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true
  restore_unit_enable_state \
    forge-backup.timer "$forge_timer_enable_state" \
    >/dev/null 2>&1 || rollback_failed=true

  if [[ "$forge_ops_was_active" = true ]] &&
    ! systemctl start forge-ops.service >/dev/null 2>&1; then
    rollback_failed=true
  fi
  if [[ "$forge_backup_was_active" = true ]] &&
    ! systemctl start forge-backup.service >/dev/null 2>&1; then
    rollback_failed=true
  fi
  if [[ "$forge_timer_was_active" = true ]] &&
    ! systemctl start forge-backup.timer >/dev/null 2>&1; then
    rollback_failed=true
  fi

  if [[ "$rollback_failed" = true ]]; then
    echo "forge systemd installer: transactional rollback was incomplete; keep services isolated and repair manually" >&2
    return 1
  fi
  echo "forge systemd installer: restored previous files and systemd state" >&2
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

rollback_root="$(mktemp -d /var/tmp/forge-install-rollback.XXXXXX)"
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

secret_key="/etc/forge/secret.key"
if [[ ! -e "$secret_key" ]]; then
  staged_secret="$(mktemp /etc/forge/.secret.key.XXXXXX)"
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

ops_token="/etc/forge/ops-token"
if [[ ! -e "$ops_token" ]]; then
  staged_token="$(mktemp /etc/forge/.ops-token.XXXXXX)"
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
    if systemctl is-failed --quiet forge-ops.service; then
      return 1
    fi
    if systemctl is-active --quiet forge-ops.service; then
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

install_backup_config() {
  local destination="$1"
  local value="$2"

  staged_backup_config="$(mktemp /etc/forge/.backup-config.XXXXXX)"
  printf '%s\n' "$value" >"$staged_backup_config"
  chmod 0600 "$staged_backup_config"
  chown root:root "$staged_backup_config"
  mv "$staged_backup_config" "$destination"
  staged_backup_config=""
}

if [[ "$forge_timer_was_active" = true ]]; then
  systemctl stop forge-backup.timer
fi
if [[ "$forge_ops_was_active" = true ]]; then
  systemctl stop forge-ops.service
fi
if [[ "$forge_backup_was_active" = true ]]; then
  systemctl stop forge-backup.service
fi

install_backup_config /etc/forge/backup-offhost-command "$offhost_command"
install_backup_config /etc/forge/backup-offhost-destination "$offhost_destination"
install_backup_config /etc/forge/backup-offhost-generation "$offhost_generation"

install -m 0755 -o root -g root "$script_dir/forge-backup" /usr/local/libexec/forge-backup
install -m 0755 -o root -g root "$script_dir/forge-admin" /usr/local/sbin/forge-admin
install -m 0755 -o root -g root "$script_dir/forge-restore-drill" /usr/local/sbin/forge-restore-drill
install -m 0644 -o root -g root "$script_dir/forge-ops.service" /etc/systemd/system/forge-ops.service
install -m 0644 -o root -g root "$script_dir/forge-backup.service" /etc/systemd/system/forge-backup.service
install -m 0644 -o root -g root "$script_dir/forge-backup.timer" /etc/systemd/system/forge-backup.timer

staged_binary="$(mktemp /usr/local/bin/.forge.XXXXXX)"
install -m 0755 -o root -g root "$binary" "$staged_binary"
mv "$staged_binary" /usr/local/bin/forge
staged_binary=""

systemctl daemon-reload
systemctl disable --now forge-backup.timer forge-ops.service

ops_probe_config="$(mktemp /etc/forge/.ops-probe.XXXXXX)"
ops_token_for_curl="${ops_token_value//\\/\\\\}"
ops_token_for_curl="${ops_token_for_curl//\"/\\\"}"
printf 'header = "Authorization: Bearer %s"\n' \
  "$ops_token_for_curl" >"$ops_probe_config"
chmod 0600 "$ops_probe_config"
chown root:root "$ops_probe_config"
unset ops_token_for_curl ops_token_value

if ! systemctl start forge-ops.service; then
  fail "Forge Ops failed during store initialization; services remain disabled"
fi

store_ready=false
if wait_for_ops_ready && [[ -f /var/lib/forge/forge.sqlite ]]; then
  store_ready=true
fi
systemctl stop forge-ops.service

[[ "$store_ready" = true ]] ||
  fail "Forge store or authenticated Ops snapshot did not become ready; Ops and backup timer remain disabled"

if ! systemctl start forge-backup.service; then
  systemctl stop forge-ops.service
  fail "initial off-host recovery challenge failed; Ops and backup timer remain disabled"
fi

systemctl enable forge-ops.service forge-backup.timer
if ! systemctl start forge-ops.service; then
  systemctl disable --now forge-backup.timer forge-ops.service
  fail "Forge Ops failed after backup promotion; services were disabled again"
fi
if ! wait_for_ops_ready; then
  systemctl disable --now forge-backup.timer forge-ops.service
  fail "authenticated Forge Ops snapshot failed after backup promotion; services were disabled again"
fi
if ! systemctl start forge-backup.timer; then
  systemctl disable --now forge-backup.timer forge-ops.service
  fail "backup timer failed after promotion; Ops and timer were disabled again"
fi
cleanup_ops_probe_config
ops_probe_config=""
systemctl --no-pager --full status forge-ops.service
systemctl --no-pager --full status forge-backup.timer
transaction_committed=true
cleanup_transaction_artifacts
transaction_active=false
trap - EXIT
