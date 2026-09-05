# Changelog

All notable Sponsor Shell client and npm-launcher changes are recorded here.

## Unreleased

### Added

- Explicit protected Claude Code/Codex terminal wrappers with no automatic ad
  takeover and a quarter-height automatic sponsor-pane cap.
- Print-only hook configuration and silent local-only lifecycle adapters, using
  per-wrapper private sockets and ten-second advisory labels. No hook data or
  new billing evidence is sent to MoneyMux.
- Offline subprocess and tmux/PTY acceptance for hook privacy, permission-prompt
  visibility, stale hints, quoting, resize behavior and wrapped exit status.

### Fixed

- Preserve the wrapped program's exit status under zsh, whose `status` variable
  is read-only. Quote the complete exit trap so paths with spaces and apostrophes
  remain safe. Regression coverage executes the trap in both bash and zsh.

## 0.1.3 - 2026-08-29

### Fixed

- `sponsor-shell login` now opens the role-specific MoneyMux `/app`
  authentication flow instead of sending publishers to the marketing homepage.

### Testing

- Added regression coverage for hosted staging and path-prefixed API base URLs.

## 0.1.2 - 2026-08-29

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
