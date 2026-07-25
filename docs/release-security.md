# Forge release trust contract

Forge `v0.5.3` and later releases fail closed at two independent trust
boundaries:

1. the release workflow accepts only an annotated SSH-signed tag made by the
   configured Ed25519 key and exact signer/tagger identities;
2. installers accept `SHA256SUMS` only after its Sigstore bundle verifies for
   the exact repository, workflow file, release tag, and GitHub Actions issuer.

Unsigned legacy tags are not grandfathered into this contract and cannot be
republished by the current workflow.

## Configure the signed-tag root

Configure these GitHub repository values before creating `v0.5.3`:

| Kind | Name | Exact value |
| --- | --- | --- |
| Variable | `FORGE_RELEASE_TAG_SIGNER_PRINCIPAL` | Exact SSH signing principal, normally the maintainer email |
| Variable | `FORGE_RELEASE_TAG_SIGNER_SSH_PUBLIC_KEY` | One `ssh-ed25519 AAAA...` public-key line |
| Variable | `FORGE_RELEASE_TAGGER_NAME` | Exact annotated-tag `taggername` |
| Variable | `FORGE_RELEASE_TAGGER_EMAIL` | Exact annotated-tag email without angle brackets |

Derive the public key from the configured private key:

```bash
ssh-keygen -y -f "$HOME/.ssh/id_ed25519"
```

The workflow normalizes that line to its key type and key bytes, validates it
with `ssh-keygen`, and creates an isolated `allowedSigners` file containing
only the configured principal and key. It verifies the SSH tag signature,
compares tagger name and email separately, fetches `origin/main`, and requires
the release commit to be an ancestor of that branch. Missing configuration is
a hard failure.

## Create `v0.5.3`

Push the release commit to `main` before pushing the tag:

```bash
git \
  -c gpg.format=ssh \
  -c user.signingkey="$HOME/.ssh/id_ed25519" \
  tag -s v0.5.3 -m "Forge v0.5.3"
git push origin main
git push origin v0.5.3
```

Confirm the two tagger values before the push:

```bash
git for-each-ref \
  --format='name=%(taggername)%0aemail=%(taggeremail)' \
  refs/tags/v0.5.3
```

The release workflow refuses lightweight tags, unsigned annotated tags,
signatures from any other SSH key or principal, tagger identity drift, tags for
another crate version, detached release commits, and attempts to replace an
existing GitHub release.

## Installer verification

The release job signs `SHA256SUMS` keylessly with GitHub OIDC. For `v0.5.3`,
the only accepted certificate claims are:

```text
issuer=https://token.actions.githubusercontent.com
identity=https://github.com/cardozoarthur/forge-core/.github/workflows/release.yml@refs/tags/v0.5.3
```

Both installers require `cosign`, download `SHA256SUMS` and
`SHA256SUMS.sigstore.json`, verify those exact claims, and only then download
the platform archive. Mirrors must preserve the manifest, bundle, archive, and
explicit tag. Plain HTTP is rejected unless
`FORGE_INSTALLER_TEST_MODE=1`; that switch exists only for isolated fixtures.

Fast negative suites cover missing and adulterated bundles, wrong issuers,
wrong workflow identities, wrong tags, and accidental HTTP use:

```bash
bash installer/tests/self-test.sh
```

```powershell
./installer/tests/self-test.ps1
```
