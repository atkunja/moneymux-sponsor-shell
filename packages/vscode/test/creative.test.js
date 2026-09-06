'use strict'

const assert = require('node:assert/strict')
const test = require('node:test')

const {
  MAX_LOGO_BYTES,
  initialsForSponsor,
  normalizeCreative,
  safeLogoDataUrl,
} = require('../lib/creative')

test('normalizes a reviewed creative for the editor card', () => {
  const logo = `data:image/png;base64,${Buffer.from('png').toString('base64')}`
  const creative = normalizeCreative({
    sponsor: ' Railway ',
    url: 'https://railway.app',
    headline: 'Deploy now',
    disclosure: 'Sponsored terminal time',
    logoDataUrl: logo,
  })

  assert.equal(creative.sponsor, 'Railway')
  assert.equal(creative.url, 'https://railway.app/')
  assert.equal(creative.logoDataUrl, logo)
  assert.ok(Object.isFrozen(creative))
})

test('rejects executable and oversized logo formats', () => {
  assert.throws(() => safeLogoDataUrl('data:image/svg+xml;base64,PHN2Zz4='), /PNG, JPEG, or WebP/)
  const tooLarge = Buffer.alloc(MAX_LOGO_BYTES + 1).toString('base64')
  assert.throws(() => safeLogoDataUrl(`data:image/png;base64,${tooLarge}`), /between 1 byte/)
})

test('requires canonical HTTPS advertiser destinations', () => {
  assert.throws(
    () => normalizeCreative({ sponsor: 'ACME', url: 'http://acme.test', headline: 'Ship' }),
    /canonical HTTPS/
  )
  assert.throws(
    () => normalizeCreative({ sponsor: 'ACME', url: 'https://user:pass@acme.test', headline: 'Ship' }),
    /canonical HTTPS/
  )
})

test('creates a deterministic two-character logo fallback', () => {
  assert.equal(initialsForSponsor('Demo Cloud'), 'DC')
  assert.equal(initialsForSponsor('Railway'), 'R')
  assert.equal(initialsForSponsor('***'), 'AD')
})

