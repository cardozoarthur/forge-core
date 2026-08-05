# Foundry v0.6 single-host production runbook

## Supported profile

Foundry v0.6 supports production operation by one trusted operator on one Linux
host. The Foundry runtime, SQLite store, local Ops service, verified backups, and
executor policy remain on that host.

Operators upgrading a host from the legacy Forge generation must follow the <!-- foundry-brand-allow: migration -->
[explicit migration contract](migration-to-foundry.md). The installer does not
rename stores, service directories or historical data automatically and must
never run old and new writers against divergent copies.

Foundry Core has no runtime, build, installation, or release dependency on AWS,
S3, or another cloud provider. The production contract requires a recoverable
off-host copy, not a specific transport. Operators may implement the uploader
contract with another host, managed storage, or an object store of their
choice. The release archive includes only the provider-neutral `file://`
uploader; cloud-specific uploaders and provisioning belong in separately
operated addons. The optional `foundry aws` CLI and MCP surfaces only delegate to
an independently installed plugin when explicitly invoked; this production
profile never calls them.

The release binary targets GNU Linux on x86_64 or ARMv8-A and requires Linux
kernel 4.18 or newer plus glibc 2.34 or newer. The supported service profile
also requires systemd 249 or newer. Ubuntu 22.04 LTS is the tested floor;
compatible newer distributions are supported.

The following are outside the v0.6 production profile:

- public exposure of the built-in Ops HTTP server;
- multi-tenant or untrusted-user operation;
- high availability, active-active replicas, or a shared network store;
- Kubernetes or Knative installation managed by Foundry;
- experimental `sdk/` language stubs as a supported integration contract;
- macOS platform notarization and Windows ARM64 packages.

## Install the service

Download a release archive, `SHA256SUMS`, and
`SHA256SUMS.sigstore.json`. Verify the archive checksum and, for a release
promotion, the Sigstore bundle as described in `installer/README.md`. Then:

```bash
tar -xzf foundry-linux-x86_64.tar.gz
sudo bash packaging/systemd/install-service.sh \
  "$PWD/foundry" \
  /usr/local/sbin/foundry-offhost-uploader \
  "provider://foundry-production-backups" \
  "production-account-2026-07"
```

The second argument is an operator-owned executable, the third is an opaque,
non-secret destination, and the fourth is an explicit non-secret target
generation. Bump the generation whenever the remote account, credential,
bucket generation, retention domain, or trust boundary changes, even when the
destination text stays the same. The installer refuses to enable production
without all three values. It stores them as root-owned `0600` files at
`/etc/foundry/backup-offhost-command` and
`/etc/foundry/backup-offhost-destination`, and
`/etc/foundry/backup-offhost-generation`; systemd delivers their contents to the
backup service as credentials.

For a provider-neutral directory or mounted filesystem, use the optional
`foundry-directory-offhost-uploader` and an exact
`file:///absolute/canonical/path` destination. The directory must already
exist on a dedicated mount whose resolved target is not `/`, be owned by
`foundry`, and have mode exactly `0700`. The destination must be a subdirectory
below that mount target so runtime identity checks are independent of the
service's writable-path bind mount. Every ancestor must be canonical, contain
no symlink, be owned by root or `foundry`, and not be writable by group or other;
root-owned mount roots are supported. The installer and adapter validate this
chain, then the installer proves as `foundry` file creation, same-directory hard
links, durable file and directory sync, and cleanup before changing the
services.

Because `foundry-backup.service` otherwise makes only `/var/backups/foundry`
writable, a validated `file://` destination also causes the installer to
atomically manage:

```text
/etc/systemd/system/foundry-backup.service.d/20-directory-offhost.conf
```

That drop-in adds `RequiresMountsFor=` and grants `ReadWritePaths=` only for the
resolved destination. The installer also persists the mount target, source,
filesystem type, and filesystem ID in the non-secret
`/etc/foundry/backup-offhost-mount-identity`. `foundry-backup` and the directory
adapter revalidate it immediately before every backup, upload, verification,
and download, so a missing mount cannot silently redirect writes to the host
directory beneath it. Both files are installed before `daemon-reload` and the
initial recovery challenge. Re-running the installer with a different
directory replaces them. Re-running it with a non-file provider removes them.
Both are part of the same rollback snapshot as the units and provider
configuration, so a failed promotion restores the previous files or their
absence. No filesystem grant or mount identity is created for a non-file
provider.

The ready-to-use adapter and account/directory preparation commands are in
`packaging/provider-adoption/single-host/README.md`. A second disk in the same
host is useful for a recovery simulation, but it is not proof of an off-host
loss domain.

The executable and every parent path component through `/` must be root-owned,
canonical, non-symlink, and not writable by group or other. Both the installer
and runtime reject an unsafe path chain. The backup unit deliberately does not
load the Foundry vault key, and the wrapper removes any inherited Foundry vault-key
variables before invoking the uploader. The uploader receives only its own
operator-configured authentication mechanism.

The uploader contract is deliberately transport-neutral. Foundry invokes the
executable three times for every complete recovery challenge:

```text
UPLOADER upload --source LOCAL_PATH --destination TARGET --object NAME --sha256 HEX
UPLOADER verify --destination TARGET --object NAME
UPLOADER download --destination TARGET --object NAME --output LOCAL_PATH --sha256-output DIGEST_PATH
```

`upload` must durably create the remote object without replacing an existing
object. `verify` must consult the stored remote bytes without receiving the
local source or expected digest, then print exactly one lowercase SHA-256 and
no other stdout. Foundry validates its format and equality; exit status zero
alone is never success. Both operations must be idempotent and must return
non-zero on an incomplete or ambiguous result. The uploader must persist the
digest as immutable object metadata or a sidecar. `download`, used for every
recovery challenge and operator drill, must retrieve both the object and that
persisted digest without depending on the original host's journal.

Do not put passwords, access keys, bearer tokens, signed URLs, or other
credentials in `TARGET`. The uploader must obtain authentication out of band,
for example from workload identity or an additional systemd credential:

```ini
# sudo systemctl edit foundry-backup.service
[Service]
LoadCredential=foundry-offhost-auth:/etc/foundry/offhost-auth
```

The uploader can then read
`$CREDENTIALS_DIRECTORY/foundry-offhost-auth`. Keep that credential under
operator-controlled rotation and never print it. After adding or rotating a
credential, bump the target generation and rerun the installer so the new
identity must pass promotion again.

The uploader retains `CREDENTIALS_DIRECTORY`, but every uploader invocation
removes Foundry vault and Ops token variables. Conversely, every Foundry invocation
from the backup or restore path removes `CREDENTIALS_DIRECTORY` and all
off-host configuration-file variables. This reciprocal boundary prevents
either executable from inheriting the other's authority.

Installation is a fail-closed go-live gate. It disables Ops, the workflow
runtime, the request supervisor, and the backup timer,
starts Ops only long enough to initialize or open the store, and requires an
HTTP `200` from an authenticated loopback `/api/snapshot` probe before stopping
Ops and running `foundry-backup.service`. That first backup must complete remote
upload, digest-only verification, download, `store check`, disposable restore,
and a second `store check`. After promotion, the installer starts Ops, requires
the same authenticated probe again, starts the workflow runtime and request
supervisor, requires both processes to remain active, then starts the backup
timer. If any step fails, Ops, runtime, request supervision and the timer remain
stopped and disabled.

Before replacing any managed key, configuration, helper, unit, or binary, the
installer snapshots the previous files and records whether Ops, runtime, request
supervisor, backup, and timer units were enabled or active. A failed
initialization, recovery challenge, authenticated readiness probe, runtime or
request-supervisor start, timer start, or final status check triggers a
transactional rollback: the candidate services are stopped, previous files and
unit enablement are restored, `daemon-reload` runs, and only services that were
previously active are restarted. Treat an explicit “rollback incomplete”
message as an incident and keep the host isolated.

The installer creates a locked `foundry` service account and installs:

- binary: `/usr/local/bin/foundry`;
- store: `/var/lib/foundry/foundry.sqlite`;
- project root: `/var/lib/foundry/workspace`;
- backups: `/var/backups/foundry`;
- off-host uploader and non-secret destination:
  `/etc/foundry/backup-offhost-command` and
  `/etc/foundry/backup-offhost-destination`;
- off-host target generation: `/etc/foundry/backup-offhost-generation`;
- vault key: `/etc/foundry/secret.key`, delivered through a systemd credential;
- Ops bearer token: `/etc/foundry/ops-token`, delivered through a separate
  systemd credential;
- loopback Ops endpoint: `http://127.0.0.1:8765`;
- systemd services: `foundry-ops.service`, `foundry-runtime.service`,
  `foundry-request-supervisor.service`, and `foundry-backup.timer`.
- credential-aware admin wrapper: `/usr/local/sbin/foundry-admin`.
- fail-fast restore drill: `/usr/local/sbin/foundry-restore-drill`.

The Ops unit has `UMask=0077`, a read-only host filesystem except for the Foundry
state directory, no Linux capabilities, restricted kernel surfaces and address
families, and automatic restart on failure. The backup unit is stricter:
`/var/lib/foundry` is read-only and `/var/backups/foundry` plus an explicitly
validated `file://` destination are the only writable paths, so an uploader
cannot corrupt the live store. The store administration path uses normal
read-only WAL access when sidecars exist. After a clean checkpoint with no WAL,
SHM or rollback journal, it may use SQLite's immutable read-only mode; any
existing sidecar blocks that fallback so uncheckpointed transactions are never
silently ignored. A non-file provider receives no additional filesystem write
path. Keep Ops bound to loopback. Use an SSH tunnel or a separately managed
authenticated TLS reverse proxy for remote operator access.

`foundry-backup.service` is a bounded oneshot with
`TimeoutStartSec=30min`. A hung uploader, verification, download, or restore
challenge therefore fails the unit instead of blocking the timer indefinitely.

The installer generates a 32-byte hexadecimal vault key with mode `0600` when
`/etc/foundry/secret.key` does not exist. The unit exposes only the systemd
credential path through `FOUNDRY_SECRET_VAULT_KEY_FILE`; it never places key
material in the unit or environment. Existing installations may instead
provision that file from their secret manager before running the installer.

Back up the vault key separately under encryption and independent access
control. Never publish it, place it in a release, or store it next to a
publicly accessible database backup. A restored database requires its current
key or a deliberately configured previous key through
`FOUNDRY_SECRET_VAULT_PREVIOUS_KEYS` or
`FOUNDRY_SECRET_VAULT_PREVIOUS_KEY_FILES`.

It also generates an independent bearer token at `/etc/foundry/ops-token`. In
production mode, the Ops server refuses an inline token and reads only the
root-owned systemd credential. Do not set `FOUNDRY_OPS_ALLOW_REMOTE`; the
supported unit remains loopback-only.

### Workflow runtime service

`foundry-ops.service` only serves the loopback HTTP surface. The installer also
enables `foundry-runtime.service`, which runs the Foundry-owned
`events runtime-daemon` continuously. It dispatches event activations,
reconciles stale event-worker leases, and scans due cron and one-shot
`wait_until` schedules under bounded worker settings. Both units use
`UMask=0077`, have no Linux capabilities, and may write only below
`/var/lib/foundry`.

`foundry-request-supervisor.service` runs the transactional
`foundry request supervise` loop continuously with a 30-second interval. Each pass
advances at most one bounded step per eligible request, marks stale owned runs
for attention, and does not take over a run with a fresh heartbeat from another
executor. A supervisor failure exits non-zero so systemd applies bounded restart
policy; ordinary handoff, rework, validation, and operator-attention states are
parked and recorded rather than retried blindly.

The runtime service is part of the fail-closed installation transaction. It is
started only after the authenticated Ops probe and initial off-host recovery
challenge pass. The installer requires the runtime process to remain active,
requires the request supervisor to remain active, and restores the previous
units, enablement, and active states if promotion fails.

## Fast production simulation

The bounded smoke replaces a multi-day soak for the v0.6 release decision. It
creates a temporary store, verifies private file modes, performs a real SQLite
backup and restore, proves unauthenticated mutation is rejected, probes the
authenticated Ops endpoint, kills the process with `SIGKILL`, checks the store,
restarts it, and finishes with `SIGTERM`:

```bash
cargo build --release
bash packaging/smoke-production.sh target/release/foundry
bash packaging/smoke-offhost-backup.sh target/release/foundry
```

These smokes are release gates, not evidence of high availability or
multi-tenant safety.

### Fail-closed production-readiness decision

Capability completion and operational production readiness are separate gates.
Inspect the non-mutating evidence plan first:

```bash
foundry milestone production-plan --version 0.6 --output json
```

Generate an operator draft for the exact Foundry release being evaluated. The
release version must match the running `foundry` binary. Both paths below are
relative to the evidence root; output files are never overwritten. Create the
private, operator-controlled evidence root before the first command that uses
it:

```bash
sudo install -d -m 0700 -o foundry -g foundry /var/lib/foundry/evidence
sudo /usr/local/sbin/foundry-admin \
  milestone production-evidence-template \
  --version 0.6 \
  --release-version 0.6.0 \
  --evidence-root /var/lib/foundry/evidence \
  --template production-evidence-draft.json \
  --output json
```

Every observed claim and every `sources.<kind>.artifact_path` /
`observed_at_epoch` starts as `null`. Fill all claims from real operator or
collector output; do not convert missing observations to `true`. Keep each of
the 13 source artifacts as a non-empty, secret-free UTF-8 regular file inside
the evidence root. Paste `mission_operational_lifecycle` exactly from the
`manifest_section` returned by `production-mission-evidence`.

After all `null` values are resolved, assemble source-bound receipts and the
final manifest:

```bash
sudo /usr/local/sbin/foundry-admin \
  milestone production-evidence-assemble \
  --version 0.6 \
  --release-version 0.6.0 \
  --evidence-root /var/lib/foundry/evidence \
  --draft production-evidence-draft.json \
  --receipt-dir receipts/0.6.0 \
  --manifest production-readiness.json \
  --output json
```

Assembly is offline with respect to production infrastructure: it runs no
probe, changes no service and infers no claim. It requires all 13 kinds,
rechecks contained non-symlink source paths, freshness and secret scanning,
then derives source, canonical-claims, receipt and manifest SHA-256 values.
Outputs use atomic no-overwrite publication. The evaluator revalidates the
bound source bytes, so later source drift closes readiness. Unbound receipt
schema `v1` is accepted only for the historical Forge subject version `0.5.2`; <!-- foundry-brand-allow: historical-release -->
the historical Forge `0.5.3` release and every Foundry release, including <!-- foundry-brand-allow: historical-release -->
`0.6.0`, require source-bound `v2`.

After collecting fresh, secret-free evidence under one operator-controlled
directory, evaluate it without running commands or changing infrastructure:

```bash
foundry milestone production-readiness \
  --version 0.6 \
  --manifest production-readiness.json \
  --evidence-root /absolute/path/to/evidence \
  --output json
```

The manifest has 14 independent receipts. The first 13 bind release artifacts
for all supported targets, SBOM/checksums/Sigstore/provenance, the installed
binary and authenticated loopback Ops probe, off-host recovery challenge,
separately protected vault-key escrow, alert delivery, restore RPO/RTO,
upgrade/rollback and bounded load/crash-restart results. The 14th binds the
exact canonical mission-platform inventory `1` through `40` and its SHA-256 to
one real, ordered `execute -> submit -> resume` lifecycle: successful
execution, queued submission referencing that execution receipt, and resume
consuming the same handoff.

Evidence must be recent, UTF-8, secret-free, non-symlinked, inside the evidence
root and SHA-256-bound to both the evaluated release version and the exact
manifest claims. The command exits non-zero unless all 11 blocking gates pass.
A generated fixture or a 40/40 bounded mission simulation is not operational
production evidence and always reports `production_ready=false`.

The release workflow runs a fast 40/40 bounded smoke and a separate operational
mission-lifecycle smoke before publishing artifacts. Those pre-publish checks
do not replace this post-installation promotion decision: installation, Ops,
off-host recovery, escrow, alerting and restore receipts necessarily describe
the deployed candidate.

### Operational mission drill and evidence

Use the installed candidate and production store for the operational drill.
Place an approved, foundry-owned Git checkout below
`/var/lib/foundry/workspace/<project>`; the admin wrapper permits writes only in
the managed Foundry directories. Keep every JSON response because IDs, revisions,
receipt hashes and timestamps are part of the 14th production receipt.

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission start \
  --goal "<bounded production-candidate objective>" \
  --squad software-factory \
  --worktree /var/lib/foundry/workspace/<project> \
  --output json

sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  context --workflow <workflow-id> --task <task-id> \
  --project-root /var/lib/foundry/workspace/<project> \
  --budget 4096 --strict --view compact --output json

sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission drive <mission-id> --output json
```

Read the projected task ID from the `start` or latest `inspect` response and
request strict context while that task is still pending. Continue only when it
reports `handoff_ready=true` and `guardrail.status=ready`. Then call `drive`,
require the assignment to use that same task ID, retain its agent ID, request
the evidence kinds named by the assignment, and repeat `--command` for each
argument:

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission execute <mission-id> \
  --task <task-id> --agent <agent-id> \
  --idempotency-key <unique-execution-key> \
  --purpose test --approved-by <operator> \
  --evidence <required-kind> \
  --command <executable> --command <argument> \
  --output json

sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission submit <mission-id> \
  --task <task-id> --agent <agent-id> \
  --idempotency-key <unique-submission-key> \
  --receipt-id <execution-receipt-id> \
  --summary "<validated bounded result>" \
  --output json

sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission resume <mission-id> --output json
```

For the production receipt, use a resume response with
`action=handoff_consumed`; `mission_completed` is the later terminal response,
not a substitute for the consumed-handoff evidence. Repeat the loop for each
assignment and any `repair_created` action, then inspect both ledgers and the
projected workflow:

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission inspect <mission-id> --output json
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission execution list --mission <mission-id> --output json
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  validate --workflow <workflow-id> --output json
```

The typed lifecycle bundle must be `foundry.milestone.mission_lifecycle.v1` and
bind the exact canonical inventory schema, numbers `1` through `40` and
inventory SHA-256 to:

- a completed, attempted and executed
  `foundry.mission.execution_receipt.v3` with exit code zero;
- the initial `foundry.mission.submit.v1` report in `queued` state, linked to that
  receipt and carrying non-empty handoff and inbox IDs;
- a later `foundry.mission.drive.v1` resume report with
  `action=handoff_consumed`, the same handoff accepted, and its inbox consumed;
- distinct canonical receipt digests and ordered execute, submit and resume
  timestamps.

Generate the typed bundle from the persisted store-backed records; do not
hand-author hashes or treat captured terminal text as the receipt. Reuse the
same operator-controlled evidence root created above:

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  milestone production-mission-evidence \
  --mission <mission-id> \
  --receipt <execution-receipt-id> \
  --evidence-root /var/lib/foundry/evidence \
  --artifact mission-operational-lifecycle.json \
  --release-version 0.6.0 \
  --output json
```

The command serializes exactly the typed artifact bytes, writes them below the
evidence root, verifies their SHA-256 against the returned `manifest_section`
and prints the complete package. Copy that `manifest_section` into
`production-readiness.json`; do not recalculate or hand-edit its claims. The
artifact path must be relative and every parent remains inside the evidence
root. Then evaluate against the same production store:

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  milestone production-readiness \
  --version 0.6 \
  --manifest /var/lib/foundry/evidence/production-readiness.json \
  --evidence-root /var/lib/foundry/evidence \
  --output json
```

The evaluator opens the source store read-only, runs its integrity check and
cross-checks the typed bundle against the persisted execution, handoff, inbox,
checkpoint and event history. Missing, stale, mismatched or hand-authored
claims fail closed. A successful 40/40 `simulate-platform` run still proves
none of these operational facts.

If execution is `failed`, `timed_out` or `indeterminate`, inspect its receipt
and do not dispatch another assignment. Only after independent evidence proves
the command had no effect may the operator record:

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mission execution reconcile <receipt-id> \
  --outcome no_effect_retry \
  --approved-by <operator> \
  --reason "<independent no-effect evidence>" \
  --confirm-no-effect-retry \
  --output json
```

Reconciliation cannot rewrite completed or consumed receipts. A stale mission
revision requires a fresh `inspect`, strict context, `drive` and execution.
Lease conflicts, dead letters or divergent history require inspection and
recovery through Foundry commands; never edit production SQLite rows manually.

For a local MCP client, start the stdio JSON-RPC server as a child process:

```bash
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  mcp serve
```

The server supports MCP `initialize`, `ping`, `tools/list`, and `tools/call`.
Keep stdio attached to the trusted local client; it is not a network listener.

## Routine checks

```bash
sudo systemctl --no-pager --full status foundry-ops.service
sudo journalctl -u foundry-ops.service --since "30 minutes ago"
sudo bash -c '
  token="$(tr -d "\r\n" </etc/foundry/ops-token)"
  printf "header = \"Authorization: Bearer %s\"\n" "$token"
' | curl --config - --fail --silent \
  http://127.0.0.1:8765/api/snapshot >/dev/null
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  store check --output json
sudo systemctl --no-pager --full status foundry-runtime.service
sudo systemctl --no-pager --full status foundry-request-supervisor.service
sudo systemctl list-timers foundry-backup.timer
sudo systemctl start foundry-backup.service
sudo systemctl --no-pager --full status foundry-backup.service
```

Alert when Ops, runtime, or the request supervisor is inactive, the backup timer
is inactive, a service repeatedly restarts, `store check` fails, off-host upload
or verification fails, or free space falls below twice the store size.
Also alert before the newest complete green recovery challenge reaches 24 hours
of age. A backup run is successful only when its final journal message confirms
drain, remote recovery challenge, snapshot, and retention success.

## Backup and restore

The timer runs at 03:00 and 15:00 UTC with at most fifteen minutes of jitter.
Before allocating space for a new SQLite snapshot, it drains every valid local
backup without a matching marker and applies safe retention. This ordering lets
the first successful run after an outage free verified local space before a
full disk can block creation of another snapshot.

For every new or invalidated marker, Foundry performs the complete remote cycle:
immutable `upload`, source-independent `verify` returning the remote digest,
`download` into new temporary paths, digest comparison, `store check`,
disposable `store restore`, and a second `store check`. Only then does it create
the marker. The marker schema pins the backup digest, destination identity,
explicit target generation, and uploader executable SHA-256. Changing the
destination, generation, or uploader bytes forces replication and recovery
challenge again.

A missing configuration, unreachable target, non-zero uploader result, empty
or malformed verification output, digest mismatch, unsafe uploader path, or
failed recovery challenge fails the unit and preserves all local backups.
Before retention removes an expired local backup, Foundry verifies that remote
object again. Configure versioning or immutable retention on the off-host
target independently; local retention is not an off-host lifecycle policy.

The single-host target is an RPO of at most 24 hours and an operator-driven RTO
of 30 minutes. The twice-daily schedule leaves margin for jitter, but the RPO
claim is valid only when at least one complete green recovery challenge exists
within the trailing 24 hours and alerts are acted on. Alert before that window
expires. The database backup and the separately protected vault key are both
required to recover encrypted secret values; the backup service itself never
receives the vault key.

Create an operator backup before an upgrade:

```bash
backup_stamp="$(date -u +%Y%m%dT%H%M%SZ)"
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  store check --output json
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  store backup \
  --destination "/var/backups/foundry/foundry-pre-upgrade-$backup_stamp.sqlite" \
  --output json
```

The destination must be a new path. To restore:

```bash
sudo systemctl stop \
  foundry-request-supervisor.service \
  foundry-runtime.service \
  foundry-ops.service
selected_backup="/var/backups/foundry/foundry-pre-upgrade-<timestamp>.sqlite"
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  store restore \
  --source "$selected_backup" \
  --approved-by "<operator>" \
  --confirm-restore \
  --output json
sudo /usr/local/sbin/foundry-admin \
  --store /var/lib/foundry/foundry.sqlite \
  store check --output json
sudo systemctl start foundry-ops.service foundry-runtime.service
sudo systemctl start foundry-request-supervisor.service
```

Restore creates a pre-restore backup of the current store and runs SQLite
`quick_check` before promotion. Do not copy a live WAL-mode database with
ordinary `cp`.

### Off-host restore drill and RPO/RTO evidence

Run this drill at least quarterly and after changing the uploader, destination,
credential path, vault keyring, or recovery procedure. Use a disposable
recovery host, never the production store. Install the same verified Foundry
release, uploader, and separately protected vault-key files. Both executable
paths and their complete ancestry must be canonical, root-owned, non-symlink,
and not writable by group or other. Select the newest green off-host object
that existed at incident declaration. Derive its epoch from the canonical
object name with GNU `date`; obtain the incident epoch from the archived
external alert or incident-system event, never from the drill start time:

```bash
drill_dir="$(mktemp -d /tmp/foundry-restore-drill.XXXXXX)"
chmod 0700 "$drill_dir"
FOUNDRY_SECRET_VAULT_KEY_FILE=/secure/runtime/foundry-secret.key \
/usr/local/sbin/foundry-restore-drill \
  --foundry /usr/local/bin/foundry \
  --uploader /usr/local/sbin/foundry-offhost-uploader \
  --target "provider://foundry-production-backups" \
  --object "foundry-YYYYMMDDTHHMMSSZ.sqlite" \
  --object-epoch "<UTC epoch derived from that exact object name>" \
  --incident-epoch "<UTC epoch from archived external alert>" \
  --max-rpo-seconds 86400 \
  --max-rto-seconds 1800 \
  --approved-by "<operator>" \
  --canary-workflow-id "wf_<known-workflow-id>" \
  --output-dir "$drill_dir"
```

The script rejects a non-canonical object timestamp or any mismatch between the
declared object epoch and the name-derived UTC epoch. RPO is the difference
between that bound object epoch and the incident epoch. RTO is the full
wall-clock interval from the archived incident epoch until the restored store
has passed exact workflow-ID inspection, started a loopback Ops server, returned
HTTP `200` from `/api/snapshot` using a temporary token file and curl config
with mode `0600`, stopped cleanly, and passed its final check. The monotonic
`/proc/uptime` duration is retained separately as the `hot_script` metric; it is
not the production RTO threshold.

Before every uploader or Foundry subprocess, the drill recalculates the remaining
global RTO budget and enforces it with GNU `timeout`. The Ops readiness poll
uses that same budget. It requires new non-symlink output paths, a lowercase
remote digest matching the downloaded bytes, restore into a separate store,
and an exact `foundry inspect <workflow-id>` lookup. It writes
`drill-report.json` plus source-check, restore, restored-check, canary, Ops
snapshot, and final-check evidence with mode `0600`. The report pins
`foundry --version`, the Foundry executable SHA-256, and the uploader SHA-256.

Uploader authentication remains out of band. The drill removes every Foundry
vault-key variable before invoking the uploader while retaining its
`CREDENTIALS_DIRECTORY`; Foundry receives its file-based vault-key configuration
but never the uploader credential directory or off-host config variables.
Temporary Ops credentials are removed after the probe. Never put credentials
in the target or command line.

Archive the output directory, selected object name, the archived external alert
that supplied `incident_epoch`, release attestation, and operator identity. Do
not archive credential contents. A missing workflow ID, object/epoch mismatch,
negative measurement, threshold miss, watchdog timeout, reused output, digest
mismatch, Ops readiness failure, or command failure is a failed drill: keep
production readiness closed, record the cause, and repeat after remediation.

## Upgrade and rollback

1. Verify the new release checksum, Sigstore bundle, and GitHub attestation.
2. Run `packaging/smoke-production.sh` and
   `packaging/smoke-offhost-backup.sh` against the new binary.
3. Create the pre-upgrade backup shown above.
4. Stop `foundry-request-supervisor.service`, `foundry-runtime.service`, and
   `foundry-ops.service`; install the verified binary; then start Ops, runtime, and
   the request supervisor in that order.
5. Run `store check` and probe `/api/snapshot`.

If the candidate does not pass installation, verify that the installer reported
successful transactional restoration of the previous files and systemd state.
If rollback is incomplete, keep Ops, runtime and backup services isolated and
reinstall the last verified release manually. Restore the pre-upgrade store only
if the old binary cannot read the upgraded store; the restore command preserves
another recovery copy automatically.

## Incident minimum

Stop `foundry-request-supervisor.service`, `foundry-runtime.service`, and
`foundry-ops.service` before investigating suspected store corruption or secret
exposure. Preserve the store, WAL, SHM, journal excerpt, Foundry version, and
release attestation without printing decrypted secrets. Preserve the separately
protected keyring needed for recovery. Rotate exposed credentials and the store
encryption key before returning the services to use.
