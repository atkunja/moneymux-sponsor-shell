# MoneyMux Sponsor Shell

The open-source terminal client for MoneyMux. Sponsor Shell wraps a normal
shell or developer tool in a native `tmux` workspace and renders sponsored
ASCII placements without reading or replacing the wrapped program.

This repository is public so terminal users can inspect exactly what is
installed, what runs locally, and what information is sent to MoneyMux.

## Trust boundary

This repository contains:

- the Rust `sponsor-shell` executable;
- the `@moneymux/sponsor-shell` npm platform launcher;
- tests and release automation for both components; and
- the complete client-side data disclosure.

It does **not** contain the MoneyMux API, dashboard, ad marketplace, billing,
database, deployment configuration, or infrastructure credentials. Those
services remain private and communicate with this client through documented
HTTP requests.

## Install

The current staging workflow is:

```sh
npm install --global @moneymux/sponsor-shell@0.1.0
sponsor-shell login --api-base-url https://staging.moneymux.com
```

After registering a terminal in the MoneyMux Developer workspace, run the
one-time `sponsor-shell link` command shown by the dashboard, then verify the
connection:

```sh
sponsor-shell status
sponsor-shell
```

You can also wrap a specific command:

```sh
sponsor-shell codex
sponsor-shell claude
sponsor-shell bash
```

Sponsor Shell supports macOS and Linux on arm64 and x64. `tmux` is required.
When it is missing, the current client attempts to install it through Homebrew
or a supported Linux package manager. Set `SPONSOR_SHELL_INSTALL_TMUX=0` to
disable automatic dependency installation.

## What the client sends

When a terminal has been linked, Sponsor Shell sends the configured device ID,
the wrapped executable's basename, terminal dimensions and interactivity,
session state, and qualified impression/click events to the configured MoneyMux
API. It authenticates those requests with the device token stored locally.

It does not send terminal input, terminal output, command arguments, file
contents, shell history, working-directory paths, or environment-variable
values. See [DATA_COLLECTION.md](DATA_COLLECTION.md) for the field-level
disclosure and local storage details.

## Build and test from source

Prerequisites: Rust 1.96.1+, Node.js 18+, and `tmux` for interactive use.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
npm test --prefix packages/sponsor-shell
cargo build --locked --release -p sponsor-shell
```

The executable is written to `target/release/sponsor-shell`. The npm launcher
expects release executables in `packages/sponsor-shell/native/`; see
`packages/sponsor-shell/scripts/build-native.sh` for the four supported targets.

## Release verification

Tagged releases build every native executable in GitHub Actions. Release assets
include SHA-256 checksums and a keyless Sigstore bundle bound to the repository,
workflow, tag, and commit. The npm publication uses npm provenance so users can
trace the package back to the public build.

Release instructions and verification commands are in
[RELEASING.md](RELEASING.md).

## Security and contributions

Please read [SECURITY.md](SECURITY.md) before reporting a vulnerability. Pull
requests are welcome; development guidance is in
[CONTRIBUTING.md](CONTRIBUTING.md).

Sponsor Shell is distributed under the [MIT License](LICENSE).
