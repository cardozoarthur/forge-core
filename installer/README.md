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
`SHA256SUMS.sigstore.json`, and `forge-core.cdx.json`. The installers download
the archive and checksum manifest separately, require exactly one matching
SHA-256 digest, and do not replace an installed binary if verification fails.
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

Use a v-prefixed `FORGE_VERSION`, for example `v0.5.0`. Do not set
`FORGE_RELEASE_BASE_URL` to an untrusted or plaintext endpoint in production.

## Verify the release signature

Checksum validation is mandatory in the installers. To additionally verify the
keyless release signature before installation, install
[Cosign](https://docs.sigstore.dev/cosign/system_config/installation/) and run:

```bash
cosign verify-blob \
  --bundle SHA256SUMS.sigstore.json \
  --certificate-identity-regexp \
    '^https://github\.com/cardozoarthur/forge-core/\.github/workflows/release\.yml@refs/tags/v.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  SHA256SUMS
```

GitHub's `gh attestation verify <asset> --repo cardozoarthur/forge-core` verifies
the build-provenance attestation for an individual downloaded asset.
