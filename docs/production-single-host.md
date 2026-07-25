# Forge v0.5 single-host production runbook

## Supported profile

Forge v0.5 supports production operation by one trusted operator on one Linux
host. The Forge runtime, SQLite store, local Ops service, verified backups, and
executor policy remain on that host.

The release binary targets GNU Linux on x86_64 or ARMv8-A and requires Linux
kernel 4.18 or newer plus glibc 2.34 or newer. The supported service profile
also requires systemd 249 or newer. Ubuntu 22.04 LTS is the tested floor;
compatible newer distributions are supported.

The following are outside the v0.5 production profile:

- public exposure of the built-in Ops HTTP server;
- multi-tenant or untrusted-user operation;
- high availability, active-active replicas, or a shared network store;
- Kubernetes or Knative installation managed by Forge;
- experimental `sdk/` language stubs as a supported integration contract;
- macOS platform notarization and Windows ARM64 packages.

## Install the service

Download a release archive, `SHA256SUMS`, and
`SHA256SUMS.sigstore.json`. Verify the archive checksum and, for a release
promotion, the Sigstore bundle as described in `installer/README.md`. Then:

```bash
tar -xzf forge-linux-x86_64.tar.gz
sudo bash packaging/systemd/install-service.sh \
  "$PWD/forge" \
  /usr/local/sbin/forge-offhost-uploader \
  "provider://forge-production-backups" \
  "production-account-2026-07"
```

The second argument is an operator-owned executable, the third is an opaque,
non-secret destination, and the fourth is an explicit non-secret target
generation. Bump the generation whenever the remote account, credential,
bucket generation, retention domain, or trust boundary changes, even when the
destination text stays the same. The installer refuses to enable production
without all three values. It stores them as root-owned `0600` files at
`/etc/forge/backup-offhost-command` and
`/etc/forge/backup-offhost-destination`, and
`/etc/forge/backup-offhost-generation`; systemd delivers their contents to the
backup service as credentials.

The executable and every parent path component through `/` must be root-owned,
canonical, non-symlink, and not writable by group or other. Both the installer
and runtime reject an unsafe path chain. The backup unit deliberately does not
load the Forge vault key, and the wrapper removes any inherited Forge vault-key
variables before invoking the uploader. The uploader receives only its own
operator-configured authentication mechanism.

The uploader contract is deliberately transport-neutral. Forge invokes the
executable three times for every complete recovery challenge:

```text
UPLOADER upload --source LOCAL_PATH --destination TARGET --object NAME --sha256 HEX
UPLOADER verify --destination TARGET --object NAME
UPLOADER download --destination TARGET --object NAME --output LOCAL_PATH --sha256-output DIGEST_PATH
```

`upload` must durably create the remote object without replacing an existing
object. `verify` must consult the stored remote bytes without receiving the
local source or expected digest, then print exactly one lowercase SHA-256 and
no other stdout. Forge validates its format and equality; exit status zero
alone is never success. Both operations must be idempotent and must return
non-zero on an incomplete or ambiguous result. The uploader must persist the
digest as immutable object metadata or a sidecar. `download`, used for every
recovery challenge and operator drill, must retrieve both the object and that
persisted digest without depending on the original host's journal.

Do not put passwords, access keys, bearer tokens, signed URLs, or other
credentials in `TARGET`. The uploader must obtain authentication out of band,
for example from workload identity or an additional systemd credential:

```ini
# sudo systemctl edit forge-backup.service
[Service]
LoadCredential=forge-offhost-auth:/etc/forge/offhost-auth
```

The uploader can then read
`$CREDENTIALS_DIRECTORY/forge-offhost-auth`. Keep that credential under
operator-controlled rotation and never print it. After adding or rotating a
credential, bump the target generation and rerun the installer so the new
identity must pass promotion again.

The uploader retains `CREDENTIALS_DIRECTORY`, but every uploader invocation
removes Forge vault and Ops token variables. Conversely, every Forge invocation
from the backup or restore path removes `CREDENTIALS_DIRECTORY` and all
off-host configuration-file variables. This reciprocal boundary prevents
either executable from inheriting the other's authority.

Installation is a fail-closed go-live gate. It disables Ops and the timer,
starts Ops only long enough to initialize or open the store, and requires an
HTTP `200` from an authenticated loopback `/api/snapshot` probe before stopping
Ops and running `forge-backup.service`. That first backup must complete remote
upload, digest-only verification, download, `store check`, disposable restore,
and a second `store check`. After promotion, the installer starts Ops and
requires the same authenticated probe again before starting the timer. If any
step fails, both remain stopped and disabled.

Before replacing any managed key, configuration, helper, unit, or binary, the
installer snapshots the previous files and records whether Ops, backup, and the
timer were enabled or active. A failed initialization, recovery challenge,
authenticated readiness probe, timer start, or final status check triggers a
transactional rollback: the candidate services are stopped, previous files and
unit enablement are restored, `daemon-reload` runs, and only services that were
previously active are restarted. Treat an explicit “rollback incomplete”
message as an incident and keep the host isolated.

The installer creates a locked `forge` service account and installs:

- binary: `/usr/local/bin/forge`;
- store: `/var/lib/forge/forge.sqlite`;
- project root: `/var/lib/forge/workspace`;
- backups: `/var/backups/forge`;
- off-host uploader and non-secret destination:
  `/etc/forge/backup-offhost-command` and
  `/etc/forge/backup-offhost-destination`;
- off-host target generation: `/etc/forge/backup-offhost-generation`;
- vault key: `/etc/forge/secret.key`, delivered through a systemd credential;
- Ops bearer token: `/etc/forge/ops-token`, delivered through a separate
  systemd credential;
- loopback Ops endpoint: `http://127.0.0.1:8765`;
- systemd services: `forge-ops.service` and `forge-backup.timer`.
- credential-aware admin wrapper: `/usr/local/sbin/forge-admin`.
- fail-fast restore drill: `/usr/local/sbin/forge-restore-drill`.

The Ops unit has `UMask=0077`, a read-only host filesystem except for the Forge
state directory, no Linux capabilities, restricted kernel surfaces and address
families, and automatic restart on failure. The backup unit is stricter:
`/var/lib/forge` is read-only and only `/var/backups/forge` is writable, so an
uploader cannot corrupt the live store. Keep Ops bound to loopback. Use an SSH
tunnel or a separately managed authenticated TLS reverse proxy for remote
operator access.

`forge-backup.service` is a bounded oneshot with
`TimeoutStartSec=30min`. A hung uploader, verification, download, or restore
challenge therefore fails the unit instead of blocking the timer indefinitely.

The installer generates a 32-byte hexadecimal vault key with mode `0600` when
`/etc/forge/secret.key` does not exist. The unit exposes only the systemd
credential path through `FORGE_SECRET_VAULT_KEY_FILE`; it never places key
material in the unit or environment. Existing installations may instead
provision that file from their secret manager before running the installer.

Back up the vault key separately under encryption and independent access
control. Never publish it, place it in a release, or store it next to a
publicly accessible database backup. A restored database requires its current
key or a deliberately configured previous key through
`FORGE_SECRET_VAULT_PREVIOUS_KEYS` or
`FORGE_SECRET_VAULT_PREVIOUS_KEY_FILES`.

It also generates an independent bearer token at `/etc/forge/ops-token`. In
production mode, the Ops server refuses an inline token and reads only the
root-owned systemd credential. Do not set `FORGE_OPS_ALLOW_REMOTE`; the
supported unit remains loopback-only.

## Fast production simulation

The bounded smoke replaces a multi-day soak for the v0.5 release decision. It
creates a temporary store, verifies private file modes, performs a real SQLite
backup and restore, proves unauthenticated mutation is rejected, probes the
authenticated Ops endpoint, kills the process with `SIGKILL`, checks the store,
restarts it, and finishes with `SIGTERM`:

```bash
cargo build --release
bash packaging/smoke-production.sh target/release/forge
bash packaging/smoke-offhost-backup.sh target/release/forge
```

These smokes are release gates, not evidence of high availability or
multi-tenant safety.

### Fail-closed production-readiness decision

Capability completion and operational production readiness are separate gates.
Inspect the non-mutating evidence plan first:

```bash
forge milestone production-plan --version 0.5 --output json
```

After collecting fresh, secret-free evidence under one operator-controlled
directory, evaluate it without running commands or changing infrastructure:

```bash
forge milestone production-readiness \
  --version 0.5 \
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
Place an approved, forge-owned Git checkout below
`/var/lib/forge/workspace/<project>`; the admin wrapper permits writes only in
the managed Forge directories. Keep every JSON response because IDs, revisions,
receipt hashes and timestamps are part of the 14th production receipt.

```bash
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission start \
  --goal "<bounded production-candidate objective>" \
  --squad software-factory \
  --worktree /var/lib/forge/workspace/<project> \
  --output json

sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  context --workflow <workflow-id> --task <task-id> \
  --project-root /var/lib/forge/workspace/<project> \
  --budget 4096 --strict --view compact --output json

sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission drive <mission-id> --output json
```

Read the projected task ID from the `start` or latest `inspect` response and
request strict context while that task is still pending. Continue only when it
reports `handoff_ready=true` and `guardrail.status=ready`. Then call `drive`,
require the assignment to use that same task ID, retain its agent ID, request
the evidence kinds named by the assignment, and repeat `--command` for each
argument:

```bash
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission execute <mission-id> \
  --task <task-id> --agent <agent-id> \
  --idempotency-key <unique-execution-key> \
  --purpose test --approved-by <operator> \
  --evidence <required-kind> \
  --command <executable> --command <argument> \
  --output json

sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission submit <mission-id> \
  --task <task-id> --agent <agent-id> \
  --idempotency-key <unique-submission-key> \
  --receipt-id <execution-receipt-id> \
  --summary "<validated bounded result>" \
  --output json

sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission resume <mission-id> --output json
```

For the production receipt, use a resume response with
`action=handoff_consumed`; `mission_completed` is the later terminal response,
not a substitute for the consumed-handoff evidence. Repeat the loop for each
assignment and any `repair_created` action, then inspect both ledgers and the
projected workflow:

```bash
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission inspect <mission-id> --output json
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mission execution list --mission <mission-id> --output json
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  validate --workflow <workflow-id> --output json
```

The typed lifecycle bundle must be `forge.milestone.mission_lifecycle.v1` and
bind the exact canonical inventory schema, numbers `1` through `40` and
inventory SHA-256 to:

- a completed, attempted and executed
  `forge.mission.execution_receipt.v3` with exit code zero;
- the initial `forge.mission.submit.v1` report in `queued` state, linked to that
  receipt and carrying non-empty handoff and inbox IDs;
- a later `forge.mission.drive.v1` resume report with
  `action=handoff_consumed`, the same handoff accepted, and its inbox consumed;
- distinct canonical receipt digests and ordered execute, submit and resume
  timestamps.

Generate the typed bundle from the persisted store-backed records; do not
hand-author hashes or treat captured terminal text as the receipt. Create the
operator-controlled evidence root, then run:

```bash
sudo install -d -m 0700 -o forge -g forge /var/lib/forge/evidence
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  milestone production-mission-evidence \
  --mission <mission-id> \
  --receipt <execution-receipt-id> \
  --evidence-root /var/lib/forge/evidence \
  --artifact mission-operational-lifecycle.json \
  --release-version 0.5.3 \
  --output json
```

The command serializes exactly the typed artifact bytes, writes them below the
evidence root, verifies their SHA-256 against the returned `manifest_section`
and prints the complete package. Copy that `manifest_section` into
`production-readiness.json`; do not recalculate or hand-edit its claims. The
artifact path must be relative and every parent remains inside the evidence
root. Then evaluate against the same production store:

```bash
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  milestone production-readiness \
  --version 0.5 \
  --manifest /var/lib/forge/evidence/production-readiness.json \
  --evidence-root /var/lib/forge/evidence \
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
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
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
recovery through Forge commands; never edit production SQLite rows manually.

For a local MCP client, start the stdio JSON-RPC server as a child process:

```bash
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  mcp serve
```

The server supports MCP `initialize`, `ping`, `tools/list`, and `tools/call`.
Keep stdio attached to the trusted local client; it is not a network listener.

## Routine checks

```bash
sudo systemctl --no-pager --full status forge-ops.service
sudo journalctl -u forge-ops.service --since "30 minutes ago"
sudo bash -c '
  token="$(tr -d "\r\n" </etc/forge/ops-token)"
  printf "header = \"Authorization: Bearer %s\"\n" "$token"
' | curl --config - --fail --silent \
  http://127.0.0.1:8765/api/snapshot >/dev/null
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  store check --output json
sudo systemctl list-timers forge-backup.timer
sudo systemctl start forge-backup.service
sudo systemctl --no-pager --full status forge-backup.service
```

Alert on a failed service, repeated restart, failed store check, backup timer
failure, off-host upload or verification failure, or less than twice the store
size in free space. Also alert before the newest complete green recovery
challenge reaches 24 hours of age. A backup run is successful only when its
final journal message confirms drain, remote recovery challenge, snapshot, and
retention success.

## Backup and restore

The timer runs at 03:00 and 15:00 UTC with at most fifteen minutes of jitter.
Before allocating space for a new SQLite snapshot, it drains every valid local
backup without a matching marker and applies safe retention. This ordering lets
the first successful run after an outage free verified local space before a
full disk can block creation of another snapshot.

For every new or invalidated marker, Forge performs the complete remote cycle:
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
Before retention removes an expired local backup, Forge verifies that remote
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
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  store check --output json
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  store backup \
  --destination "/var/backups/forge/forge-pre-upgrade-$backup_stamp.sqlite" \
  --output json
```

The destination must be a new path. To restore:

```bash
sudo systemctl stop forge-ops.service
selected_backup="/var/backups/forge/forge-pre-upgrade-<timestamp>.sqlite"
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  store restore \
  --source "$selected_backup" \
  --approved-by "<operator>" \
  --confirm-restore \
  --output json
sudo /usr/local/sbin/forge-admin \
  --store /var/lib/forge/forge.sqlite \
  store check --output json
sudo systemctl start forge-ops.service
```

Restore creates a pre-restore backup of the current store and runs SQLite
`quick_check` before promotion. Do not copy a live WAL-mode database with
ordinary `cp`.

### Off-host restore drill and RPO/RTO evidence

Run this drill at least quarterly and after changing the uploader, destination,
credential path, vault keyring, or recovery procedure. Use a disposable
recovery host, never the production store. Install the same verified Forge
release, uploader, and separately protected vault-key files. Both executable
paths and their complete ancestry must be canonical, root-owned, non-symlink,
and not writable by group or other. Select the newest green off-host object
that existed at incident declaration. Derive its epoch from the canonical
object name with GNU `date`; obtain the incident epoch from the archived
external alert or incident-system event, never from the drill start time:

```bash
drill_dir="$(mktemp -d /tmp/forge-restore-drill.XXXXXX)"
chmod 0700 "$drill_dir"
FORGE_SECRET_VAULT_KEY_FILE=/secure/runtime/forge-secret.key \
/usr/local/sbin/forge-restore-drill \
  --forge /usr/local/bin/forge \
  --uploader /usr/local/sbin/forge-offhost-uploader \
  --target "provider://forge-production-backups" \
  --object "forge-YYYYMMDDTHHMMSSZ.sqlite" \
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

Before every uploader or Forge subprocess, the drill recalculates the remaining
global RTO budget and enforces it with GNU `timeout`. The Ops readiness poll
uses that same budget. It requires new non-symlink output paths, a lowercase
remote digest matching the downloaded bytes, restore into a separate store,
and an exact `forge inspect <workflow-id>` lookup. It writes
`drill-report.json` plus source-check, restore, restored-check, canary, Ops
snapshot, and final-check evidence with mode `0600`. The report pins
`forge --version`, the Forge executable SHA-256, and the uploader SHA-256.

Uploader authentication remains out of band. The drill removes every Forge
vault-key variable before invoking the uploader while retaining its
`CREDENTIALS_DIRECTORY`; Forge receives its file-based vault-key configuration
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
4. Stop `forge-ops.service`, install the verified binary, and start the service.
5. Run `store check` and probe `/api/snapshot`.

If the candidate does not pass installation, verify that the installer reported
successful transactional restoration of the previous files and systemd state.
If rollback is incomplete, keep both services isolated and reinstall the last
verified release manually. Restore the pre-upgrade store only if the old binary
cannot read the upgraded store; the restore command preserves another recovery
copy automatically.

## Incident minimum

Stop the service before investigating suspected store corruption or secret
exposure. Preserve the store, WAL, SHM, journal excerpt, Forge version, and
release attestation without printing decrypted secrets. Preserve the separately
protected keyring needed for recovery. Rotate exposed credentials and the store
encryption key before returning the service to use.
