# Security policy

## Supported versions

Security fixes are applied to the latest published Sponsor Shell release. Users
should update to the newest `@moneymux/sponsor-shell` version before reporting a
problem that may already have been fixed.

## Reporting a vulnerability

Do not disclose an exploitable vulnerability in a public issue. Use this
repository's **Security → Report a vulnerability** flow to send a private
GitHub security advisory to the maintainers.

Please include:

- affected Sponsor Shell and operating-system versions;
- reproduction steps or a minimal proof of concept;
- expected and observed behavior;
- potential impact; and
- any suggested remediation or disclosure constraints.

Do not include real device tokens, account credentials, customer terminal
content, or other secrets. Revoke any credential that may have been exposed.

The maintainers will acknowledge a complete report, assess severity, coordinate
a fix and release, and credit the reporter when requested and appropriate.

## Security properties

- Device tokens are stored with owner-only permissions on Unix.
- The client warns before transmitting a device token over non-local HTTP.
- Remote creative text is stripped of terminal control and bidirectional
  override characters before rendering.
- Only schema-declared creative links become terminal hyperlinks.
- Release assets are checksumed and signed using keyless Sigstore identity.
- npm releases are expected to carry npm provenance from GitHub Actions.

Open source improves auditability but is not, by itself, proof that a downloaded
binary matches the source. Verify the release checksum, signature, and npm
provenance before installing in a sensitive environment.
