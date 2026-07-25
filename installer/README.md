# Forge installers

Forge v0.5 is distributed as verified archives for Linux, macOS, and Windows.
The scripts install the same `forge` runtime on every platform.

## Supported release assets

| Platform | Architecture | Asset |
| --- | --- | --- |
| Linux (GNU) | x86_64 | `forge-linux-x86_64.tar.gz` |
| Linux (GNU) | aarch64/ARMv8-A | `forge-linux-aarch64.tar.gz` |
| macOS | x86_64 | `forge-macos-x86_64.tar.gz` |
| macOS | Apple silicon | `forge-macos-aarch64.tar.gz` |
| Windows | x86_64 | `forge-windows-x86_64.zip` |

Every release also contains `SHA256SUMS`,
`SHA256SUMS.sigstore.json`, and `forge-core.cdx.json`. The installers first
verify the checksum manifest's Sigstore bundle against the exact repository,
release workflow, tag, and GitHub Actions OIDC issuer. Only then do they
download the archive, require exactly one matching SHA-256 digest, and
atomically replace the installed binary. A missing or altered bundle, identity,
issuer, tag, checksum, or archive leaves the existing binary unchanged.
Archive entry order, ownership metadata, modes, and timestamps are normalized
from the tagged commit epoch. Every build job creates each archive twice and
requires byte-for-byte equality before upload.

## Linux compatibility floor

The supported CLI floor is a 64-bit Linux kernel 4.18 or newer with glibc 2.34
or newer on x86_64 or ARMv8-A. Both Linux archives are built and executed on
native Ubuntu 22.04 runners. Release CI inspects their ELF version requirements
and rejects any symbol newer than `GLIBC_2.34`.

The bundled production service profile additionally requires systemd 249 or
newer. Ubuntu 22.04 LTS and compatible newer distributions meet that tested
service baseline.

## Install

Install
[Cosign](https://docs.sigstore.dev/cosign/system_config/installation/) first.
The installers intentionally fail closed when `cosign` is unavailable because
the checksum manifest is not a trust root by itself.

Linux or macOS:

```bash
curl --proto '=https' --tlsv1.2 -fsSLO \
  https://github.com/cardozoarthur/forge-core/releases/latest/download/install.sh
bash install.sh
```

The default destination is `$HOME/.local/bin/forge`. The installer deliberately
does not edit shell startup files. If `$HOME/.local/bin` is not already in
`PATH`, add this line to `~/.profile` on Linux or `~/.zprofile` on macOS, then
start a new login shell:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

For the current shell, run the same `export` directly. Confirm the selected
binary with `command -v forge` and `forge --version`.

Windows PowerShell:

```powershell
Invoke-WebRequest `
  https://github.com/cardozoarthur/forge-core/releases/latest/download/install.ps1 `
  -OutFile install.ps1
.\install.ps1
```

The Windows default is `%LOCALAPPDATA%\Forge\bin\forge.exe`. The installer does
not mutate the user `PATH`; add `%LOCALAPPDATA%\Forge\bin` through the Windows
Environment Variables UI, open a new terminal, and verify with:

```powershell
Get-Command forge
forge --version
```

When `FORGE_BIN_DIR` is set, add that exact directory instead of the default.

## Script overrides

- `FORGE_REPO`
- `FORGE_VERSION`
- `FORGE_PREFIX`
- `FORGE_BIN_DIR`
- `FORGE_RELEASE_BASE_URL` for an explicitly selected release mirror

Use a v-prefixed `FORGE_VERSION`, for example `v0.5.3`. A custom
`FORGE_RELEASE_BASE_URL` requires an explicit version so the expected Sigstore
certificate identity contains one exact tag. Release URLs must use HTTPS.
`FORGE_INSTALLER_TEST_MODE=1` permits HTTP solely for isolated installer
fixtures and must never be set during a real installation.

## Verify the release signature

The installers perform this verification automatically before reading a digest
from `SHA256SUMS`. For an explicit `v0.5.3` manual verification, run:

```bash
cosign verify-blob \
  --bundle SHA256SUMS.sigstore.json \
  --certificate-identity \
    https://github.com/cardozoarthur/forge-core/.github/workflows/release.yml@refs/tags/v0.5.3 \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  SHA256SUMS
```

GitHub's `gh attestation verify <asset> --repo cardozoarthur/forge-core` verifies
the build-provenance attestation for an individual downloaded asset.
The maintainer-side signed-tag and release configuration is documented in
the
[release trust contract](https://github.com/cardozoarthur/forge-core/blob/main/docs/release-security.md).
