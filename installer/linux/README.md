# Forge Linux Packaging

Status: delivered by `.github/workflows/release.yml`.

The release publishes GNU archives for x86_64 and aarch64/ARMv8-A. They require
Linux kernel 4.18 or newer and glibc 2.34 or newer. Both archives are built and
executed on native Ubuntu 22.04 runners; release CI rejects ELF symbol
requirements newer than `GLIBC_2.34`.

Each archive contains `forge`, `LICENSE`, the systemd service bundle, and the
single-host production runbook. `installer/install.sh` verifies the archive
against the release checksum manifest before atomically replacing the binary.
It installs to `$HOME/.local/bin` by default but does not modify `PATH`; see
`installer/README.md` for shell-specific activation.

For a long-lived single-host deployment, use the systemd unit and runbook under
`packaging/systemd` and `docs/production-single-host.md`. That profile requires
systemd 249 or newer.
