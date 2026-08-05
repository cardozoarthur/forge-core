#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=C

readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly project_root="$(cd -- "$script_dir/.." && pwd)"
readonly allowlist_file="$script_dir/foundry-branding-allowlist.txt"
readonly legacy_pattern='(FORGE|Forge|forge|\.forge|cardozoarthur/forge-core)' # foundry-brand-allow: legacy-compat
readonly marker_pattern='foundry-brand-allow: (legacy-compat|migration|historical-release)'

fail() {
  printf 'foundry branding gate: %s\n' "$*" >&2
  exit 1
}

[[ -f "$allowlist_file" && ! -L "$allowlist_file" ]] ||
  fail "missing explicit allowlist: $allowlist_file"

cd -- "$project_root"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 ||
  fail "project root is not a Git worktree"

declare -A allowlisted_paths=()
while IFS=$'\t' read -r allowed_path reason extra; do
  [[ -n "$allowed_path" ]] || continue
  [[ "$allowed_path" != \#* ]] || continue
  [[ -n "$reason" && -z "${extra:-}" ]] ||
    fail "allowlist entries must contain exactly PATH, tab, and REASON"
  [[ "$allowed_path" != /* &&
    "$allowed_path" != *'..'* &&
    "$allowed_path" != *'*'* &&
    "$allowed_path" != *'?'* &&
    "$allowed_path" != *'['* ]] ||
    fail "allowlist path must be exact and repository-relative: $allowed_path"
  [[ -z "${allowlisted_paths[$allowed_path]:-}" ]] ||
    fail "duplicate allowlist path: $allowed_path"
  allowlisted_paths["$allowed_path"]="$reason"
done <"$allowlist_file"

violations=0
scanned_files=0

report_violation() {
  local path="$1"
  local line_number="$2"
  local detail="$3"

  printf '%s:%s: unapproved legacy branding: %s\n' \
    "$path" "$line_number" "$detail" >&2
  violations="$((violations + 1))"
}

while IFS= read -r -d '' path; do
  [[ -f "$path" && ! -L "$path" ]] || continue
  case "$path" in
    .git/* | target/* | output/* | artifacts/* | docs/reports/* | docs/research/* | \
      .foundry/*.sqlite | .foundry/*.sqlite-* | \
      .foundry/worktrees/* | .foundry/execution/* | .foundry/artifacts/*)
      continue
      ;;
    .agents/*)
      # `.agents/skills/**` is active product surface. Sibling trees are
      # immutable executor receipts and run history.
      [[ "$path" == .agents/skills/* ]] || continue
      ;;
  esac

  if [[ -n "${allowlisted_paths[$path]:-}" ]]; then
    continue
  fi

  if [[ "$path" =~ $legacy_pattern ]]; then
    report_violation "$path" 0 "legacy branding in active path"
  fi

  [[ -s "$path" ]] || continue
  LC_ALL=C grep -Iq . "$path" || continue
  scanned_files="$((scanned_files + 1))"

  line_number=0
  previous_line=""
  while IFS= read -r line || [[ -n "$line" ]]; do
    line_number="$((line_number + 1))"
    if [[ "$line" =~ $legacy_pattern ]] &&
      [[ ! "$line" =~ $marker_pattern ]] &&
      [[ ! "$previous_line" =~ $marker_pattern ]]; then
      report_violation "$path" "$line_number" "$line"
    fi
    previous_line="$line"
  done <"$path"
done < <(git ls-files -co --exclude-standard -z | sort -z -u)

((violations == 0)) ||
  fail "$violations unapproved legacy branding occurrence(s) found"

printf 'foundry branding gate: PASS (%s active text files scanned)\n' "$scanned_files"
