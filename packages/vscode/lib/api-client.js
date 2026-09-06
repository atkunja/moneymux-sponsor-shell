'use strict'

const { normalizeCreative } = require('./creative')

const REQUEST_TIMEOUT_MS = 5_000

async function postJson(config, pathname, body, options = {}) {
  const fetchImpl = options.fetchImpl ?? globalThis.fetch
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), options.timeoutMs ?? REQUEST_TIMEOUT_MS)
  try {
    const response = await fetchImpl(`${config.apiBaseUrl}${pathname}`, {
      method: 'POST',
      headers: {
        authorization: `Bearer ${config.deviceToken}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify(body),
      signal: controller.signal,
    })
    const text = await response.text()
    if (!response.ok) throw new Error(`MoneyMux API returned HTTP ${response.status}.`)
    return text ? JSON.parse(text) : {}
  } finally {
    clearTimeout(timer)
  }
}

async function fetchEditorCreative(config, options = {}) {
  const session = await postJson(
    config,
    '/api/terminal-sessions',
    { deviceId: config.deviceId, command: 'vscode-editor-setup' },
    options
  )
  if (typeof session.id !== 'string' || session.id.length === 0) {
    throw new Error('MoneyMux did not return an editor setup session.')
  }

  try {
    const decision = await postJson(
      config,
      '/api/ad-decision',
      {
        clientAdRequestId: `editor-setup-${Date.now()}`,
        sessionId: session.id,
        deviceId: config.deviceId,
        width: 80,
        height: 24,
        placement: 'editor_overlay',
        isTty: false,
        isInteractive: true,
        adsShownThisSession: 0,
      },
      options
    )
    return decision.creative ? normalizeCreative(decision.creative) : null
  } finally {
    await postJson(config, `/api/terminal-sessions/${encodeURIComponent(session.id)}/end`, {}, options)
  }
}

module.exports = {
  REQUEST_TIMEOUT_MS,
  fetchEditorCreative,
  postJson,
}
