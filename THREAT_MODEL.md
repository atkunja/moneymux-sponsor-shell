# Sponsor Shell threat model

This model covers the Rust terminal client, npm launcher, native release
artifacts, and their interaction with a configured MoneyMux API. It does not
claim that the private MoneyMux service is implemented in this repository.

## Security objectives

Sponsor Shell must:

1. run only the command and arguments the user selected;
2. never treat sponsor creative as shell code;
3. never upload terminal input, terminal output, source files, command
   arguments, environment values, or working-directory paths;
4. protect the device bearer token at rest and in transit;
5. make privilege-changing or package-installing behavior explicit;
6. ensure a headless/CI render cannot qualify as a human impression; and
7. let users trace distributed binaries to this public source.

## Assets

- terminal input, output, history, and scrollback;
- source code and files reachable from the wrapped process;
- shell environment and command arguments;
- MoneyMux device ID and bearer token;
- advertiser budget and publisher impression/click accounting;
- integrity of the wrapped command, rendered creative, npm package, and native
  release executable.

## Trust boundaries

### npm launcher to native executable

The JavaScript launcher maps the current OS/CPU pair to one of four fixed
filenames inside the npm package. It does not download an executable or run a
remote installer. An absent or unsupported binary is a hard failure.

### Native executable to wrapped command

The Rust client creates a dedicated `tmux` session and passes the selected
command and arguments through shell quoting. Sponsor creative is rendered in a
separate pane and is never inserted into the wrapped command's stdin.

### Native executable to MoneyMux API

Only the fields enumerated in `DATA_COLLECTION.md` may cross this boundary.
Requests use HTTPS except for explicit local-development hosts. The device token
is placed in the authorization header only after URL validation succeeds.

### MoneyMux creative to terminal renderer

Creative fields are untrusted. Control characters and Unicode bidirectional
overrides are removed. Only schema-declared destinations become clickable,
their displayed text remains data, and click reporting uses the signed decision
issued with that creative.

## Threats and mitigations

### Malicious or compromised npm package

**Threat:** an attacker publishes a package that does not match this source.

**Mitigations:** npm Trusted Publishing from a tag-bound GitHub workflow, npm
provenance, a public immutable GitHub release, SHA-256 checksums, and a keyless
Sigstore signature tied to the repository/workflow identity. The release
workflow requires Rust/npm versions and the `vX.Y.Z` tag to agree.

### Malicious sponsor creative or API response

**Threat:** terminal escape injection, misleading hyperlinks, command execution,
or rendering outside the sponsor pane.

**Mitigations:** terminal-control and direction-override sanitization, explicit
schema mapping, declared-link allowlisting, HTTPS canonicalization, bounded
text/art dimensions, and a separate rendering process/pane. Renderer safety has
unit tests covering nested terminal hyperlinks, undeclared links, multibyte
text, and many terminal sizes.

### Device-token theft

**Threat:** credentials leak through filesystem permissions, logs, diagnostics,
URL userinfo, or cleartext HTTP.

**Mitigations:** Unix mode `0600`, no token output in `status` or `doctor`, URL
userinfo rejection, HTTPS enforcement for non-local endpoints, environment
override names without values, and `unlink`/`logout` credential removal.

### Command or argument exfiltration

**Threat:** a wrapped command, argument, path, or terminal content is uploaded as
analytics.

**Mitigations:** network payload constructors accept only the executable
basename, device/session identifiers, terminal geometry/interactivity, and
signed ad-event fields. The basename is path-stripped and bounded to 80
characters. Command arguments are deliberately ignored by the reporting label.
Any change to a payload or local storage requires a disclosure update in the PR
template.

### Silent privilege escalation

**Threat:** normal execution invokes a package manager or `sudo` without a clear
user decision.

**Mitigations:** missing `tmux` is a hard failure during normal execution.
Package installation occurs only through the explicit `sponsor-shell
install-tmux` management command, whose help text states what it does.

### Fraudulent impressions

**Threat:** CI logs, redirected output, hidden terminals, or repeated retries are
counted as human-visible impressions.

**Mitigations:** interactivity is measured in the outer process before `tmux`
creates a PTY, common CI markers force non-interactive status, a minimum visible
duration is required, signed decisions bind events, client sequence numbers
deduplicate reports, and retries are bounded.

### Dependency or build-system compromise

**Threat:** a vulnerable crate, unexpected Git dependency, registry substitution,
or unreviewed action changes release behavior.

**Mitigations:** committed Cargo lockfile, `cargo-deny` advisory/license/source
policy, pinned GitHub Action commit SHAs, grouped Dependabot updates, compiler
lint/test gates, and source-only Git history without committed native binaries.

## Residual risks

- A compromised MoneyMux service can choose misleading but sanitized creative
  copy and declared destinations until the service is remediated.
- The configured API and ordinary network infrastructure receive connection
  metadata such as IP address and HTTP headers.
- A locally compromised operating system, shell, `tmux`, package manager, Node,
  or Rust toolchain can observe or modify Sponsor Shell execution.
- Open source alone does not prove that an installed binary matches this source;
  users must verify release provenance and signatures.
- Removing a token from the config does not revoke server-side credentials or
  securely erase historical filesystem blocks. Users should revoke a suspected
  token in MoneyMux as well.

## Review triggers

Update this model and `DATA_COLLECTION.md` before merging a change that adds or
alters:

- a network request or payload field;
- local storage or credential handling;
- a subprocess, package manager, browser action, or privilege boundary;
- creative parsing, sanitization, link handling, or terminal control sequences;
- impression/click qualification or accounting; or
- npm/native build, signing, provenance, or release behavior.
