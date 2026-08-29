# Releasing Sponsor Shell

The Rust crate version and npm package version must always match.

## Maintainer checklist

1. Update both versions and the npm install example.
2. Run the full local validation suite.
3. Merge the version change to `main` through a reviewed pull request.
4. Create an annotated `vX.Y.Z` tag pointing at that exact `main` commit.
5. Push the tag and monitor the `Release` workflow.
6. Verify the GitHub release, Sigstore bundle, npm provenance, and a clean
   installation on each supported platform.

Before the first automated npm release, configure npm Trusted Publishing for:

- repository: `atkunja/moneymux-sponsor-shell`;
- workflow: `release.yml`;
- package: `@moneymux/sponsor-shell`.

No long-lived npm token is required by the release workflow.

## Verify a GitHub release

Download the release files, then verify their checksums:

```sh
sha256sum --check checksums.txt
```

Verify the keyless signature with Cosign, substituting the released tag:

```sh
cosign verify-blob checksums.txt \
  --bundle checksums.sigstore.json \
  --certificate-identity \
    https://github.com/atkunja/moneymux-sponsor-shell/.github/workflows/release.yml@refs/tags/vX.Y.Z \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

On macOS, use `shasum -a 256` to calculate individual SHA-256 hashes if
`sha256sum` is unavailable.
