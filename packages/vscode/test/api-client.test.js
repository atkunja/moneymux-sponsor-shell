'use strict'

const assert = require('node:assert/strict')
const test = require('node:test')

const { fetchEditorCreative } = require('../lib/api-client')

const config = {
  apiBaseUrl: 'https://staging.moneymux.com',
  deviceId: 'device_fixture',
  deviceToken: 'ssdev_fixture_not_a_secret',
}

function jsonResponse(body, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: async () => JSON.stringify(body),
  }
}

test('selects editor inventory and always closes its setup session', async () => {
  const calls = []
  const responses = [
    jsonResponse({ id: 'session_editor' }),
    jsonResponse({
      adDecisionId: 'decision_editor',
      creative: {
        sponsor: 'Railway',
        url: 'https://railway.app',
        headline: 'Deploy now',
        disclosure: 'Sponsored',
      },
    }),
    jsonResponse({ ok: true }),
  ]
  const creative = await fetchEditorCreative(config, {
    fetchImpl: async (url, request) => {
      calls.push({ url, request })
      return responses.shift()
    },
  })

  assert.equal(creative.sponsor, 'Railway')
  assert.deepEqual(calls.map((call) => new URL(call.url).pathname), [
    '/api/terminal-sessions',
    '/api/ad-decision',
    '/api/terminal-sessions/session_editor/end',
  ])
  const decisionBody = JSON.parse(calls[1].request.body)
  assert.equal(decisionBody.placement, 'editor_overlay')
  assert.equal(decisionBody.isTty, false)
  assert.equal(decisionBody.isInteractive, true)
  assert.ok(calls.every((call) => !call.url.includes('/api/events/')))
})

test('closes the setup session when inventory selection fails', async () => {
  const calls = []
  await assert.rejects(
    fetchEditorCreative(config, {
      fetchImpl: async (url) => {
        calls.push(url)
        if (calls.length === 1) return jsonResponse({ id: 'session_failed' })
        if (calls.length === 2) return jsonResponse({ error: 'later' }, 503)
        return jsonResponse({ ok: true })
      },
    }),
    /HTTP 503/
  )
  assert.match(calls[2], /session_failed\/end$/)
})

test('returns null rather than inventing an advertiser when inventory is empty', async () => {
  const responses = [
    jsonResponse({ id: 'session_empty' }),
    jsonResponse({ creative: null }),
    jsonResponse({ ok: true }),
  ]
  const creative = await fetchEditorCreative(config, {
    fetchImpl: async () => responses.shift(),
  })
  assert.equal(creative, null)
})
