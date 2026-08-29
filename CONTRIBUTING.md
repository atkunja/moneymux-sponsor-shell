# Contributing

Thank you for helping improve Sponsor Shell.

## Development setup

Install Rust 1.96.1 or newer, Node.js 18 or newer, and `cargo-deny` 0.20.2
(`cargo install cargo-deny --version 0.20.2 --locked`), then run:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo deny check advisories bans licenses sources
npm test --prefix packages/sponsor-shell
```

Interactive testing also requires `tmux`. Normal execution never invokes a
package manager. The explicit `sponsor-shell install-tmux` command exists for
developers who choose that behavior.

## Pull requests

- Keep changes focused and add tests for changed behavior.
- Update `DATA_COLLECTION.md` whenever a change adds or alters local storage,
  subprocess execution, network requests, or collected fields.
- Update `THREAT_MODEL.md` when a change alters a protected asset, trust
  boundary, threat, mitigation, or residual risk.
- Never commit device tokens, customer creatives, credentials, compiled native
  executables, or private MoneyMux backend code.
- Run the full validation suite before requesting review.
- Treat terminal content as hostile input and preserve sanitization boundaries.

By contributing, you agree that your contribution is licensed under the MIT
License used by this repository.
