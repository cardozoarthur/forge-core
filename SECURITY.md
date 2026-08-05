# Security policy

## Supported versions

Foundry v0.6 is supported for the single-host, trusted-operator production profile
documented in `docs/production-single-host.md`. Security fixes are released only
for the latest published version.

| Version | Supported |
| --- | --- |
| Latest GitHub release | Yes |
| Older releases | No |
| Unreleased snapshots | No |

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Use the repository's
[private vulnerability reporting](https://github.com/cardozoarthur/foundry-core/security/advisories/new)
flow and include:

- the affected Foundry version and operating system;
- the minimal steps needed to reproduce the problem;
- the expected and observed security boundary;
- whether secrets, workflow state, or release artifacts may be exposed.

The maintainers will acknowledge a complete report within three business days,
provide an initial severity assessment within seven business days, and
coordinate disclosure after a fix is available.

## Production security boundary

- Run the Ops service under the dedicated `foundry` system account.
- Keep the SQLite store and backup files at mode `0600` inside directories at
  mode `0700`.
- Enable `FOUNDRY_PRODUCTION_MODE=1`, load vault and Ops bearer secrets from
  root-owned `0600` files through systemd credentials, and never use inline
  secret environment values.
- Bind the authenticated built-in Ops HTTP service to loopback. Remote access
  requires a separately managed TLS and authentication boundary.
- Install only release assets that match `SHA256SUMS`. Releases also publish a
  Sigstore bundle and GitHub build-provenance attestations.
- Never place vault keys, provider tokens, or decrypted secrets in the
  repository, service unit, command line, or support report.

If secret material may have been exposed, stop the service, preserve the store
for investigation, rotate the affected credentials, and restore service only
after the store encryption key and release provenance have been verified.
