# Foundry Windows Packaging

Status: delivered by `.github/workflows/release.yml`.

The release publishes an x86_64 zip containing `foundry.exe`, the temporary
compatibility shim, and `LICENSE`.
<!-- foundry-brand-allow: legacy-compat -->
`forge.exe` is deprecated, warns on stderr, and exists only for the 0.6.x
migration window.
With `cosign` installed, `installer/install.ps1` verifies the checksum
manifest's exact Sigstore issuer/workflow/tag identity and then verifies
SHA-256 before atomically replacing `foundry.exe`. Windows ARM64 is outside the
v0.6 release matrix.

The default destination is `%LOCALAPPDATA%\Foundry\bin\foundry.exe`. Add
`%LOCALAPPDATA%\Foundry\bin` to the user `PATH` through Windows Environment
Variables and open a new terminal; the installer does not change `PATH`.
