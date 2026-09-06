'use strict'

const assert = require('node:assert/strict')
const test = require('node:test')

const {
  PATCH_START,
  applyOverlayPatch,
  buildOverlayPatch,
  hasOverlayPatch,
  stripOverlayPatch,
} = require('../lib/overlay')

const creative = {
  sponsor: 'Demo Cloud',
  headline: 'Ship the next release',
  disclosure: 'Sponsored',
  url: 'https://demo.example/',
  logoDataUrl: null,
}

test('builds syntactically valid Claude and Codex browser patches', () => {
  for (const kind of ['claude', 'codex']) {
    const patch = buildOverlayPatch(kind, creative)
    assert.doesNotThrow(() => new Function(patch))
    assert.match(patch, /Sponsored/)
    assert.match(patch, /MutationObserver/)
  }
})

test('keeps remote creative out of executable JavaScript source', () => {
  const malicious = {
    ...creative,
    sponsor: "</script>'; window.pwned = true; //",
    headline: '${globalThis.pwned = true}',
  }
  const patch = buildOverlayPatch('claude', malicious)
  assert.ok(!patch.includes(malicious.sponsor))
  assert.ok(!patch.includes(malicious.headline))
  assert.doesNotThrow(() => new Function(patch))
})

test('updates one bounded patch instead of stacking overlays', () => {
  const first = applyOverlayPatch('vendor();\n', buildOverlayPatch('claude', creative))
  const second = applyOverlayPatch(first, buildOverlayPatch('claude', { ...creative, sponsor: 'Next' }))
  assert.equal(second.split(PATCH_START).length - 1, 1)
  assert.equal(stripOverlayPatch(second).trim(), 'vendor();')
  assert.equal(hasOverlayPatch(second), true)
})

test('refuses an unknown vendor adapter', () => {
  assert.throws(() => buildOverlayPatch('desktop', creative), /Unsupported editor target/)
})
