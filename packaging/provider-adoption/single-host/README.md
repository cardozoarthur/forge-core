# Single-host provider-adoption bundle

This directory contains optional operator-side adapters for the supported Foundry
single-host profile. Foundry Core remains the orchestration and workflow-state
authority. The release bundle contains:

- `bin/foundry-directory-offhost-uploader`, a provider-neutral uploader for a
  canonical `file:///absolute/path` destination;
- `bin/foundry-production-alert`, a transition-deduplicated health notifier;
- `systemd/foundry-production-alert.{service,timer}`, the bounded alert schedule;
- `tests/self-test.sh`, the complete offline test entrypoint.

Cloud-specific uploaders and resource provisioning belong in separately
operated addons. They are not packaged or tested as part of the Foundry Core
release gate.

## Offline validation

The self-tests use local stubs, do not contact Telegram, and require no real
credentials:

```bash
bash packaging/provider-adoption/single-host/tests/self-test.sh
```

The directory test proves create-only upload, same-digest idempotency, conflict
rejection, independent sidecar verification, corruption detection, atomic
no-overwrite download, symlink/traversal rejection, and durable file and
directory synchronization. It also stubs a mount loss and proves that upload,
verify, and download all fail before touching the underlying host directory.
The alert test covers all six health categories, failure deduplication,
recovery notification, and secret-free argv/output.

## Provider-neutral directory or mount

`foundry-directory-offhost-uploader` uses an existing canonical directory as its
entire non-secret destination. It never reads credentials or invokes a network
provider SDK. The destination must have the exact form
`file:///absolute/canonical/path`; URI percent encoding is intentionally not
accepted, so the configured value cannot diverge from the audited filesystem
path.

Use the offline self-test above for a local adapter simulation. Direct
`file://` operations deliberately require the mount identity file captured by
the installer; an ordinary directory on the host root filesystem is rejected.

A local directory proves the adapter contract but not off-host recovery. A
second disk mounted in the Foundry host is also only a recovery simulation unless
its operational loss domain is genuinely independent. For production, point
the same `file:///...` destination at a separately operated, persistently
mounted filesystem whose loss domain is independent from the Foundry host.

The adapter publishes the object and its `.foundry-sha256` sidecar with
create-only hard links, and independently hashes the stored bytes during every
verification. Each downloaded file is published atomically and never
overwritten. A retry completes an interrupted one-file pair only when the
existing component matches the independently verified remote SHA-256.
Consumers must require both files to verify. The mounted filesystem must
support same-directory hard links plus durable file and directory
synchronization.

## Installation with `file://`

Install the adapter without any backup-provider credential:

```bash
bundle=packaging/provider-adoption/single-host

sudo install -d -m 0755 -o root -g root /usr/local/libexec
sudo install -m 0755 -o root -g root \
  "$bundle/bin/foundry-directory-offhost-uploader" \
  /usr/local/libexec/foundry-directory-offhost-uploader

if ! getent group foundry >/dev/null; then
  sudo groupadd --system foundry
fi
if ! id -u foundry >/dev/null 2>&1; then
  sudo useradd \
    --system \
    --gid foundry \
    --home-dir /var/lib/foundry \
    --shell "$(command -v nologin)" \
    foundry
fi

sudo install -d -m 0700 -o foundry -g foundry \
  /srv/backups-secondary/foundry

sudo bash packaging/systemd/install-service.sh \
  /absolute/path/to/verified/foundry \
  /usr/local/libexec/foundry-directory-offhost-uploader \
  file:///srv/backups-secondary/foundry \
  remote-mount-generation-1
```

The dedicated mount and destination must already exist. The resolved mount
target must not be `/`, and the destination must be a subdirectory below that
target so the identity check remains independent of systemd's writable-path
bind mount. The destination must be owned by `foundry`, have mode exactly `0700`,
and use an exact canonical, unencoded URI. Every ancestor must contain no
symlink, be owned by root or `foundry`, and not be writable by group or other.
Root-owned mount roots are supported.

Before the first backup, the installer proves as `foundry` that file creation,
same-directory hard links, durable file and directory synchronization, and
cleanup all work. The base backup unit may write only to
`/var/backups/foundry`. For `file://`, the installer transactionally manages:

```text
/etc/systemd/system/foundry-backup.service.d/20-directory-offhost.conf
/etc/foundry/backup-offhost-mount-identity
```

The drop-in adds `RequiresMountsFor=` and grants `ReadWritePaths=` only for the
resolved destination. The non-secret identity records the mount target, source,
filesystem type, and filesystem ID. `foundry-backup` and the adapter compare that
identity immediately before every backup, upload, verify, and download. A
missing or changed mount therefore fails closed instead of writing to the host
directory hidden beneath it. Changing the directory replaces both files;
switching to a non-file provider removes them. A failed installation restores
the previous files or their absence before restarting any previously active
service.

Bump the non-secret target generation whenever the remote filesystem, mount
identity, retention policy, or loss domain changes.

## Telegram alert credential

No secret belongs in this repository, a unit file, an environment variable, a
command argument, or the non-secret backup destination.

`foundry-production-alert` accepts authentication only from the systemd
credential named `foundry-telegram-alert`. Its owner-only source file contains
exactly two lines: the Telegram bot token, then the numeric chat ID:

```text
/etc/foundry/credentials/foundry-telegram-alert
```

Provision it through the operator's secret manager or a secure editor. The
credential file must be owned by root with mode `0600`; its parent directory
must be root-owned with mode `0700`.

After provisioning it, install and start alerting:

```bash
bundle=packaging/provider-adoption/single-host

sudo install -m 0755 -o root -g root \
  "$bundle/bin/foundry-production-alert" \
  /usr/local/libexec/foundry-production-alert
sudo install -m 0644 -o root -g root \
  "$bundle/systemd/foundry-production-alert.service" \
  /etc/systemd/system/foundry-production-alert.service
sudo install -m 0644 -o root -g root \
  "$bundle/systemd/foundry-production-alert.timer" \
  /etc/systemd/system/foundry-production-alert.timer
sudo systemctl daemon-reload
sudo systemctl enable --now foundry-production-alert.timer
```

Run real target and alert checks only during an explicit production promotion
step. Keep their receipts with the production-readiness evidence described in
`docs/production-single-host.md`.
