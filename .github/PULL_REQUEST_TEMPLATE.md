## What changed

Describe the user-visible behavior and why this change is needed.

## Trust-boundary review

- [ ] This change does not add terminal input, terminal output, command arguments,
      file contents, working-directory paths, or environment values to network payloads.
- [ ] Any new network request or stored field is documented in `DATA_COLLECTION.md`.
- [ ] Any subprocess, installer, privilege escalation, or browser-opening behavior is explicit.
- [ ] Logs and errors do not expose device tokens or other credentials.
- [ ] Remote creative content remains sanitized before terminal rendering.

## Verification

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `cargo test --locked`
- [ ] `cargo deny check advisories bans licenses sources`
- [ ] `npm test --prefix packages/sponsor-shell`
- [ ] I added or updated tests for changed behavior.

## Release impact

- [ ] No release required
- [ ] Patch release
- [ ] Minor release
- [ ] Breaking release
