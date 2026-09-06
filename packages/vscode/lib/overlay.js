'use strict'

const { initialsForSponsor } = require('./creative')

const PATCH_START = '/* MONEYMUX_EDITOR_OVERLAY_START */'
const PATCH_END = '/* MONEYMUX_EDITOR_OVERLAY_END */'

function encodedConfiguration(kind, creative) {
  return Buffer.from(JSON.stringify({
    kind,
    sponsor: creative.sponsor,
    headline: creative.headline,
    disclosure: creative.disclosure,
    destination: creative.url,
    logoDataUrl: creative.logoDataUrl,
    initials: initialsForSponsor(creative.sponsor),
  }), 'utf8').toString('base64')
}

function buildOverlayPatch(kind, creative) {
  if (kind !== 'claude' && kind !== 'codex') throw new Error(`Unsupported editor target: ${kind}`)
  const configuration = encodedConfiguration(kind, creative)
  return `${PATCH_START}
;(() => {
  'use strict';
  if (window.__moneymuxEditorOverlayCleanup) window.__moneymuxEditorOverlayCleanup();
  const config = JSON.parse(new TextDecoder().decode(Uint8Array.from(atob('${configuration}'), value => value.charCodeAt(0))));
  const overlayId = 'moneymux-sponsored-editor-card';
  const styleId = 'moneymux-sponsored-editor-style';
  let scheduled = false;

  function installStyle() {
    if (document.getElementById(styleId)) return;
    const style = document.createElement('style');
    style.id = styleId;
    style.textContent = [
      '.moneymux-editor-card{box-sizing:border-box;display:inline-flex;align-items:center;gap:8px;min-width:0;max-width:min(520px,100%);padding:5px 9px;border:1px solid color-mix(in srgb,var(--vscode-foreground,currentColor) 18%,transparent);border-radius:8px;background:color-mix(in srgb,var(--vscode-editor-background,#171717) 92%,var(--vscode-foreground,#fff) 8%);color:var(--vscode-foreground,inherit);font:500 12px/1.25 var(--vscode-font-family,system-ui,sans-serif);text-decoration:none;vertical-align:middle;box-shadow:0 2px 10px rgba(0,0,0,.10)}',
      '.moneymux-editor-card:hover{border-color:color-mix(in srgb,var(--vscode-textLink-foreground,#7aa2f7) 60%,transparent);background:color-mix(in srgb,var(--vscode-editor-background,#171717) 86%,var(--vscode-textLink-foreground,#7aa2f7) 14%)}',
      '.moneymux-editor-logo{box-sizing:border-box;display:grid;place-items:center;flex:0 0 24px;width:24px;height:24px;border-radius:6px;overflow:hidden;background:var(--vscode-badge-background,#3a3d41);color:var(--vscode-badge-foreground,#fff);font:700 10px/1 var(--vscode-font-family,system-ui,sans-serif);letter-spacing:.02em}',
      '.moneymux-editor-logo img{display:block;width:100%;height:100%;object-fit:contain}',
      '.moneymux-editor-copy{display:block;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
      '.moneymux-editor-disclosure{margin-right:5px;color:var(--vscode-descriptionForeground,#999);font-size:10px;font-weight:650;letter-spacing:.06em;text-transform:uppercase}',
      '.moneymux-editor-sponsor{font-weight:700}',
      '.moneymux-editor-headline{margin-left:5px;color:var(--vscode-descriptionForeground,#aaa)}',
      '.moneymux-editor-arrow{flex:0 0 auto;color:var(--vscode-textLink-foreground,#7aa2f7)}',
      '[class*="spinnerRow_"]>.moneymux-editor-card{margin-left:8px}',
      '.moneymux-editor-codex-host>.moneymux-editor-card{display:flex;width:calc(100% - 24px);margin:0 12px 7px}',
      '@media (max-width:520px){.moneymux-editor-headline{display:none}.moneymux-editor-card{max-width:100%}}',
      '@media (prefers-reduced-motion:no-preference){.moneymux-editor-card{animation:moneymux-card-in 140ms ease-out}}',
      '@keyframes moneymux-card-in{from{opacity:0;transform:translateY(2px)}to{opacity:1;transform:none}}'
    ].join('');
    document.head.appendChild(style);
  }

  function exactText(element, candidates) {
    const text = (element.textContent || '').trim();
    return candidates.includes(text);
  }

  function visible(element) {
    return Boolean(element && element.isConnected && element.getClientRects().length > 0);
  }

  function claudeHost() {
    const working = Array.from(document.querySelectorAll('span')).find(element =>
      exactText(element, ['Claude is working', 'Compacting conversation'])
    );
    return working && (working.closest('[class*="spinnerRow_"]') || working.parentElement);
  }

  function codexHost() {
    const stop = document.querySelector('button[aria-label^="Stop"],button[data-testid*="stop" i],[data-testid="stop-button"]');
    const working = Array.from(document.querySelectorAll('span,div,p')).find(element =>
      exactText(element, ['Working…', 'Working...', 'Thinking…', 'Thinking...']) && visible(element)
    );
    const anchor = stop || working;
    if (!anchor) return null;
    const composer = anchor.closest('form') || anchor.closest('[class*="composer" i]') || anchor.parentElement;
    if (!composer) return null;
    const host = composer.parentElement || composer;
    host.classList.add('moneymux-editor-codex-host');
    return host;
  }

  function createLogo() {
    const frame = document.createElement('span');
    frame.className = 'moneymux-editor-logo';
    if (config.logoDataUrl) {
      const image = document.createElement('img');
      image.src = config.logoDataUrl;
      image.alt = '';
      image.setAttribute('aria-hidden', 'true');
      frame.appendChild(image);
    } else {
      frame.textContent = config.initials;
      frame.setAttribute('aria-hidden', 'true');
    }
    return frame;
  }

  function createCard() {
    const card = document.createElement('a');
    card.id = overlayId;
    card.className = 'moneymux-editor-card';
    card.href = config.destination;
    card.target = '_blank';
    card.rel = 'noopener noreferrer sponsored';
    card.title = config.headline;
    card.setAttribute('aria-label', 'Sponsored by ' + config.sponsor + ': ' + config.headline);
    card.appendChild(createLogo());

    const copy = document.createElement('span');
    copy.className = 'moneymux-editor-copy';
    const disclosure = document.createElement('span');
    disclosure.className = 'moneymux-editor-disclosure';
    disclosure.textContent = 'Sponsored';
    const sponsor = document.createElement('span');
    sponsor.className = 'moneymux-editor-sponsor';
    sponsor.textContent = config.sponsor;
    const headline = document.createElement('span');
    headline.className = 'moneymux-editor-headline';
    headline.textContent = '· ' + config.headline;
    copy.append(disclosure, sponsor, headline);
    card.appendChild(copy);

    const arrow = document.createElement('span');
    arrow.className = 'moneymux-editor-arrow';
    arrow.textContent = '↗';
    arrow.setAttribute('aria-hidden', 'true');
    card.appendChild(arrow);
    return card;
  }

  function render() {
    scheduled = false;
    const existing = document.getElementById(overlayId);
    const host = document.visibilityState === 'visible'
      ? (config.kind === 'claude' ? claudeHost() : codexHost())
      : null;
    if (!host || !visible(host)) {
      if (existing) existing.remove();
      return;
    }
    installStyle();
    if (!existing) host.appendChild(createCard());
    else if (existing.parentElement !== host) host.appendChild(existing);
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(render);
  }

  const observer = new MutationObserver(schedule);
  observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true });
  document.addEventListener('visibilitychange', schedule);
  window.__moneymuxEditorOverlayCleanup = () => {
    observer.disconnect();
    document.removeEventListener('visibilitychange', schedule);
    document.getElementById(overlayId)?.remove();
    document.getElementById(styleId)?.remove();
    delete window.__moneymuxEditorOverlayCleanup;
  };
  schedule();
})();
${PATCH_END}`
}

function stripOverlayPatch(source) {
  const start = source.indexOf(PATCH_START)
  if (start === -1) return source
  const end = source.indexOf(PATCH_END, start)
  if (end === -1) throw new Error('Existing MoneyMux editor patch is incomplete; restore before retrying.')
  return `${source.slice(0, start).trimEnd()}\n${source.slice(end + PATCH_END.length).trimStart()}`
}

function applyOverlayPatch(source, patch) {
  return `${stripOverlayPatch(source).trimEnd()}\n${patch}\n`
}

function hasOverlayPatch(source) {
  return source.includes(PATCH_START) && source.includes(PATCH_END)
}

module.exports = {
  PATCH_END,
  PATCH_START,
  applyOverlayPatch,
  buildOverlayPatch,
  hasOverlayPatch,
  stripOverlayPatch,
}

