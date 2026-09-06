'use strict'

const assert = require('node:assert/strict')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const test = require('node:test')

const { targetForExtension } = require('../lib/targets')
const { installTarget, restoreTarget, targetStatus } = require('../lib/patcher')
const { PATCH_START } = require('../lib/overlay')

const fixtures = path.join(__dirname, 'fixtures', 'targets')
const creative = {
  sponsor: 'Demo Cloud',
  url: 'https://demo.example/',
  headline: 'Ship now',
  disclosure: 'Sponsored',
  logoDataUrl: null,
}

function copiedTarget(name, id) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), `moneymux-${name}-patch-test-`))
  fs.cpSync(path.join(fixtures, name), directory, { recursive: true })
  return targetForExtension({
    id,
    extensionPath: directory,
    packageJSON: { version: '1.2.3' },
  })
}

test('installs, refreshes, and exactly restores the Claude overlay', () => {
  const target = copiedTarget('claude', 'anthropic.claude-code')
  const original = fs.readFileSync(target.bundlePath, 'utf8')

  assert.equal(installTarget(target, creative).refreshed, false)
  assert.equal(targetStatus(target).installed, true)
  assert.equal(installTarget(target, { ...creative, sponsor: 'Next Sponsor' }).refreshed, true)
  const patched = fs.readFileSync(target.bundlePath, 'utf8')
  assert.equal(patched.split(PATCH_START).length - 1, 1)

  assert.equal(restoreTarget(target).restored, true)
  assert.equal(fs.readFileSync(target.bundlePath, 'utf8'), original)
})

test('installs and restores the Codex entry bundle', () => {
  const target = copiedTarget('codex', 'openai.chatgpt')
  const original = fs.readFileSync(target.bundlePath, 'utf8')
  installTarget(target, creative)
  assert.equal(targetStatus(target).installed, true)
  restoreTarget(target)
  assert.equal(fs.readFileSync(target.bundlePath, 'utf8'), original)
})

test('fails closed on an unknown vendor build', () => {
  const target = copiedTarget('claude', 'anthropic.claude-code')
  fs.writeFileSync(target.bundlePath, 'new incompatible vendor bundle')
  assert.throws(() => installTarget(target, creative), /not compatible/)
})
