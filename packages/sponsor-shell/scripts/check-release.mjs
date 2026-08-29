import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const repositoryRoot = path.resolve(packageRoot, '..', '..')

async function read(relativePath) {
  return readFile(path.join(repositoryRoot, relativePath), 'utf8')
}

const packageJson = JSON.parse(await read('packages/sponsor-shell/package.json'))
const cargoManifest = await read('crates/sponsor-shell/Cargo.toml')
const cargoLock = await read('Cargo.lock')
const changelog = await read('CHANGELOG.md')
const rootReadme = await read('README.md')
const packageReadme = await read('packages/sponsor-shell/README.md')
const releaseRunbook = await read('RELEASING.md')

const version = packageJson.version
const cargoVersion = cargoManifest.match(/^version = "([^"]+)"$/m)?.[1]
const lockedVersion = cargoLock.match(
  /\[\[package\]\]\nname = "sponsor-shell"\nversion = "([^"]+)"/,
)?.[1]

assert.match(version, /^\d+\.\d+\.\d+$/, `npm version is not stable semver: ${version}`)
assert.equal(cargoVersion, version, 'Rust and npm package versions must match')
assert.equal(lockedVersion, version, 'Cargo.lock and package versions must match')
assert.match(
  changelog,
  new RegExp(`^## ${version.replaceAll('.', '\\.')}(?: - \\d{4}-\\d{2}-\\d{2})?$`, 'm'),
  `CHANGELOG.md must contain a ${version} release heading`,
)

const installCommand = `npm install --global @moneymux/sponsor-shell@${version}`
assert.ok(rootReadme.includes(installCommand), 'root README install command is stale')
assert.ok(packageReadme.includes(installCommand), 'npm README install command is stale')

const releaseTag = `v${version}`
const tagCommand = `git tag --annotate ${releaseTag} --message "Sponsor Shell ${releaseTag}"`
const packageSelector = `@moneymux/sponsor-shell@${version}`
assert.ok(releaseRunbook.includes(tagCommand), 'release runbook tag command is stale')
assert.ok(releaseRunbook.includes(packageSelector), 'release runbook package verification is stale')

const expectedTag = process.env.EXPECTED_RELEASE_TAG
if (expectedTag) {
  assert.equal(expectedTag, releaseTag, `tag ${expectedTag} does not match package version`)
}

console.log(`Release metadata is consistent for @moneymux/sponsor-shell@${version}`)
