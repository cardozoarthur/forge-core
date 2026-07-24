# Forge macOS Packaging

Status: delivered by `.github/workflows/release.yml`.

The release publishes x86_64 and Apple-silicon archives. Each archive contains
`forge` and `LICENSE`. `installer/install.sh` validates SHA-256 before
atomically replacing the binary. The release checksum manifest is signed
keylessly with Sigstore; platform notarization is not part of the v0.5
single-host profile.

The default destination is `$HOME/.local/bin/forge`. Add
`export PATH="$HOME/.local/bin:$PATH"` to `~/.zprofile` when that directory is
not already on `PATH`; the installer does not edit shell startup files.
