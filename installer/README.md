# Forge Installer

This directory captures the distribution contract for Forge across Linux, macOS and Windows.

## Installation goals

- Keep `forge` as the shell-visible entrypoint.
- Preserve the same product semantics across all platforms.
- Support install, update and version pinning with one contract.

## Packaging targets

- Linux: tarball, package manager or single-binary install path
- macOS: signed binary or archive-based install path
- Windows: executable plus installer or archive-based install path

## Contract rule

The installer should deploy the same Forge runtime and not fork behavior by platform.

## Release layout

The shell installers expect GitHub release assets named like:

- `forge-linux-x86_64.tar.gz`
- `forge-linux-aarch64.tar.gz`
- `forge-macos-x86_64.tar.gz`
- `forge-macos-aarch64.tar.gz`
- `forge-windows-x86_64.zip`
- `forge-windows-aarch64.zip`

## Script overrides

- `FORGE_REPO`
- `FORGE_VERSION`
- `FORGE_PREFIX`
- `FORGE_BIN_DIR`

The scripts install the shell-visible `forge` binary into the target bin directory and keep the runtime semantics identical across platforms.
