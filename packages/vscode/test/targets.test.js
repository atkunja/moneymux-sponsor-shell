'use strict'

const assert = require('node:assert/strict')
const path = require('node:path')
const test = require('node:test')

const { discoverTargets, resolveInside, targetForExtension } = require('../lib/targets')

const roots = path.join(__dirname, 'fixtures', 'targets')

function extension(id, name) {
  return {
    id,
    extensionPath: path.join(roots, name),
    packageJSON: { version: '1.2.3' },
  }
}

test('finds the Claude Code single webview bundle', () => {
  const target = targetForExtension(extension('Anthropic.claude-code', 'claude'))
  assert.equal(target.kind, 'claude')
  assert.equal(target.version, '1.2.3')
  assert.match(target.bundlePath, /claude[/\\]webview[/\\]index\.js$/)
})

test('reads the current Codex entry bundle from its webview HTML', () => {
  const target = targetForExtension(extension('OpenAI.chatgpt', 'codex'))
  assert.equal(target.kind, 'codex')
  assert.match(target.bundlePath, /codex[/\\]webview[/\\]assets[/\\]index-fixture\.js$/)
})

test('ignores unrelated and incompatible extensions', () => {
  const vscode = {
    extensions: {
      all: [
        extension('example.unrelated', 'claude'),
        extension('openai.chatgpt', 'missing'),
        extension('anthropic.claude-code', 'claude'),
      ],
    },
  }
  assert.deepEqual(discoverTargets(vscode).map((target) => target.id), ['anthropic.claude-code'])
})

test('does not allow discovered paths to escape a vendor extension', () => {
  assert.throws(() => resolveInside('/safe/extension', '../../outside.js'), /outside/)
})

