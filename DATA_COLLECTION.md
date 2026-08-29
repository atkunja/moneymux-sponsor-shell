# Client data and behavior disclosure

This document describes the observable client behavior in this repository. It
is intended to make the terminal trust boundary explicit.

## Local execution

Sponsor Shell starts `tmux`, creates an isolated session, runs the requested
shell or developer tool in one pane, and renders the sponsor creative in a
second pane. The requested executable and arguments are passed to the wrapped
process locally.

The client uses local `tmux` activity timestamps to decide whether the user is
idle and whether an opt-in loading-state placement should expand. These activity
timestamps are not terminal contents.

If `tmux` is unavailable, normal Sponsor Shell commands stop and print manual
installation guidance. Sponsor Shell invokes Homebrew or a detected Linux
package manager only after the user explicitly runs `sponsor-shell
install-tmux`. That explicit command may invoke `sudo` on Linux.

## Local storage

By default the client stores its configuration at:

```text
~/.sponsor-shell/config.json
```

The file contains the configured API base URL, device ID, and device bearer
token. On Unix, Sponsor Shell creates and maintains the file with mode `0600`.
`SPONSOR_SHELL_CONFIG` can override its location.

`sponsor-shell unlink` (or `logout`) removes the stored device ID and bearer
token while preserving the selected API URL. Credential environment variables
cannot be removed from a parent shell, so the command names any active override
variables without printing their values.

For local creative development, Sponsor Shell may read
`.sponsor-shell/ad.json` in the current directory. `SPONSOR_SHELL_AD_FILE` can
override that path.

## Network requests

Network requests are sent only after a device has been linked, except that
`sponsor-shell login` opens the configured MoneyMux onboarding URL in the
default browser. API requests are sent to the base URL selected with
`--api-base-url`, stored in the config, or supplied through
`SPONSOR_SHELL_API_BASE_URL`.

Every authenticated API request includes the device bearer token in the
`Authorization` header. Remote API URLs must use HTTPS. Plain HTTP is accepted
only for loopback IP addresses, `localhost`, and `.localhost` development
domains. Embedded URL credentials, query strings, fragments, and non-HTTP
schemes are rejected before an authorization header is created.

`sponsor-shell doctor` performs local-only diagnostics. It reports the client
version, operating-system/CPU pair, configured API URL and transport state,
whether config and device linking are valid, `tmux` availability, terminal
interactivity, and the names of active credential override variables. It does
not print credential values.

### Terminal session

`POST /api/terminal-sessions` sends:

- device ID;
- the wrapped executable's basename, such as `codex`, `bash`, or `shell`.

The basename is limited to 80 characters. The full executable path and command
arguments are not sent. When the session ends, the client posts to
`/api/terminal-sessions/{sessionId}/end`.

### Ad decision

`POST /api/ad-decision` sends:

- device ID;
- terminal width and height;
- placement name (`prompt_boundary`);
- whether the outer process is attached to an interactive TTY;
- number of ads already shown in the current session;
- the MoneyMux session ID, when one was created;
- seconds since the last qualified ad, when available.

### Qualified impression

`POST /api/events/impression` sends:

- ad decision ID and signed decision token;
- terminal columns and rows;
- rendered line count;
- visible duration in milliseconds;
- a client sequence number.

### Click

`POST /api/events/click` sends:

- ad decision ID and signed decision token;
- a generated client event ID;
- the declared sponsor URL the user clicked.

Clicking a declared sponsor link opens it in the default browser. Creative text
is sanitized before rendering, and only schema-declared destinations are made
clickable.

## Information the client does not send

The client does not upload:

- terminal keystrokes or prompts;
- terminal output or scrollback;
- full command lines or command arguments;
- shell history;
- clipboard contents;
- source code or file contents;
- current working-directory paths;
- environment-variable names or values;
- unrelated process lists;
- hostnames, usernames, or hardware serial numbers.

The MoneyMux service may separately receive ordinary connection metadata such
as IP address and HTTP headers at the server/network layer. That server-side
handling is outside this client repository and should be covered by the
MoneyMux privacy policy.
