'use strict'

const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')

function defaultConfigPath() {
  return path.join(os.homedir(), '.sponsor-shell', 'config.json')
}

function validateApiBaseUrl(value) {
  const url = new URL(value)
  const local = url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '::1'
  if (url.username || url.password || (url.protocol !== 'https:' && !(local && url.protocol === 'http:'))) {
    throw new Error('Sponsor Shell API must use HTTPS, except for a loopback development server.')
  }
  url.pathname = url.pathname.replace(/\/+$/, '')
  url.search = ''
  url.hash = ''
  return url.toString().replace(/\/$/, '')
}

function loadSponsorShellConfig(configPath = defaultConfigPath()) {
  let parsed
  try {
    parsed = JSON.parse(fs.readFileSync(configPath, 'utf8'))
  } catch (error) {
    throw new Error(`Could not read Sponsor Shell configuration at ${configPath}.`, { cause: error })
  }
  if (typeof parsed.deviceId !== 'string' || parsed.deviceId.trim() === '') {
    throw new Error('Sponsor Shell is not linked: deviceId is missing.')
  }
  if (typeof parsed.deviceToken !== 'string' || !parsed.deviceToken.startsWith('ssdev_')) {
    throw new Error('Sponsor Shell is not linked: deviceToken is missing or invalid.')
  }
  return Object.freeze({
    apiBaseUrl: validateApiBaseUrl(parsed.apiBaseUrl ?? 'https://moneymux.com'),
    deviceId: parsed.deviceId.trim(),
    deviceToken: parsed.deviceToken,
  })
}

function publicConfigSummary(config) {
  return { apiBaseUrl: config.apiBaseUrl, deviceId: config.deviceId, linked: true }
}

module.exports = {
  defaultConfigPath,
  loadSponsorShellConfig,
  publicConfigSummary,
  validateApiBaseUrl,
}
