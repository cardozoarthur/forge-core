#!/usr/bin/env bash
set -euo pipefail
umask 077

foundry_binary="${1:-target/release/foundry}"
smoke_port="${FOUNDRY_SMOKE_PORT:-18765}"
smoke_dir="$(mktemp -d)"
server_pid=""
server_start_count=0
last_mutated_goal=""

cleanup() {
  if [[ "$server_pid" =~ ^[0-9]+$ ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$smoke_dir"
}
trap cleanup EXIT

[[ -x "$foundry_binary" ]] || {
  echo "production smoke: executable not found: $foundry_binary" >&2
  exit 1
}
command -v curl >/dev/null 2>&1 || {
  echo "production smoke: curl is required" >&2
  exit 1
}
command -v openssl >/dev/null 2>&1 || {
  echo "production smoke: openssl is required" >&2
  exit 1
}
command -v stat >/dev/null 2>&1 || {
  echo "production smoke: GNU stat is required" >&2
  exit 1
}

store="$smoke_dir/foundry.sqlite"
backup="$smoke_dir/foundry-backup.sqlite"
vault_key_file="$smoke_dir/secret.key"
ops_token_file="$smoke_dir/ops-token"
curl_auth_config="$smoke_dir/curl-auth.conf"
curl_incorrect_auth_config="$smoke_dir/curl-incorrect-auth.conf"
openssl rand -hex 32 >"$vault_key_file"
openssl rand -hex 32 >"$ops_token_file"
ops_token="$(tr -d '\r\n' <"$ops_token_file")"
printf 'header = "Authorization: Bearer %s"\n' "$ops_token" >"$curl_auth_config"
printf 'header = "Authorization: Bearer %sincorrect"\n' "$ops_token" >"$curl_incorrect_auth_config"
unset ops_token
chmod 0600 \
  "$vault_key_file" \
  "$ops_token_file" \
  "$curl_auth_config" \
  "$curl_incorrect_auth_config"
for credential_file in \
  "$vault_key_file" \
  "$ops_token_file" \
  "$curl_auth_config" \
  "$curl_incorrect_auth_config"; do
  [[ "$(stat -c '%a' "$credential_file")" == "600" ]] || {
    echo "production smoke: credential file permissions are not 0600: $credential_file" >&2
    exit 1
  }
done
foundry_env=(
  env
  FOUNDRY_PRODUCTION_MODE=1
  "FOUNDRY_SECRET_VAULT_KEY_FILE=$vault_key_file"
  "FOUNDRY_OPS_BEARER_TOKEN_FILE=$ops_token_file"
)

"${foundry_env[@]}" "$foundry_binary" --store "$store" plan \
  --goal "Validate the Foundry v0.6 single-host production path" \
  --output json >"$smoke_dir/plan.json"
workflow_id=""
while IFS= read -r plan_line; do
  if [[ "$plan_line" =~ \"workflow_id\"[[:space:]]*:[[:space:]]*\"([^\"]+)\" ]]; then
    workflow_id="${BASH_REMATCH[1]}"
    break
  fi
done <"$smoke_dir/plan.json"
[[ "$workflow_id" == wf_* ]] || {
  echo "production smoke: plan did not return a valid workflow id" >&2
  exit 1
}
[[ "$(stat -c '%a' "$store")" == "600" ]] || {
  echo "production smoke: store permissions are not 0600" >&2
  exit 1
}
[[ ! -e "$store.secret.key" ]] || {
  echo "production smoke: external key configuration created an adjacent fallback key" >&2
  exit 1
}
"${foundry_env[@]}" "$foundry_binary" --store "$store" store check \
  --output json >"$smoke_dir/check-before.json"
"${foundry_env[@]}" "$foundry_binary" --store "$store" store backup \
  --destination "$backup" \
  --output json >"$smoke_dir/backup.json"
[[ -s "$backup" ]] || {
  echo "production smoke: backup was not created" >&2
  exit 1
}
"${foundry_env[@]}" "$foundry_binary" --store "$store" store restore \
  --source "$backup" \
  --approved-by production-smoke \
  --confirm-restore \
  --output json >"$smoke_dir/restore.json"
"${foundry_env[@]}" "$foundry_binary" --store "$store" store check \
  --output json >"$smoke_dir/check-after.json"

mkdir -p "$smoke_dir/workspace"
"$foundry_binary" events runtime-daemon --help \
  >"$smoke_dir/runtime-daemon-help.txt"
"$foundry_binary" request supervise --help \
  >"$smoke_dir/request-supervisor-help.txt"
grep -Fq -- '--continuous' "$smoke_dir/runtime-daemon-help.txt" || {
  echo "production smoke: runtime daemon help is missing continuous mode" >&2
  exit 1
}
grep -Fq -- '--max-steps-per-run' "$smoke_dir/request-supervisor-help.txt" || {
  echo "production smoke: request supervisor help is missing bounded steps" >&2
  exit 1
}

"${foundry_env[@]}" "$foundry_binary" --store "$store" events runtime-daemon \
  --project-root "$smoke_dir/workspace" \
  --execute \
  --dispatch-activations \
  --max-cycles 1 \
  --interval-seconds 0 \
  --recover-stale-services \
  --scan-schedules \
  --schedule-executor foundry-production-smoke-scheduler \
  --schedule-max-workers 1 \
  --schedule-ttl-seconds 30 \
  --output json \
  >"$smoke_dir/runtime-daemon.json"
grep -Eq '"schema_version"[[:space:]]*:[[:space:]]*"foundry.event_runtime_daemon.v1"' \
  "$smoke_dir/runtime-daemon.json" || {
  echo "production smoke: runtime daemon did not emit its contract" >&2
  exit 1
}

supervisor_store="$smoke_dir/request-supervisor.sqlite"
"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" request start \
  --goal "Validate request supervisor advancement; constraint: use only temporary local state; deliverable: persisted supervisor evidence" \
  --origin production-smoke \
  --output json \
  >"$smoke_dir/request-supervisor-start.json"
"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" request supervise \
  --executor foundry-request-supervisor \
  --ttl-seconds 30 \
  --max-steps-per-run 1 \
  --origin production-smoke \
  --continuous \
  --max-cycles 1 \
  --interval-seconds 1 \
  --output json \
  >"$smoke_dir/request-supervisor-advance.json"
grep -Eq '"needs_attention"[[:space:]]*:[[:space:]]*[1-9][0-9]*' \
  "$smoke_dir/request-supervisor-advance.json" || {
  echo "production smoke: request supervisor did not park a receipt-required run" >&2
  exit 1
}

"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" request start \
  --goal "Validate stale request recovery; constraint: use only temporary local state; deliverable: persisted recovery evidence" \
  --origin production-smoke \
  --output json \
  >"$smoke_dir/request-supervisor-stale-start.json"
stale_run_id=""
while IFS= read -r request_line; do
  if [[ "$request_line" =~ \"run_id\"[[:space:]]*:[[:space:]]*\"([^\"]+)\" ]]; then
    stale_run_id="${BASH_REMATCH[1]}"
    break
  fi
done <"$smoke_dir/request-supervisor-stale-start.json"
[[ "$stale_run_id" == run_* ]] || {
  echo "production smoke: stale recovery fixture did not return a run id" >&2
  exit 1
}
"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" request heartbeat \
  --run "$stale_run_id" \
  --executor foundry-request-supervisor \
  --summary "production smoke stale heartbeat" \
  --ttl-seconds 1 \
  --origin production-smoke \
  --output json \
  >"$smoke_dir/request-supervisor-stale-heartbeat.json"
sleep 2
"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" request supervise \
  --executor foundry-request-supervisor \
  --ttl-seconds 30 \
  --max-steps-per-run 1 \
  --origin production-smoke \
  --output json \
  >"$smoke_dir/request-supervisor-recover.json"
grep -Eq '"recovered"[[:space:]]*:[[:space:]]*[1-9][0-9]*' \
  "$smoke_dir/request-supervisor-recover.json" || {
  echo "production smoke: request supervisor did not recover a stale owned run" >&2
  exit 1
}
"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" request status \
  --run "$stale_run_id" \
  --output json \
  >"$smoke_dir/request-supervisor-recovered-status.json"
grep -Eq '"status"[[:space:]]*:[[:space:]]*"needs_attention"' \
  "$smoke_dir/request-supervisor-recovered-status.json" || {
  echo "production smoke: stale run was not parked in needs_attention" >&2
  exit 1
}
"${foundry_env[@]}" "$foundry_binary" --store "$supervisor_store" store check \
  --output json >"$smoke_dir/request-supervisor-store-check.json"

start_server() {
  server_start_count=$((server_start_count + 1))
  local start_id="$server_start_count"
  local ops_origin="http://127.0.0.1:$smoke_port"
  local mutated_goal="production-smoke-authenticated-mutation-$start_id"
  local expected_persisted_goal="$last_mutated_goal"
  local readiness_status=""
  local unauthenticated_read_status
  local incorrect_read_token_status
  local server_cmdline
  local unauthenticated_status
  local incorrect_token_status
  local pre_mutation_status
  local authenticated_mutation_status
  local snapshot_status
  local mutation_response
  local startup_snapshot
  local pre_mutation_snapshot
  local post_mutation_snapshot

  "${foundry_env[@]}" "$foundry_binary" --store "$store" ops serve \
    --project-root "$smoke_dir/workspace" \
    --host 127.0.0.1 \
    --port "$smoke_port" \
    >"$smoke_dir/ops.log" 2>&1 &
  server_pid="$!"

  for _ in {1..40}; do
    readiness_status="$(
      curl --disable \
        --config "$curl_auth_config" \
        --silent \
        --output "$smoke_dir/snapshot-at-startup-$start_id.json" \
        --write-out '%{http_code}' \
        "http://127.0.0.1:$smoke_port/api/snapshot" ||
        true
    )"
    if [[ "$readiness_status" == "200" ]]; then
      break
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      echo "production smoke: Ops service exited before becoming ready" >&2
      return 1
    fi
    sleep 0.25
  done
  [[ "$readiness_status" == "200" ]] || {
    echo "production smoke: Ops service did not become ready" >&2
    return 1
  }

  server_cmdline="$(tr '\0' ' ' <"/proc/$server_pid/cmdline")"
  [[ "$server_cmdline" != *"$ops_token_file"* ]] || {
    echo "production smoke: Ops bearer token file leaked into the server argv" >&2
    return 1
  }
  [[ "$server_cmdline" != *"FOUNDRY_OPS_BEARER_TOKEN"* ]] || {
    echo "production smoke: Ops bearer credential leaked into the server argv" >&2
    return 1
  }

  unauthenticated_read_status="$(
    curl --disable \
      --silent --show-error \
      --output "$smoke_dir/unauthenticated-snapshot-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/snapshot" ||
      true
  )"
  [[ "$unauthenticated_read_status" == "401" ]] || {
    echo "production smoke: unauthenticated snapshot returned $unauthenticated_read_status instead of 401" >&2
    return 1
  }

  incorrect_read_token_status="$(
    curl --disable \
      --config "$curl_incorrect_auth_config" \
      --silent --show-error \
      --output "$smoke_dir/incorrect-token-snapshot-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/snapshot" ||
      true
  )"
  [[ "$incorrect_read_token_status" == "401" ]] || {
    echo "production smoke: incorrect bearer snapshot returned $incorrect_read_token_status instead of 401" >&2
    return 1
  }

  startup_snapshot="$(<"$smoke_dir/snapshot-at-startup-$start_id.json")"
  if [[ -n "$expected_persisted_goal" ]]; then
    [[ "$startup_snapshot" == *"$expected_persisted_goal"* ]] || {
      echo "production smoke: mutation from the previous boot did not survive SIGKILL and restart" >&2
      return 1
    }
  fi
  [[ "$startup_snapshot" != *"$mutated_goal"* ]] || {
    echo "production smoke: next mutation was already present at server startup" >&2
    return 1
  }

  unauthenticated_status="$(
    curl --disable \
      --silent --show-error \
      --request POST \
      --header "Content-Type: application/x-www-form-urlencoded" \
      --header "Origin: $ops_origin" \
      --header "Sec-Fetch-Site: same-origin" \
      --data-urlencode "workflow_id=$workflow_id" \
      --data-urlencode "goal=$mutated_goal" \
      --output "$smoke_dir/unauthenticated-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/workflow/update-goal" ||
      true
  )"
  [[ "$unauthenticated_status" == "401" ]] || {
    echo "production smoke: missing bearer token returned $unauthenticated_status instead of 401" >&2
    return 1
  }

  incorrect_token_status="$(
    curl --disable \
      --config "$curl_incorrect_auth_config" \
      --silent --show-error \
      --request POST \
      --header "Content-Type: application/x-www-form-urlencoded" \
      --header "Origin: $ops_origin" \
      --header "Sec-Fetch-Site: same-origin" \
      --data-urlencode "workflow_id=$workflow_id" \
      --data-urlencode "goal=$mutated_goal" \
      --output "$smoke_dir/incorrect-token-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/workflow/update-goal" ||
      true
  )"
  [[ "$incorrect_token_status" == "401" ]] || {
    echo "production smoke: incorrect bearer token returned $incorrect_token_status instead of 401" >&2
    return 1
  }

  pre_mutation_status="$(
    curl --disable \
      --config "$curl_auth_config" \
      --silent --show-error \
      --output "$smoke_dir/snapshot-before-mutation-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/snapshot" ||
      true
  )"
  [[ "$pre_mutation_status" == "200" ]] || {
    echo "production smoke: authenticated pre-mutation snapshot returned $pre_mutation_status" >&2
    return 1
  }
  pre_mutation_snapshot="$(<"$smoke_dir/snapshot-before-mutation-$start_id.json")"
  [[ "$pre_mutation_snapshot" != *"$mutated_goal"* ]] || {
    echo "production smoke: rejected mutation unexpectedly changed the workflow" >&2
    return 1
  }
  if [[ -n "$expected_persisted_goal" ]]; then
    [[ "$pre_mutation_snapshot" == *"$expected_persisted_goal"* ]] || {
      echo "production smoke: rejected request changed the goal persisted across restart" >&2
      return 1
    }
  fi

  authenticated_mutation_status="$(
    curl --disable \
      --config "$curl_auth_config" \
      --silent --show-error \
      --request POST \
      --header "Content-Type: application/x-www-form-urlencoded" \
      --header "Origin: $ops_origin" \
      --header "Sec-Fetch-Site: same-origin" \
      --data-urlencode "workflow_id=$workflow_id" \
      --data-urlencode "goal=$mutated_goal" \
      --output "$smoke_dir/authenticated-mutation-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/workflow/update-goal" ||
      true
  )"
  [[ "$authenticated_mutation_status" == "200" ]] || {
    echo "production smoke: authenticated mutation returned $authenticated_mutation_status" >&2
    return 1
  }
  mutation_response="$(<"$smoke_dir/authenticated-mutation-$start_id.json")"
  [[ "$mutation_response" =~ \"status\"[[:space:]]*:[[:space:]]*\"ok\" ]] || {
    echo "production smoke: authenticated mutation response lacks ok status" >&2
    return 1
  }
  [[ "$mutation_response" =~ \"action\"[[:space:]]*:[[:space:]]*\"update_goal\" ]] || {
    echo "production smoke: authenticated mutation response lacks update action" >&2
    return 1
  }
  [[ "$mutation_response" == *"$mutated_goal"* ]] || {
    echo "production smoke: authenticated mutation response lacks execution evidence" >&2
    return 1
  }

  snapshot_status="$(
    curl --disable \
      --config "$curl_auth_config" \
      --silent --show-error \
      --output "$smoke_dir/snapshot-after-mutation-$start_id.json" \
      --write-out '%{http_code}' \
      "http://127.0.0.1:$smoke_port/api/snapshot" ||
      true
  )"
  [[ "$snapshot_status" == "200" ]] || {
    echo "production smoke: authenticated post-mutation snapshot returned $snapshot_status" >&2
    return 1
  }
  post_mutation_snapshot="$(<"$smoke_dir/snapshot-after-mutation-$start_id.json")"
  [[ "$post_mutation_snapshot" == *"$mutated_goal"* ]] || {
    echo "production smoke: authenticated mutation was not persisted" >&2
    return 1
  }
  last_mutated_goal="$mutated_goal"
}

stop_server() {
  signal="$1"
  kill "-$signal" "$server_pid"
  wait "$server_pid" 2>/dev/null || true
  server_pid=""
}

start_server
stop_server KILL
"${foundry_env[@]}" "$foundry_binary" --store "$store" store check \
  --output json >"$smoke_dir/check-after-sigkill.json"
start_server
stop_server TERM

echo "Foundry production smoke passed: store check, backup, restore, runtime reconciliation, receipt-aware request supervision and stale recovery, authenticated reads and mutations, bearer rejection, SIGKILL recovery, readiness, and graceful stop."
