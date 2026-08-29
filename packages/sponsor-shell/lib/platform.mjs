import path from 'node:path'

const targets = new Map([
  ['darwin-arm64', 'aarch64-apple-darwin'],
  ['darwin-x64', 'x86_64-apple-darwin'],
  ['linux-arm64', 'aarch64-unknown-linux-gnu'],
  ['linux-x64', 'x86_64-unknown-linux-gnu'],
])

export function targetFor(platform = process.platform, arch = process.arch) {
  const target = targets.get(`${platform}-${arch}`)

  if (!target) {
    throw new Error(
      `MoneyMux does not currently support ${platform}/${arch}. ` +
        'Supported platforms are macOS and Linux on arm64 or x64.',
    )
  }

  return target
}

export function binaryPath(packageRoot, platform = process.platform, arch = process.arch) {
  return path.join(packageRoot, 'native', `sponsor-shell-${targetFor(platform, arch)}`)
}

export const supportedTargets = Object.freeze([...targets.values()])
