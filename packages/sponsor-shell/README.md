# @moneymux/sponsor-shell

The official MoneyMux terminal shell. It wraps your normal shell or developer
tool in a native `tmux` workspace with contextual, non-intrusive sponsor
inventory. The complete Rust client and npm launcher source are public at
<https://github.com/atkunja/moneymux-sponsor-shell>.

## Install

```sh
npm install --global @moneymux/sponsor-shell@0.1.0
```

Then connect the terminal to your MoneyMux developer account. For staging:

```sh
sponsor-shell login --api-base-url https://staging.moneymux.com
sponsor-shell link --api-base-url https://staging.moneymux.com --device-id <device_id> --device-token <token>
sponsor-shell status
sponsor-shell
```

Register the terminal in the authenticated Developer workspace before running
`link`. Use `https://moneymux.com` instead when connecting to production.

You can wrap a specific tool or command too:

```sh
sponsor-shell codex
sponsor-shell claude
sponsor-shell bash
```

## Supported platforms

- macOS arm64 and x64
- Linux arm64 and x64

`tmux` is required. If it is missing, Sponsor Shell can install it using a
supported macOS or Linux package manager. Set `SPONSOR_SHELL_INSTALL_TMUX=0`
to require a manual installation instead.

## Privacy

Sponsor Shell reports the linked device ID, wrapped executable basename,
terminal dimensions/interactivity, session state, and qualified sponsor
impression/click events. It does not send terminal input/output, command
arguments, shell history, source files, working-directory paths, or environment
variable values. Read the field-level disclosure in
<https://github.com/atkunja/moneymux-sponsor-shell/blob/main/DATA_COLLECTION.md>.

## Package integrity

The npm package contains the native MoneyMux Rust executable for every
supported platform. The launcher selects only the executable matching the
current operating system and CPU architecture; it does not download or execute
an unpinned remote install script.

Tagged releases publish SHA-256 checksums, a keyless Sigstore signature, and
npm provenance tied to the public source commit.

MoneyMux homepage: <https://moneymux.com>
