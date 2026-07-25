# Forge Windows Packaging

Status: delivered by `.github/workflows/release.yml`.

The release publishes an x86_64 zip containing `forge.exe` and `LICENSE`.
With `cosign` installed, `installer/install.ps1` verifies the checksum
manifest's exact Sigstore issuer/workflow/tag identity and then verifies
SHA-256 before atomically replacing `forge.exe`. Windows ARM64 is outside the
v0.5 release matrix.

The default destination is `%LOCALAPPDATA%\Forge\bin\forge.exe`. Add
`%LOCALAPPDATA%\Forge\bin` to the user `PATH` through Windows Environment
Variables and open a new terminal; the installer does not change `PATH`.
