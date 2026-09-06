'use strict'

const assert = require('node:assert/strict')
const path = require('node:path')
const test = require('node:test')

const {
  loadSponsorShellConfig,
  publicConfigSummary,
  validateApiBaseUrl,
} = require('../lib/config')

const fixtures = path.join(__dirname, 'fixtures')

test('loads a linked CLI configuration without changing it', () => {
  const config = loadSponsorShellConfig(path.join(fixtures, 'linked-config.json'))
  assert.deepEqual(publicConfigSummary(config), {
    apiBaseUrl: 'https://staging.moneymux.com',
    deviceId: 'device_fixture',
    linked: true,
  })
  assert.equal(config.deviceToken, 'ssdev_fixture_not_a_secret')
})

test('refuses an unlinked CLI configuration', () => {
  assert.throws(
    () => loadSponsorShellConfig(path.join(fixtures, 'unlinked-config.json')),
    /deviceId is missing/
  )
})

test('permits HTTPS and loopback HTTP API origins only', () => {
  assert.equal(validateApiBaseUrl('https://moneymux.com/path/?q=1#hash'), 'https://moneymux.com/path')
  assert.equal(validateApiBaseUrl('http://127.0.0.1:4000/'), 'http://127.0.0.1:4000')
  assert.throws(() => validateApiBaseUrl('http://moneymux.com'), /must use HTTPS/)
  assert.throws(() => validateApiBaseUrl('https://token@moneymux.com'), /must use HTTPS/)
})

