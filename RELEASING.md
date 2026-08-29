# Releasing Sponsor Shell

The Rust crate version and npm package version must always match.

## Maintainer checklist

1. Confirm npm Trusted Publishing is active for the package and workflow below.
2. Update both versions, `Cargo.lock`, the changelog, and install examples.
3. Run the full local validation suite, including `npm run release:check`.
4. Merge the version change to `main` through a reviewed pull request.
5. Create an annotated `vX.Y.Z` tag pointing at that exact `main` commit.
6. Push only the tag and monitor the `Release` workflow to completion.
7. Verify the GitHub release, Sigstore bundle, npm provenance, and a clean
   staging installation on each supported platform.

Before the first automated npm release, configure npm Trusted Publishing for:

- repository: `atkunja/moneymux-sponsor-shell`;
- workflow: `release.yml`;
- package: `@moneymux/sponsor-shell`.
- environment: leave unset.

No long-lived npm token is required by the release workflow.

## Prepare and tag a release

Run the complete validation suite on the release branch:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo deny check advisories bans licenses sources
npm test --prefix packages/sponsor-shell
npm run release:check --prefix packages/sponsor-shell
```

After the reviewed release pull request is merged, update local `main` and tag
that exact commit. Replace the example version in both commands:

```sh
git switch main
git pull --ff-only origin main
git tag --annotate v0.1.3 --message "Sponsor Shell v0.1.3"
git push origin v0.1.3
```

Do not move, recreate, or force-push a published release tag. npm package
versions and release tags are immutable.

## Staging smoke test

The current package promotion target is the MoneyMux staging API. After npm and
the GitHub release both succeed, verify the published package without relying
on a repository checkout:

```sh
npm view @moneymux/sponsor-shell@0.1.3 version dist.integrity repository.url
npm exec --yes --package=@moneymux/sponsor-shell@0.1.3 -- sponsor-shell --version
SPONSOR_SHELL_API_BASE_URL=https://staging.moneymux.com \
  npm exec --yes --package=@moneymux/sponsor-shell@0.1.3 -- sponsor-shell doctor
```

Then use a staging-only terminal registration to exercise `link`, `status`, one
interactive shell session, `unlink`, and a second `doctor` run. Do not promote
the package documentation or test device to the production API during this
release.

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
