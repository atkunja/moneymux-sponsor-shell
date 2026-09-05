# Claude Code and Codex terminal integration

This is an opt-in **sidecar**, not a replacement for either vendor's spinner or
official app UI. It keeps the harness pane selected and never automatically
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

## What a label means

`Claude | recent hook: permission requested` means exactly that an allowlisted
hook arrived recently. Labels expire after ten seconds. They are **not** proof
of current model state, a loading/streaming distinction, visibility, attention,
or a billable impression. Hooks may be missing, delayed, reordered, duplicated,
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
