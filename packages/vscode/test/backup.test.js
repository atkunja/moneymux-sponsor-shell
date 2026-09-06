'use strict'

const assert = require('node:assert/strict')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const test = require('node:test')

const { backupPaths, createBackup, restoreBackup, sha256 } = require('../lib/backup')

function fixtureBundle() {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'moneymux-backup-test-'))
  const bundlePath = path.join(directory, 'index.js')
  fs.writeFileSync(bundlePath, 'vendor-original')
  return bundlePath
}

const target = {
  id: 'anthropic.claude-code',
  label: 'Claude Code',
  version: '1.2.3',
}

test('backs up a vendor bundle once with a cryptographic checksum', () => {
  const bundlePath = fixtureBundle()
  const metadata = createBackup(bundlePath, target)
  const paths = backupPaths(bundlePath)

  assert.equal(fs.readFileSync(paths.bytes, 'utf8'), 'vendor-original')
  assert.equal(metadata.originalSha256, sha256('vendor-original'))
  assert.equal(createBackup(bundlePath, target).originalSha256, metadata.originalSha256)
})

test('restores the exact original vendor bytes', () => {
  const bundlePath = fixtureBundle()
  createBackup(bundlePath, target)
  fs.writeFileSync(bundlePath, 'patched')

  assert.equal(restoreBackup(bundlePath), true)
  assert.equal(fs.readFileSync(bundlePath, 'utf8'), 'vendor-original')
})

test('refuses to restore a tampered backup', () => {
  const bundlePath = fixtureBundle()
  createBackup(bundlePath, target)
  fs.writeFileSync(backupPaths(bundlePath).bytes, 'tampered')

  assert.throws(() => restoreBackup(bundlePath), /checksum mismatch/)
})

test('reports no-op when a target has no MoneyMux backup', () => {
  assert.equal(restoreBackup(fixtureBundle()), false)
})
