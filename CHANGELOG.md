# Changelog

All notable Sponsor Shell client and npm-launcher changes are recorded here.

## Unreleased

### Changed

- Updated the terminal event and rendering backend from crossterm 0.27 to 0.29.
- Upgraded the Sponsor Shell HTTP client from ureq 2 to ureq 3 while
  preserving the two-second request deadline and non-success status handling.
- Reuse one configured HTTP agent so API calls can share connection state.

### Fixed

- Key-release events are ignored so enhanced terminals cannot forward a
  duplicate `Ctrl-C` into the wrapped application.

### Testing

- Added loopback transport coverage for JSON request bodies, bearer
  authorization, response bodies, and non-success API statuses.

## 0.1.1 - 2026-08-29

### Added

- Public contribution governance, security issue routing, and code ownership.
- `sponsor-shell doctor` secret-free local diagnostics.
- `sponsor-shell unlink` and `logout` local credential removal.
- `sponsor-shell install-tmux` as an explicit dependency-installation command.
- Global help and version commands.
- Cargo advisory, license, source, wildcard, and duplicate-dependency policy.
- A field-level client disclosure and public terminal threat model.

### Changed

- Remote API base URLs now require HTTPS. Plain HTTP is accepted only for
  loopback and `.localhost` development hosts.
- API URLs containing embedded credentials, query strings, fragments, or
  unsupported schemes are rejected before authorization is attached.
- Normal Sponsor Shell execution no longer invokes Homebrew, a Linux package
  manager, or `sudo` when `tmux` is missing.
- Routine patch dependencies and GitHub Action updates are grouped by
  Dependabot while breaking updates remain isolated.

### Security

- Updated `anyhow` to a version that fixes RustSec advisory RUSTSEC-2026-0190.
- Serialized process-environment tests to remove nondeterministic security-test
  failures.

## 0.1.0 - 2026-08-29

- Initial Rust terminal client and four-platform npm launcher.
- MoneyMux device linking, terminal sessions, remote ad decisions, qualified
  impression/click reporting, responsive ASCII rendering, and local creative
  development.
