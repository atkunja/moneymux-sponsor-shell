import assert from 'node:assert/strict'
import test from 'node:test'

import { binaryPath, supportedTargets, targetFor } from '../lib/platform.mjs'

test('maps every supported npm platform to its Rust release target', () => {
  assert.equal(targetFor('darwin', 'arm64'), 'aarch64-apple-darwin')
  assert.equal(targetFor('darwin', 'x64'), 'x86_64-apple-darwin')
  assert.equal(targetFor('linux', 'arm64'), 'aarch64-unknown-linux-gnu')
  assert.equal(targetFor('linux', 'x64'), 'x86_64-unknown-linux-gnu')
  assert.equal(supportedTargets.length, 4)
})

test('rejects an unsupported operating system before spawning', () => {
  assert.throws(() => targetFor('win32', 'x64'), /does not currently support win32\/x64/)
})

test('resolves binaries inside the package instead of a global install directory', () => {
  assert.equal(
    binaryPath('/package', 'linux', 'x64'),
    '/package/native/sponsor-shell-x86_64-unknown-linux-gnu',
  )
})
