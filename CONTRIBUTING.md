# Contributing

Thank you for helping improve Sponsor Shell.

## Development setup

Install Rust 1.96.1 or newer and Node.js 18 or newer, then run:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
npm test --prefix packages/sponsor-shell
```

Interactive testing also requires `tmux`. To prevent the client from attempting
to install it automatically, export `SPONSOR_SHELL_INSTALL_TMUX=0`.

## Pull requests

- Keep changes focused and add tests for changed behavior.
- Update `DATA_COLLECTION.md` whenever a change adds or alters local storage,
  subprocess execution, network requests, or collected fields.
- Never commit device tokens, customer creatives, credentials, compiled native
  executables, or private MoneyMux backend code.
- Run the full validation suite before requesting review.
- Treat terminal content as hostile input and preserve sanitization boundaries.

By contributing, you agree that your contribution is licensed under the MIT
License used by this repository.
