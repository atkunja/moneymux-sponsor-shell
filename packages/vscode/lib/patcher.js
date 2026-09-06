'use strict'

const fs = require('node:fs')
const path = require('node:path')

const { backupPaths, createBackup, restoreBackup, sha256 } = require('./backup')
const { applyOverlayPatch, buildOverlayPatch, hasOverlayPatch } = require('./overlay')

function assertCompatible(target, source) {
  if (target.kind === 'claude') {
    if (!source.includes('spinnerRow_') || !source.includes('Claude is working')) {
      throw new Error(`Claude Code ${target.version} is not compatible with this MoneyMux build.`)
    }
    return
  }
  if (target.kind === 'codex') {
    if (!source.includes('__vite__mapDeps') || !source.includes('app-main-')) {
      throw new Error(`Codex ${target.version} is not compatible with this MoneyMux build.`)
    }
    return
  }
  throw new Error(`Unsupported editor target: ${target.kind}`)
}

function atomicWrite(filePath, contents) {
  const temporaryPath = path.join(
    path.dirname(filePath),
    `.${path.basename(filePath)}.moneymux-${process.pid}-${Date.now()}`
  )
  try {
    fs.writeFileSync(temporaryPath, contents, { flag: 'wx', mode: fs.statSync(filePath).mode })
    fs.renameSync(temporaryPath, filePath)
  } catch (error) {
    try {
      fs.unlinkSync(temporaryPath)
    } catch {
      // The temporary file may not have been created.
    }
    throw error
  }
}

function installTarget(target, creative) {
  const source = fs.readFileSync(target.bundlePath, 'utf8')
  assertCompatible(target, source)
  const paths = backupPaths(target.bundlePath)
  const alreadyPatched = hasOverlayPatch(source)
  if (alreadyPatched && !fs.existsSync(paths.bytes)) {
    throw new Error(`${target.label} contains a MoneyMux marker but its original backup is missing.`)
  }
  const metadata = createBackup(target.bundlePath, target)
  if (!alreadyPatched && sha256(source) !== metadata.originalSha256) {
    throw new Error(`${target.label} changed after MoneyMux created its backup; refusing to overwrite it.`)
  }

  const patched = applyOverlayPatch(source, buildOverlayPatch(target.kind, creative))
  atomicWrite(target.bundlePath, patched)
  if (!hasOverlayPatch(fs.readFileSync(target.bundlePath, 'utf8'))) {
    throw new Error(`MoneyMux could not verify the ${target.label} editor patch.`)
  }
  return { label: target.label, version: target.version, refreshed: alreadyPatched }
}

function restoreTarget(target) {
  return {
    label: target.label,
    version: target.version,
    restored: restoreBackup(target.bundlePath),
  }
}

function targetStatus(target) {
  const source = fs.readFileSync(target.bundlePath, 'utf8')
  const paths = backupPaths(target.bundlePath)
  let compatible = true
  try {
    assertCompatible(target, source)
  } catch {
    compatible = false
  }
  return {
    id: target.id,
    label: target.label,
    version: target.version,
    compatible,
    installed: hasOverlayPatch(source),
    backupAvailable: fs.existsSync(paths.bytes) && fs.existsSync(paths.metadata),
  }
}

module.exports = {
  assertCompatible,
  atomicWrite,
  installTarget,
  restoreTarget,
  targetStatus,
}
