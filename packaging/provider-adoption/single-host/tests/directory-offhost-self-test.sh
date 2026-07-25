#!/usr/bin/env bash
set -euo pipefail

umask 077

fail() {
  printf 'forge directory uploader self-test: %s\n' "$*" >&2
  exit 1
}

script_dir="$(
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P
)"
readonly script_dir
package_root="$(
  cd -- "$script_dir/.." && pwd -P
)"
readonly package_root
readonly uploader="$package_root/bin/forge-directory-offhost-uploader"

[[ -x "$uploader" ]] || fail "directory uploader must be executable"
export PATH="$script_dir/directory-stubs:$PATH"

test_parent="$(getent passwd "$(id -u)" | cut -d: -f6)"
[[ -n "$test_parent" && -d "$test_parent" && ! -L "$test_parent" && -w "$test_parent" ]] ||
  fail "current account requires a writable non-symlink home for secure-path tests"
test_root="$(mktemp -d "$test_parent/.forge-directory-uploader-test.XXXXXX")"
cleanup() {
  case "${test_root:-}" in
    "$test_parent"/.forge-directory-uploader-test.*)
      [[ -d "$test_root" && ! -L "$test_root" ]] && rm -rf -- "$test_root"
      ;;
  esac
}
trap cleanup EXIT

mkdir -p \
  "$test_root/remote" \
  "$test_root/other-remote" \
  "$test_root/work"
export TEST_DIRECTORY_MOUNT_TARGET="$test_root"
export TEST_DIRECTORY_MOUNT_SOURCE="fixture-remote:/forge"
export TEST_DIRECTORY_MOUNT_FSTYPE="fuse.fixture"
export TEST_DIRECTORY_MOUNT_STATE="up"
printf 'forge provider-neutral directory fixture\n' \
  >"$test_root/work/source.sqlite"
printf 'different immutable object\n' \
  >"$test_root/work/conflicting.sqlite"

source_sha256="$(sha256sum <"$test_root/work/source.sqlite")"
source_sha256="${source_sha256%% *}"
conflicting_sha256="$(sha256sum <"$test_root/work/conflicting.sqlite")"
conflicting_sha256="${conflicting_sha256%% *}"
printf '%s\n' "$source_sha256" >"$test_root/work/expected.sha256"

export CREDENTIALS_DIRECTORY="/credentials-must-not-be-required"
readonly destination="file://$test_root/remote"
readonly object_name="forge-20260725T120000Z.sqlite"
readonly remote_object="$test_root/remote/$object_name"
readonly remote_digest="$remote_object.forge-sha256"
readonly stdout_file="$test_root/work/stdout"
readonly stderr_file="$test_root/work/stderr"
readonly mount_identity_file="$test_root/work/mount-identity"

mount_fsid="$(stat -f -c '%i' -- "$test_root/remote")"
[[ "$mount_fsid" =~ ^[0-9A-Fa-f]+$ ]] ||
  fail "fixture filesystem identity is invalid"
printf '%s\n' \
  "forge-directory-mount-v1" \
  "target=$TEST_DIRECTORY_MOUNT_TARGET" \
  "source=$TEST_DIRECTORY_MOUNT_SOURCE" \
  "fstype=$TEST_DIRECTORY_MOUNT_FSTYPE" \
  "fsid=$mount_fsid" \
  >"$mount_identity_file"
export FORGE_DIRECTORY_MOUNT_IDENTITY_FILE="$mount_identity_file"

: >"$stdout_file"
: >"$stderr_file"
chmod 0750 "$test_root/other-remote"
if "$uploader" upload \
  --source "$test_root/work/source.sqlite" \
  --destination "file://$test_root/other-remote" \
  --object "unsafe-mode.sqlite" \
  --sha256 "$source_sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "destination mode other than 0700 unexpectedly succeeded"
fi
grep -Fq "destination mode must be exactly 0700" "$stderr_file" ||
  fail "destination mode rejection was not explicit"
chmod 0700 "$test_root/other-remote"

if env -u FORGE_DIRECTORY_MOUNT_IDENTITY_FILE \
  "$uploader" upload \
  --source "$test_root/work/source.sqlite" \
  --destination "$destination" \
  --object "missing-identity.sqlite" \
  --sha256 "$source_sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "file destination without a persisted mount identity unexpectedly succeeded"
fi
grep -Fq "FORGE_DIRECTORY_MOUNT_IDENTITY_FILE is required" "$stderr_file" ||
  fail "missing mount identity rejection was not explicit"
[[ ! -e "$test_root/remote/missing-identity.sqlite" ]] ||
  fail "missing mount identity wrote into the destination"

if [[ "$EUID" -eq 0 ]] && id -u nobody >/dev/null 2>&1; then
  chown "$(id -u nobody):$(id -g nobody)" "$test_root/other-remote"
  if "$uploader" upload \
    --source "$test_root/work/source.sqlite" \
    --destination "file://$test_root/other-remote" \
    --object "unsafe-owner.sqlite" \
    --sha256 "$source_sha256" \
    >"$stdout_file" 2>"$stderr_file"; then
    fail "destination owned by another account unexpectedly succeeded"
  fi
  grep -Fq "destination must be owned by the uploader account" "$stderr_file" ||
    fail "destination owner rejection was not explicit"
  chown "$(id -u):$(id -g)" "$test_root/other-remote"
fi

mkdir -m 0770 "$test_root/unsafe-ancestor"
mkdir -m 0700 "$test_root/unsafe-ancestor/remote"
if "$uploader" upload \
  --source "$test_root/work/source.sqlite" \
  --destination "file://$test_root/unsafe-ancestor/remote" \
  --object "unsafe-ancestor.sqlite" \
  --sha256 "$source_sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "group-writable destination ancestor unexpectedly succeeded"
fi
grep -Fq "destination ancestor must not be writable by group or other" "$stderr_file" ||
  fail "unsafe destination ancestor rejection was not explicit"
chmod 0700 "$test_root/unsafe-ancestor"

"$uploader" upload \
  --source "$test_root/work/source.sqlite" \
  --destination "$destination" \
  --object "$object_name" \
  --sha256 "$source_sha256" \
  >"$stdout_file" 2>"$stderr_file" ||
  fail "initial create-only upload failed"
[[ ! -s "$stdout_file" && ! -s "$stderr_file" ]] ||
  fail "successful upload emitted output"
cmp -s "$test_root/work/source.sqlite" "$remote_object" ||
  fail "uploaded object differs from source"
cmp -s "$test_root/work/expected.sha256" "$remote_digest" ||
  fail "uploaded digest sidecar differs"

object_inode="$(stat -c '%i' -- "$remote_object")"
digest_inode="$(stat -c '%i' -- "$remote_digest")"
"$uploader" upload \
  --source "$test_root/work/source.sqlite" \
  --destination "$destination" \
  --object "$object_name" \
  --sha256 "$source_sha256" \
  >"$stdout_file" 2>"$stderr_file" ||
  fail "same-digest idempotent upload failed"
[[ "$(stat -c '%i' -- "$remote_object")" = "$object_inode" &&
  "$(stat -c '%i' -- "$remote_digest")" = "$digest_inode" ]] ||
  fail "idempotent retry replaced immutable remote files"

if "$uploader" upload \
  --source "$test_root/work/conflicting.sqlite" \
  --destination "$destination" \
  --object "$object_name" \
  --sha256 "$conflicting_sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "different-content conflict unexpectedly succeeded"
fi
cmp -s "$test_root/work/source.sqlite" "$remote_object" ||
  fail "conflict overwrote the remote object"
cmp -s "$test_root/work/expected.sha256" "$remote_digest" ||
  fail "conflict overwrote the remote digest"

"$uploader" verify \
  --destination "$destination" \
  --object "$object_name" \
  >"$stdout_file" 2>"$stderr_file" ||
  fail "independent remote verification failed"
cmp -s "$test_root/work/expected.sha256" "$stdout_file" ||
  fail "verify did not print exactly the independently checked digest"
[[ ! -s "$stderr_file" ]] || fail "verify emitted stderr"

"$uploader" download \
  --destination "$destination" \
  --object "$object_name" \
  --output "$test_root/work/downloaded.sqlite" \
  --sha256-output "$test_root/work/downloaded.sqlite.sha256" \
  >"$stdout_file" 2>"$stderr_file" ||
  fail "verified create-only download failed"
[[ ! -s "$stdout_file" && ! -s "$stderr_file" ]] ||
  fail "successful download emitted output"
cmp -s "$test_root/work/source.sqlite" "$test_root/work/downloaded.sqlite" ||
  fail "downloaded object differs"
cmp -s \
  "$test_root/work/expected.sha256" \
  "$test_root/work/downloaded.sqlite.sha256" ||
  fail "downloaded digest differs"

cp -- "$test_root/work/source.sqlite" "$test_root/work/resumed.sqlite"
resumed_inode="$(stat -c '%i' -- "$test_root/work/resumed.sqlite")"
"$uploader" download \
  --destination "$destination" \
  --object "$object_name" \
  --output "$test_root/work/resumed.sqlite" \
  --sha256-output "$test_root/work/resumed.sqlite.sha256" \
  >"$stdout_file" 2>"$stderr_file" ||
  fail "retry did not complete a matching object-only download pair"
[[ "$(stat -c '%i' -- "$test_root/work/resumed.sqlite")" = "$resumed_inode" ]] ||
  fail "retry replaced the matching preexisting download object"
cmp -s "$test_root/work/expected.sha256" "$test_root/work/resumed.sqlite.sha256" ||
  fail "retry did not publish the missing digest output"

cp -- "$test_root/work/expected.sha256" "$test_root/work/resumed-digest.sqlite.sha256"
resumed_digest_inode="$(
  stat -c '%i' -- "$test_root/work/resumed-digest.sqlite.sha256"
)"
"$uploader" download \
  --destination "$destination" \
  --object "$object_name" \
  --output "$test_root/work/resumed-digest.sqlite" \
  --sha256-output "$test_root/work/resumed-digest.sqlite.sha256" \
  >"$stdout_file" 2>"$stderr_file" ||
  fail "retry did not complete a matching digest-only download pair"
[[ "$(stat -c '%i' -- "$test_root/work/resumed-digest.sqlite.sha256")" = "$resumed_digest_inode" ]] ||
  fail "retry replaced the matching preexisting digest output"
cmp -s "$test_root/work/source.sqlite" "$test_root/work/resumed-digest.sqlite" ||
  fail "retry did not publish the missing object output"

printf 'preexisting output must survive\n' >"$test_root/work/no-overwrite.sqlite"
printf 'preexisting digest must survive\n' \
  >"$test_root/work/no-overwrite.sqlite.sha256"
cp -- "$test_root/work/no-overwrite.sqlite" \
  "$test_root/work/no-overwrite.sqlite.before"
cp -- "$test_root/work/no-overwrite.sqlite.sha256" \
  "$test_root/work/no-overwrite.sqlite.sha256.before"
if "$uploader" download \
  --destination "$destination" \
  --object "$object_name" \
  --output "$test_root/work/no-overwrite.sqlite" \
  --sha256-output "$test_root/work/no-overwrite.sqlite.sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "download overwrote existing output paths"
fi
cmp -s \
  "$test_root/work/no-overwrite.sqlite.before" \
  "$test_root/work/no-overwrite.sqlite" ||
  fail "failed download changed the existing object output"
cmp -s \
  "$test_root/work/no-overwrite.sqlite.sha256.before" \
  "$test_root/work/no-overwrite.sqlite.sha256" ||
  fail "failed download changed the existing digest output"

cp -- "$remote_object" "$test_root/work/remote-object.before-corruption"
printf 'corrupt remote bytes\n' >"$remote_object"
if "$uploader" verify \
  --destination "$destination" \
  --object "$object_name" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "remote object corruption unexpectedly verified"
fi
cp -- "$test_root/work/remote-object.before-corruption" "$remote_object"

printf '%064d\n' 0 >"$remote_digest"
if "$uploader" verify \
  --destination "$destination" \
  --object "$object_name" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "digest sidecar corruption unexpectedly verified"
fi
printf '%s\n' "$source_sha256" >"$remote_digest"

ln -s -- "$test_root/other-remote" "$test_root/remote-link"
if "$uploader" verify \
  --destination "file://$test_root/remote-link" \
  --object "$object_name" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "symlink destination unexpectedly passed validation"
fi

printf 'symlink target must not be read\n' \
  >"$test_root/other-remote/symlink-target"
ln -s -- \
  "$test_root/other-remote/symlink-target" \
  "$test_root/remote/symlink.sqlite"
printf '%s\n' "$source_sha256" \
  >"$test_root/remote/symlink.sqlite.forge-sha256"
if "$uploader" verify \
  --destination "$destination" \
  --object "symlink.sqlite" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "symlink remote object unexpectedly passed validation"
fi

if "$uploader" verify \
  --destination "$destination" \
  --object "../escape.sqlite" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "traversal object name unexpectedly passed validation"
fi
if "$uploader" verify \
  --destination "$destination" \
  --object "backup.sqlite.forge-sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "reserved digest-sidecar suffix unexpectedly passed object validation"
fi
grep -Fq "invalid object name" "$stderr_file" ||
  fail "reserved digest-sidecar suffix rejection was not explicit"
if locale -a | grep -Fxq "pt_PT.utf8"; then
  if LC_ALL=pt_PT.utf8 "$uploader" verify \
    --destination "$destination" \
    --object "café.sqlite" \
    >"$stdout_file" 2>"$stderr_file"; then
    fail "locale-expanded non-ASCII object name unexpectedly passed validation"
  fi
  grep -Fq "invalid object name" "$stderr_file" ||
    fail "locale-independent ASCII object rejection was not explicit"
fi
if "$uploader" verify \
  --destination "file://$test_root/work/../remote" \
  --object "$object_name" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "non-canonical destination traversal unexpectedly passed validation"
fi

export TEST_DIRECTORY_MOUNT_STATE="down"
if "$uploader" upload \
  --source "$test_root/work/source.sqlite" \
  --destination "$destination" \
  --object "mount-fallback.sqlite" \
  --sha256 "$source_sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "upload wrote through the underlying directory after mount loss"
fi
grep -Fq "mount identity changed or is unavailable" "$stderr_file" ||
  fail "upload mount-loss rejection was not explicit"
[[ ! -e "$test_root/remote/mount-fallback.sqlite" ]] ||
  fail "upload created an object in the underlying directory after mount loss"

if "$uploader" verify \
  --destination "$destination" \
  --object "$object_name" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "verify read the underlying directory after mount loss"
fi
grep -Fq "mount identity changed or is unavailable" "$stderr_file" ||
  fail "verify mount-loss rejection was not explicit"

if "$uploader" download \
  --destination "$destination" \
  --object "$object_name" \
  --output "$test_root/work/mount-fallback-download.sqlite" \
  --sha256-output "$test_root/work/mount-fallback-download.sqlite.sha256" \
  >"$stdout_file" 2>"$stderr_file"; then
  fail "download read the underlying directory after mount loss"
fi
grep -Fq "mount identity changed or is unavailable" "$stderr_file" ||
  fail "download mount-loss rejection was not explicit"
[[ ! -e "$test_root/work/mount-fallback-download.sqlite" &&
  ! -e "$test_root/work/mount-fallback-download.sqlite.sha256" ]] ||
  fail "mount-loss download published local outputs"

printf 'forge directory off-host uploader self-test: ok\n'
