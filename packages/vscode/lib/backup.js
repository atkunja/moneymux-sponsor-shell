'use strict'

const crypto = require('node:crypto')
const fs = require('node:fs')

function sha256(value) {
  return crypto.createHash('sha256').update(value).digest('hex')
}

function backupPaths(bundlePath) {
  return {
    bytes: `${bundlePath}.moneymux.backup`,
    metadata: `${bundlePath}.moneymux.json`,
  }
}

function createBackup(bundlePath, target) {
  const paths = backupPaths(bundlePath)
  if (fs.existsSync(paths.bytes) || fs.existsSync(paths.metadata)) {
    const metadata = readBackupMetadata(bundlePath)
    const backup = fs.readFileSync(paths.bytes)
    if (sha256(backup) !== metadata.originalSha256) {
      throw new Error(`MoneyMux backup checksum mismatch for ${target.label}.`)
    }
    return metadata
  }

  const original = fs.readFileSync(bundlePath)
  const metadata = {
    schemaVersion: 1,
    targetId: target.id,
    targetLabel: target.label,
    targetVersion: target.version,
    originalSha256: sha256(original),
    createdAt: new Date().toISOString(),
  }
  fs.writeFileSync(paths.bytes, original, { flag: 'wx', mode: 0o600 })
  fs.writeFileSync(paths.metadata, `${JSON.stringify(metadata, null, 2)}\n`, {
    flag: 'wx',
    mode: 0o600,
  })
  return metadata
}

function readBackupMetadata(bundlePath) {
  const paths = backupPaths(bundlePath)
  const value = JSON.parse(fs.readFileSync(paths.metadata, 'utf8'))
  if (value.schemaVersion !== 1 || typeof value.originalSha256 !== 'string') {
    throw new Error('MoneyMux backup metadata is invalid.')
  }
  return value
}

function restoreBackup(bundlePath) {
  const paths = backupPaths(bundlePath)
  if (!fs.existsSync(paths.bytes) || !fs.existsSync(paths.metadata)) return false
  const metadata = readBackupMetadata(bundlePath)
  const original = fs.readFileSync(paths.bytes)
  if (sha256(original) !== metadata.originalSha256) {
    throw new Error(`MoneyMux backup checksum mismatch for ${metadata.targetLabel}.`)
  }
  fs.writeFileSync(bundlePath, original)
  return true
}

module.exports = {
  backupPaths,
  createBackup,
  readBackupMetadata,
  restoreBackup,
  sha256,
}

