'use strict'

const fs = require('node:fs')
const path = require('node:path')

const TARGETS = Object.freeze([
  Object.freeze({ id: 'anthropic.claude-code', label: 'Claude Code', kind: 'claude' }),
  Object.freeze({ id: 'openai.chatgpt', label: 'Codex', kind: 'codex' }),
])

function resolveInside(root, relativePath) {
  const resolvedRoot = path.resolve(root)
  const resolved = path.resolve(resolvedRoot, relativePath)
  if (resolved !== resolvedRoot && !resolved.startsWith(`${resolvedRoot}${path.sep}`)) {
    throw new Error('Refusing an editor bundle path outside its extension directory.')
  }
  return resolved
}

function claudeBundle(extensionPath) {
  const bundlePath = resolveInside(extensionPath, path.join('webview', 'index.js'))
  return fs.existsSync(bundlePath) ? bundlePath : null
}

function codexBundle(extensionPath) {
  const htmlPath = resolveInside(extensionPath, path.join('webview', 'index.html'))
  if (!fs.existsSync(htmlPath)) return null
  const html = fs.readFileSync(htmlPath, 'utf8')
  const match = html.match(/<script\b[^>]*\btype=["']module["'][^>]*\bsrc=["']\.\/(assets\/index-[^"']+\.js)["']/i)
  if (!match) return null
  const bundlePath = resolveInside(path.dirname(htmlPath), match[1])
  return fs.existsSync(bundlePath) ? bundlePath : null
}

function targetForExtension(extension) {
  const descriptor = TARGETS.find((target) => target.id.toLowerCase() === extension.id.toLowerCase())
  if (!descriptor) return null
  const bundlePath = descriptor.kind === 'claude'
    ? claudeBundle(extension.extensionPath)
    : codexBundle(extension.extensionPath)
  if (!bundlePath) return null
  return Object.freeze({
    ...descriptor,
    version: String(extension.packageJSON?.version ?? 'unknown'),
    extensionPath: extension.extensionPath,
    bundlePath,
  })
}

function discoverTargets(vscode) {
  return vscode.extensions.all.map(targetForExtension).filter(Boolean)
}

module.exports = {
  TARGETS,
  codexBundle,
  discoverTargets,
  resolveInside,
  targetForExtension,
}
