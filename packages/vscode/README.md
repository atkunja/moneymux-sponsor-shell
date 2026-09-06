# MoneyMux Sponsor Shell for VS Code

This companion extension adds one opt-in, logo-enabled sponsored card beside
the active work indicator in compatible Claude Code and Codex VS Code panels.
It complements the terminal client; it does not replace or wrap either agent.

## Install from a VSIX

1. Install and link the public terminal client first.
2. Install the MoneyMux VSIX in VS Code.
3. Run **MoneyMux: Install Rich Editor Placement** from the Command Palette.
4. Read and accept the modal disclosure, then reload the window.

The command selects currently approved inventory using the linked terminal
device. That short-lived selection session is closed immediately. The editor
card never reports an impression or click and therefore does not charge an
advertiser or credit a publisher.

## Logo behavior

Reviewed PNG, JPEG, and WebP logos up to 64 KiB render inside the card without
contacting the advertiser's server. When a creative has no uploaded image, the
card uses a deterministic two-letter brand mark instead of fetching a favicon
from a third party.

## What the integration changes

VS Code does not provide an API for one extension to place UI inside another
extension's webview. After explicit confirmation, MoneyMux appends a bounded
script to one installed JavaScript bundle for each compatible editor extension:

- `anthropic.claude-code`: `webview/index.js`
- `openai.chatgpt`: the entry bundle referenced by `webview/index.html`

Before changing either file, MoneyMux stores a byte-exact backup and SHA-256
checksum beside it. It refuses unknown bundle layouts, incomplete prior patches,
missing backups, and checksum mismatches.

Run **MoneyMux: Restore Claude Code and Codex** before uninstalling MoneyMux.
That command restores the original vendor bytes and then offers to reload VS
Code. A vendor extension update normally installs to a new versioned directory;
run the install command again after the update if the placement is no longer
present.

## Development

```sh
npm install
npm test
npm run package
```

The generated `.vsix` can be installed with **Extensions: Install from VSIX** or
with `code --install-extension <file>.vsix`.

