# Foundry installers

Foundry v0.6 is distributed as verified archives for Linux, macOS, and Windows.
The scripts install the same canonical `foundry` runtime on every platform.
<!-- foundry-brand-allow: legacy-compat -->
During the 0.6.x migration window, archives also contain the deprecated `forge`
shim. The installers place it beside `foundry`; it prints a deprecation warning
to stderr and forwards the invocation. It is not a second canonical runtime.

## Supported release assets

| Platform | Architecture | Asset |
| --- | --- | --- |
| Linux (GNU) | x86_64 | `foundry-linux-x86_64.tar.gz` |
| Linux (GNU) | aarch64/ARMv8-A | `foundry-linux-aarch64.tar.gz` |
| macOS | x86_64 | `foundry-macos-x86_64.tar.gz` |
| macOS | Apple silicon | `foundry-macos-aarch64.tar.gz` |
| Windows | x86_64 | `foundry-windows-x86_64.zip` |

Every release also contains `SHA256SUMS`,
`SHA256SUMS.sigstore.json`, and `foundry-core.cdx.json`. The installers first
verify the checksum manifest's Sigstore bundle against the exact repository,
release workflow, tag, and GitHub Actions OIDC issuer. Only then do they
download the archive, require exactly one matching SHA-256 digest, and
atomically replace each installed executable. A missing or altered bundle,
identity, issuer, tag, checksum, or archive leaves existing executables unchanged.
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
  https://github.com/cardozoarthur/foundry-core/releases/latest/download/install.sh
bash install.sh
```

The default destination is `$HOME/.local/bin/foundry`. The installer deliberately
does not edit shell startup files. If `$HOME/.local/bin` is not already in
`PATH`, add this line to `~/.profile` on Linux or `~/.zprofile` on macOS, then
start a new login shell:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

For the current shell, run the same `export` directly. Confirm the selected
binary with `command -v foundry` and `foundry --version`. New automation must
invoke `foundry` directly.

Windows PowerShell:

```powershell
Invoke-WebRequest `
  https://github.com/cardozoarthur/foundry-core/releases/latest/download/install.ps1 `
  -OutFile install.ps1
.\install.ps1
```

The Windows default is `%LOCALAPPDATA%\Foundry\bin\foundry.exe`. The installer does
not mutate the user `PATH`; add `%LOCALAPPDATA%\Foundry\bin` through the Windows
Environment Variables UI, open a new terminal, and verify with:

```powershell
Get-Command foundry
foundry --version
```

When `FOUNDRY_BIN_DIR` is set, add that exact directory instead of the default.

## Script overrides

- `FOUNDRY_REPO`
- `FOUNDRY_VERSION`
- `FOUNDRY_PREFIX`
- `FOUNDRY_BIN_DIR`
- `FOUNDRY_RELEASE_BASE_URL` for an explicitly selected release mirror

<!-- foundry-brand-allow: legacy-compat -->
During the temporary migration window, the matching `FORGE_*` installer
variables remain accepted only when their canonical `FOUNDRY_*` counterpart is
unset. New automation must use `FOUNDRY_*`; all installed binaries and release
assets use Foundry as their canonical identity. The temporary shim described
above is the sole old-name exception.

Use a v-prefixed `FOUNDRY_VERSION`, for example `v0.6.0`. A custom
`FOUNDRY_RELEASE_BASE_URL` requires an explicit version so the expected Sigstore
certificate identity contains one exact tag. Release URLs must use HTTPS.
`FOUNDRY_INSTALLER_TEST_MODE=1` permits HTTP solely for isolated installer
fixtures and must never be set during a real installation.

## Verify the release signature

The installers perform this verification automatically before reading a digest
from `SHA256SUMS`. For an explicit `v0.6.0` manual verification, run:

```bash
cosign verify-blob \
  --bundle SHA256SUMS.sigstore.json \
  --certificate-identity \
    https://github.com/cardozoarthur/foundry-core/.github/workflows/release.yml@refs/tags/v0.6.0 \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  SHA256SUMS
```

GitHub's `gh attestation verify <asset> --repo cardozoarthur/foundry-core` verifies
the build-provenance attestation for an individual downloaded asset.
The maintainer-side signed-tag and release configuration is documented in
the
[release trust contract](https://github.com/cardozoarthur/foundry-core/blob/main/docs/release-security.md).
