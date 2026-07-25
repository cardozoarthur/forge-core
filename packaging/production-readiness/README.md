# Forge production-readiness drills

This bundle produces fresh source artifacts for the `bounded_load` and
`upgrade_rollback` gates of the supported single-host Linux profile. The drills
do not edit the production store: they open it read-only, create a consistent
SQLite backup, and perform every mutation against a private disposable copy.

The reports are inputs to the production-evidence assembler. They are not
canonical receipts and do not make Forge production-ready by themselves.
Each report uses the strict source-attestation envelope
`forge.milestone.production_source_evidence.<kind>.v1`: release identity and
execution mode stay at the top level, manifest fields live only under
`claims`, and collector-specific hashes and canary details live only under
`evidence`.

Only a report with `execution_mode: "production"` is eligible as production
source evidence. Offline self-tests emit `execution_mode: "test"` and must
never be copied into a production evidence draft or promoted. Test mode is
rejected whenever `FORGE_PRODUCTION_MODE` is any case-insensitive enabled
value (`1`, `true`, `yes`, or `on`); unknown production-mode values also fail
closed.

## Requirements

- GNU/Linux with Bash, Python 3, GNU `timeout`, OpenSSL, curl, and the standard
  GNU core utilities;
- canonical root-owned Forge binaries and drill scripts whose path components
  are not writable by group or other;
- an absolute `0600` source store readable by the `forge` service user;
- an existing absolute `0700` output directory owned by `forge`;
- `/etc/forge/secret.key` available to the system service manager for
  `LoadCredential`; the drill receives only the runtime credential copy;
- a real workflow ID that is safe to use as the persistence canary.

Inline vault-key values are rejected. Raw Forge, Ops, and canary responses stay
inside a temporary `0700` directory and are removed; only the fixed,
secret-free report schema is published.

Install the verified drill scripts at fixed root-owned paths:

```bash
sudo install -d -m 0755 -o root -g root /usr/local/libexec
sudo install -m 0755 -o root -g root \
  packaging/production-readiness/forge-bounded-load-drill \
  packaging/production-readiness/forge-upgrade-rollback-drill \
  /usr/local/libexec/
```

The production examples below use fixed transient system-unit names. `sudo`
authorizes only creation of the transient unit; the drill process itself runs
as `forge:forge`. `--collect` unloads the unit after completion, while an
already-active unit with the same name makes a concurrent invocation fail
instead of starting a second drill.

## Bounded load

```bash
sudo systemd-run \
  --unit=forge-bounded-load-drill.service \
  --service-type=exec \
  --wait \
  --collect \
  --pipe \
  --property=User=forge \
  --property=Group=forge \
  --property=LoadCredential=forge-secret-key:/etc/forge/secret.key \
  --setenv=FORGE_PRODUCTION_MODE=1 \
  --setenv=FORGE_SECRET_VAULT_KEY_FILE=/run/credentials/forge-bounded-load-drill.service/forge-secret-key \
  /usr/local/libexec/forge-bounded-load-drill \
  --forge /usr/local/bin/forge \
  --store /var/lib/forge/forge.sqlite \
  --release-version 0.5.3 \
  --canary-workflow-id wf_REPLACE_WITH_KNOWN_WORKFLOW_ID \
  --output-dir /var/lib/forge/evidence \
  --operations 120 \
  --concurrency 4 \
  --max-duration-seconds 120 \
  --max-rss-bytes 536870912 \
  --ops-port 18767
```

The drill performs at least 100 authenticated loopback Ops reads, measures p95
latency, wraps the load driver in GNU `timeout`, and verifies the measured
duration. It samples Ops `VmRSS` from `/proc` every 20 ms, kills Ops on an
observed breach, and requires the monitor itself to exit cleanly before it
asserts `resource_limit_enforced`. This is a sampled process-RSS gate, not a
cgroup peak-memory guarantee. The drill then sends `SIGKILL`, restarts Ops,
rechecks the canary, and runs final store checks. It publishes
`bounded-load-report.json` only after every invariant passes. The report also
binds the installed binary SHA-256 and canary workflow ID used by the drill.

## Upgrade and rollback

Run this against a store or pre-upgrade backup that the previous verified
binary and the `forge` service user can read:

```bash
sudo systemd-run \
  --unit=forge-upgrade-rollback-drill.service \
  --service-type=exec \
  --wait \
  --collect \
  --pipe \
  --property=User=forge \
  --property=Group=forge \
  --property=LoadCredential=forge-secret-key:/etc/forge/secret.key \
  --setenv=FORGE_PRODUCTION_MODE=1 \
  --setenv=FORGE_SECRET_VAULT_KEY_FILE=/run/credentials/forge-upgrade-rollback-drill.service/forge-secret-key \
  /usr/local/libexec/forge-upgrade-rollback-drill \
  --candidate /usr/local/bin/forge \
  --previous /opt/forge/releases/0.5.2/forge \
  --store /var/backups/forge/forge-pre-upgrade-YYYYMMDDTHHMMSSZ.sqlite \
  --release-version 0.5.3 \
  --canary-workflow-id wf_REPLACE_WITH_KNOWN_WORKFLOW_ID \
  --output-dir /var/lib/forge/evidence \
  --max-duration-seconds 120 \
  --ops-port 18768
```

The drill verifies the previous binary, upgrades a disposable copy with the
candidate, probes authenticated Ops, restores the verified baseline with the
previous binary, proves the previous schema and canary are intact, then
reinstalls the candidate and requires the resulting schema fingerprint to
match the first upgrade. It publishes `upgrade-rollback-report.json` only after
the complete cycle succeeds. The audit fields bind the previous version, both
binary SHA-256 values, the canary ID, the immutable baseline-backup digest, and
the baseline, upgraded, rolled-back, and reinstalled schema fingerprints. The
previous base semantic version must be strictly older than the candidate.
Here `rollback_completed` means the restore command succeeded and the restored
copy passed SQLite integrity, canary inspection, and baseline-schema equality;
it does not claim a row-by-row logical comparison beyond those checks.

Both report names are fixed, mode `0600`, and must not already exist. Existing
files and symlinks fail closed.

## Offline validation

The self-test uses only temporary stores and local stubs; it never uses sudo,
system services, AWS, or the production store. Its reports are deliberately
marked `execution_mode: "test"` and are not promotable:

```bash
bash -n packaging/production-readiness/forge-bounded-load-drill
bash -n packaging/production-readiness/forge-upgrade-rollback-drill
bash -n packaging/production-readiness/tests/self-test.sh
bash packaging/production-readiness/tests/self-test.sh
```
