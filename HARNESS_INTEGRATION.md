# Claude Code and Codex terminal integration

This is an opt-in **sidecar**, not a replacement for either vendor's spinner or
official app UI. It starts with the harness pane selected and never automatically
zooms the ad over it, even during long idle periods, permission prompts, missing
hooks, or `SPONSOR_SHELL_AD_WHILE_WORKING=1`. The initial split gives the harness
75% of the window; automatic ad fitting uses at most one quarter (with a minimum
of two rows). Users can still move the divider themselves.

## Start a protected terminal session

Build the current source first; these commands are not in npm release 0.1.3:

```sh
cargo build --locked --release
./target/release/sponsor-shell harness claude
./target/release/sponsor-shell harness codex
./target/release/sponsor-shell harness codex -- --help
```

Arguments following the harness name (or optional `--`) are passed unchanged.
The executable is resolved from the invoking shell's `PATH`, not a stale tmux
server's `PATH`. No aliases, shell profiles, vendor binaries or settings are
rewritten. Both a supported harness executable and tmux must already be installed.

Existing `sponsor-shell claude` / `sponsor-shell codex` commands retain the old
generic wrapper behavior. Use the explicit `harness` command for protected mode.

## Optional activity hints

The sidecar works without hooks and displays `activity unavailable`. To enable
recent-event labels, print the appropriate configuration from the binary you
intend to keep installed:

```sh
./target/release/sponsor-shell harness-hooks claude
./target/release/sponsor-shell harness-hooks codex
```

These commands **only print JSON**. Review it, then manually merge its hook
entries into your existing settings without replacing existing entries:

- Claude Code: the `hooks` object in your selected Claude settings layer, for
  example `~/.claude/settings.json` or a trusted project settings file.
- Codex: the `hooks` object in `~/.codex/hooks.json` or a trusted project's
  `.codex/hooks.json`. In Codex, review and trust the definitions through `/hooks`.

Do not redirect the output over an existing settings file. Each generated command
contains the absolute binary path, shell-quoted for spaces/apostrophes. Regenerate
and re-review if you move the binary. Do not install the same entries at multiple
layers. No hook changes permissions, supplies model context, or returns a policy
decision. The one-second hook timeout bounds hung stdin; command failures are
silent successful no-ops within Sponsor Shell.

To disable: remove only the Sponsor Shell entries you added, then stop the
wrapped session. No global hook service or session log remains. The private
socket directory is removed on normal wrapper exit. A force-killed wrapper may
leave an empty directory/socket inode in the operating system's temporary area;
it contains no prompt, transcript or token data.

## Optional sponsored spinner tip

The waiting state itself. `spinnerTipsOverride` is a documented Claude Code
setting that adds an entry to the tip rotation shown while a turn runs, and
Claude Code renders it as `<label>: <text>` — so with a `Sponsored` label the
disclosure is part of the line.

```sh
./target/release/sponsor-shell claude-spinner-setup
```

Print-only. It emits the JSON to merge into your own settings and never writes a
file.

Two deliberate refusals. It does **not** use `spinnerVerbs`: that slot says what
Claude is doing — "Accomplishing", "Baking" — so a sponsor there dresses an
advertisement up as the model's own status. And it does **not** set
`excludeDefault`, so Claude Code's built-in tips keep showing alongside it.

It earns nothing. Claude Code renders the rotation itself and reports nothing
back, so there is no impression, no click and no visibility signal to bill on.
Earnings come from the sidecar, which can observe its own pane. The creative is
fixed when you install it; re-run the command to refresh it.

Remove it by deleting the `spinnerTipsOverride` key you added.

## Optional Claude status line

A second, separate placement: one sponsored row beneath Claude Code's own
footer, showing an approved sponsor name as a clickable link.

```sh
./target/release/sponsor-shell claude-status-line-setup
```

That command **only prints**. Read what it says before pasting anything: Claude
Code hides most built-in footer hints once any custom status line is configured,
including `esc to interrupt`. Losing the documented way to stop a running model
is a real cost, and it is your decision rather than this tool's.

The status line receives far more session data than the hooks do — transcript
path, working directory, repository owner and name, session id and name, prompt
id, session cost, rate-limit consumption. `claude-status-line` drains that input
so Claude Code never blocks writing it, then discards it without parsing. It
makes no network request: updates are debounced at 300ms and an in-flight
command is killed when a new one arrives, so a request there could not be
retried and would delay your own status line.

It renders nothing at all when there is no approved creative. It is **not** a
billable impression — the command runs even while the status line is hidden by a
prompt or menu, so running it proves nothing about visibility. Impressions
remain with the sidecar, which can observe its own pane.

Remove it by deleting the `statusLine` key you added. Nothing else is installed.

## What a label means

`Claude | working | recent hook: prompt submitted` has two parts. The phase
(`working`, `waiting on you`, `idle`, or `activity unavailable`) is derived from
the sequence of allowlisted hooks; the recent hook names the single event it was
derived from and expires after ten seconds.

The phase is tracked separately because a turn routinely runs for minutes with
no hook in between, so a ten-second label cannot describe one. It expires after
fifteen minutes without any event, so a harness killed mid-turn decays to
`activity unavailable` rather than claiming work forever.

`waiting on you` is deliberately distinct from `working`: while a permission
prompt is open the model is not computing, and the user can see that on their
own screen.

None of this is proof of model state, a loading/streaming distinction,
visibility, attention, or a billable impression. A phase is a best-effort
reading of an advisory event stream. Hooks may be missing, delayed, reordered, duplicated,
or inherited by subagents. A subagent sharing the wrapper environment can update
the same advisory label. No per-turn correctness is claimed.

The generated configuration uses core session, prompt, tool, permission and stop
events, plus Claude notifications/tool failures and Codex interrupts. It does not
depend on Claude's newer MessageDisplay event or parse transcripts/terminal
output to infer a loading state. If your installed vendor version does not
support an event, omit that entry; protected mode still works without it.

Each wrapper has a private mode-0700 temporary directory and a local Unix socket.
The hook reads at most 64 KiB of stdin, discards all but the event name, and sends
only the harness identifier, event name and timestamp to that socket. Nothing
from a hook is sent to MoneyMux. Oversized/malformed input, unavailable sockets,
unknown events and stale/future datagrams are ignored. These are local hints,
not an authenticated boundary against other processes running as the same user.

## Advertising and payment boundaries

Short panes show a clearly labeled sponsored preview instead of the cropped top
of the full poster. Sponsor identity, the exact destination and full disclosure
must fit together; otherwise the slot says to enlarge the pane and exposes no
ad link. A supplied logo is included only if the entire logo fits after those
essentials. Recent-hook labels replace only the closing border.

These compact previews allow navigation but send **no impression or click
reports**. The existing signed server contract describes the full creative, not
this shortened placement. Full-creative reports still use the original line
count, token and minimum duration. Resizing or hiding a creative restarts its
uninterrupted geometry interval, including when a resize event is missed.
This is geometry checking, not proof that the terminal window is foregrounded.
Paid compact/loading placements need a versioned server visibility contract;
do not shorten reported line counts to make an existing decision qualify.

The existing creative pane still displays disclosed sponsor art and declared
links. Its existing signed decision, interactivity, duration and click rules
remain the only client-side ad-event inputs. A hook never requests an ad, reports
an impression/click, changes billing eligibility or earns publisher money. This
does not add a status-line or native-loading placement. Such a placement needs
its own server contract and visibility qualification before it can be billed.

## Evidence and limitations

The implementation follows the official
[Codex hooks contract](https://learn.chatgpt.com/docs/hooks) and
[Claude Code hooks contract](https://code.claude.com/docs/en/hooks), checked on
2026-09-04. The docs describe lifecycle callbacks, not permission to replace a
vendor's native loading UI. Claude's separate
[status-line interface](https://code.claude.com/docs/en/statusline) and Codex's
[App Server](https://learn.chatgpt.com/docs/app-server) are possible future
integration surfaces, not features implemented here.

Offline acceptance with synthetic harness executables checks actual tmux PTYs,
argument forwarding, local hook rendering, stale-hint expiry, permission-prompt
visibility, resizing and exit codes:

```sh
cargo build --locked
python3 scripts/test-harness-pty.py target/debug/sponsor-shell
```

It uses a dedicated temporary tmux server, disables billing interactivity, never
loads vendor account credentials and does not alter your existing tmux sessions.
This fixture test is not proof of hook delivery from a real Claude/Codex model
turn. Live vendor hook acceptance and a signed public release remain separate
release gates.

### Local verification record — 2026-09-04

- Rust 1.96.1 on macOS arm64: formatting, strict Clippy, 53 unit tests and three
  compiled-CLI integration tests passed; optimized release build passed.
- Three npm-launcher tests and release-metadata consistency passed.
- Dependency policy passed for the initial bridge, with the existing non-blocking
  duplicate `syn` dependency warning. Linux acceptance subsequently selected
  Crossterm's supported `use-dev-tty` input backend (adding `filedescriptor` and
  its `thiserror` dependency) to avoid losing keys alongside resize readiness.
  This backend uses level-triggered polling with a one-millisecond deadline;
  zero-timeout polls are unsupported by its event loop.
  Dependency policy was rerun successfully with those three new locked packages.
- Synthetic Claude and Codex PTYs each preserved exit 37, literal metacharacter
  arguments, stale-server socket isolation, private-input suppression, visible
  permission prompts at 100x36 and 48x18, and no zoom beyond the ten-second lease.
  The final suite passed on macOS arm64 and a clean Debian Linux arm64 container:
  a third Codex fixture forwarded Ctrl-C after resize from the selected ad pane
  and preserved exit 130; a fourth handled the interrupt, stayed open, then
  preserved its later exit 37. All runs cleaned their private hook directories.
  The supervisory shell catches SIGINT without ignoring it, so the wrapped
  process still receives it normally and controls whether its session ends.
- Installed vendor binaries reported `codex-cli 0.144.1` and Claude Code
  `2.1.191`; this records discovery only, not real provider hook acceptance.
- The public repository still requires its normal code-owner review and hosted
  checks. This record is local evidence, not a claim that a PR is merged or that
  a new npm release is available. No staging/production flags were changed.

Ubuntu's tmux 3.4 also exposed a pre-existing startup failure with `split-window
-p`. The wrapper now uses the documented `-l 75%` form (or the configured generic
wrapper percentage), supported by the
[tmux manual](https://man.openbsd.org/OpenBSD-7.5/tmux.1). PTY acceptance uses a
real controlling terminal and an isolated tmux server, with bounded synthetic
output available if startup fails.

### Provider and compact-layout acceptance — 2026-09-04

An isolated macOS tmux session ran installed Claude Code 2.1.191 with built-in
tools disabled and synthetic text only. A real response completed beside the
sidecar. A second invocation loaded the binary's generated hook JSON through
session-only `--settings`: actual `SessionStart` and `Stop` labels appeared,
and the exact requested response was visible. At 80x24 the sidecar retained
the destination and disclosure; at 48x18 it hid the undersized ad and retained
the selected 13-row harness pane without zoom. No ad account was configured;
the outer `CI=1` verdict kept billing interactivity disabled.

Installed Codex CLI 0.144.1 also completed a real synthetic no-tools response
under read-only/untrusted approval settings. Its six-row sidecar retained the
preview, and resizing to 48x18 hid the ad without taking over the harness.
The initial Codex run did not install or trust hooks, so it proves the no-hook
fallback. A follow-up loaded a temporary project `hooks.json`, checked for exact
equality with generated configuration, then reviewed its local command/source
and trusted it through the normal `/hooks` UI. The installed version recognized
six of the eight configured events; its actual `Stop` label appeared beside a
second exact synthetic response. This establishes real Codex stop-hook delivery,
not acceptance of unsupported or unobserved events. No hook-trust bypass flags
were used. The observed rate-limit prompt was dismissed without changing models.
The temporary project hook file was removed after the test.

The first Claude run exposed the cropped-logo bug fixed here. It also presented
an unexpected onboarding dialog; an Enter intended for the test selected global
auto-permission mode. The operator stopped that session and explicitly set
Claude's permission default to `default`, then used session-scoped `default`
for the follow-up. No model tools were available in either Claude test.

Local checks: 60 Rust unit tests, three compiled-CLI tests, all four synthetic
PTY cases, three npm tests, release-metadata checking, formatting, strict Clippy,
optimized build and dependency policy passed (existing duplicate `syn` warning).
The PTY suite now asserts disclosed previews at normal size and no cropped
destination after shrinking. Real permission/tool/streaming scenarios, complete
vendor-event coverage, full waiting-state placement, paid compact-placement
acceptance and a signed release remain unproven.
