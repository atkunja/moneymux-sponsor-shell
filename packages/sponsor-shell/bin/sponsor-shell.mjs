#!/usr/bin/env node

import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import process from 'node:process'

import { binaryPath } from '../lib/platform.mjs'

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

let executable
try {
  executable = binaryPath(packageRoot)
} catch (error) {
  console.error(`sponsor-shell: ${error.message}`)
  process.exit(1)
}

if (!existsSync(executable)) {
  console.error(
    `sponsor-shell: the native executable is missing for ${process.platform}/${process.arch}. ` +
      'Reinstall @moneymux/sponsor-shell and try again.',
  )
  process.exit(1)
}

const child = spawn(executable, process.argv.slice(2), {
  env: process.env,
  stdio: 'inherit',
})

const forwardedSignals = ['SIGINT', 'SIGTERM', 'SIGHUP']
const signalHandlers = new Map()

for (const signal of forwardedSignals) {
  const handler = () => {
    if (!child.killed) child.kill(signal)
  }
  signalHandlers.set(signal, handler)
  process.on(signal, handler)
}

child.on('error', (error) => {
  console.error(`sponsor-shell: failed to launch the native executable: ${error.message}`)
  process.exit(1)
})

child.on('exit', (code, signal) => {
  for (const [forwardedSignal, handler] of signalHandlers) {
    process.off(forwardedSignal, handler)
  }

  if (signal) {
    process.kill(process.pid, signal)
    return
  }

  process.exit(code ?? 1)
})
