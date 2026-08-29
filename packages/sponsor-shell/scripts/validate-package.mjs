import assert from 'node:assert/strict'
import { access, readFile, stat } from 'node:fs/promises'
import { constants } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

import { supportedTargets } from '../lib/platform.mjs'

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repositoryRoot = path.resolve(packageRoot, '..', '..')
const packageJson = JSON.parse(await readFile(path.join(packageRoot, 'package.json'), 'utf8'))
const cargoManifest = await readFile(
  path.join(repositoryRoot, 'crates', 'sponsor-shell', 'Cargo.toml'),
  'utf8',
)
const cargoVersion = cargoManifest.match(/^version = "([^"]+)"$/m)?.[1]

assert.equal(
  packageJson.version,
  cargoVersion,
  `npm version ${packageJson.version} must match Rust CLI version ${cargoVersion}`,
)

for (const target of supportedTargets) {
  const executable = path.join(packageRoot, 'native', `sponsor-shell-${target}`)
  await access(executable, constants.X_OK)
  const metadata = await stat(executable)
  assert.ok(metadata.size > 1_000_000, `${path.basename(executable)} is unexpectedly small`)
}

console.log(`@moneymux/sponsor-shell@${packageJson.version}: four native executables verified`)
