'use strict'

const MAX_LOGO_BYTES = 64 * 1024
const IMAGE_DATA_URL = /^data:(image\/(?:png|jpeg|webp));base64,([A-Za-z0-9+/]+={0,2})$/

function requiredText(value, name, maxLength) {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`Creative ${name} must be a non-empty string.`)
  }
  return value.trim().slice(0, maxLength)
}

function canonicalHttpsUrl(value) {
  const url = new URL(requiredText(value, 'url', 2_048))
  if (url.protocol !== 'https:' || url.username || url.password) {
    throw new Error('Creative URL must be canonical HTTPS without embedded credentials.')
  }
  return url.toString()
}

function safeLogoDataUrl(value) {
  if (value === undefined || value === null || value === '') return null
  if (typeof value !== 'string') throw new Error('Creative logo must be an image data URL.')
  const match = IMAGE_DATA_URL.exec(value)
  if (!match) throw new Error('Creative logo must be a base64 PNG, JPEG, or WebP image.')
  const bytes = Buffer.from(match[2], 'base64')
  if (bytes.length === 0 || bytes.length > MAX_LOGO_BYTES) {
    throw new Error(`Creative logo must be between 1 byte and ${MAX_LOGO_BYTES} bytes.`)
  }
  return `data:${match[1]};base64,${bytes.toString('base64')}`
}

function normalizeCreative(value) {
  if (!value || typeof value !== 'object') throw new Error('Creative payload is missing.')
  return Object.freeze({
    sponsor: requiredText(value.sponsor, 'sponsor', 32),
    url: canonicalHttpsUrl(value.url),
    headline: requiredText(value.headline ?? value.cta ?? 'Learn more', 'headline', 120),
    disclosure: requiredText(value.disclosure ?? 'Sponsored', 'disclosure', 80),
    logoDataUrl: safeLogoDataUrl(value.logoDataUrl),
  })
}

function initialsForSponsor(sponsor) {
  const words = sponsor.match(/[\p{L}\p{N}]+/gu) ?? []
  const initials = words.slice(0, 2).map((word) => Array.from(word)[0]).join('')
  return (initials || 'AD').toLocaleUpperCase().slice(0, 2)
}

module.exports = {
  MAX_LOGO_BYTES,
  initialsForSponsor,
  normalizeCreative,
  safeLogoDataUrl,
}
