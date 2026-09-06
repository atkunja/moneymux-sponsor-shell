'use strict'

const vscode = require('vscode')

const { fetchEditorCreative } = require('./lib/api-client')
const { loadSponsorShellConfig, publicConfigSummary } = require('./lib/config')
const { installTarget, restoreTarget, targetStatus } = require('./lib/patcher')
const { discoverTargets } = require('./lib/targets')

const ENABLED_SETTING = 'editorPlacement.enabled'

function targetSummary(targets) {
  return targets.map((target) => `${target.label} ${target.version}`).join(' and ')
}

async function requestReload(message) {
  const selected = await vscode.window.showInformationMessage(message, 'Reload window')
  if (selected === 'Reload window') {
    await vscode.commands.executeCommand('workbench.action.reloadWindow')
  }
}

function explainError(error) {
  return error instanceof Error ? error.message : 'Unknown MoneyMux editor placement error.'
}

async function installEditorPlacement() {
  const targets = discoverTargets(vscode)
  if (targets.length === 0) {
    throw new Error('Install the Claude Code or Codex VS Code extension first, then retry.')
  }

  const selected = await vscode.window.showWarningMessage(
    `Install a MoneyMux sponsored card in ${targetSummary(targets)}? ` +
      'MoneyMux will modify one webview JavaScript bundle per editor extension and keep a byte-exact local backup. ' +
      'The card is visibly labeled, opt-in, reversible, and not billable.',
    { modal: true },
    'Install placement'
  )
  if (selected !== 'Install placement') return []

  const config = loadSponsorShellConfig()
  const creative = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Notification, title: 'Selecting approved MoneyMux creative' },
    () => fetchEditorCreative(config)
  )
  if (!creative) throw new Error('No approved creative is eligible for this linked terminal right now.')

  const results = targets.map((target) => installTarget(target, creative))
  await vscode.workspace.getConfiguration('moneymux').update(
    ENABLED_SETTING,
    true,
    vscode.ConfigurationTarget.Global
  )
  await requestReload(`MoneyMux installed the sponsored card for ${targetSummary(targets)}.`)
  return results
}

async function refreshEditorPlacement() {
  const targets = discoverTargets(vscode).filter((target) => targetStatus(target).installed)
  if (targets.length === 0) throw new Error('No MoneyMux editor placement is installed yet.')
  const config = loadSponsorShellConfig()
  const creative = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Notification, title: 'Refreshing approved MoneyMux creative' },
    () => fetchEditorCreative(config)
  )
  if (!creative) throw new Error('No approved creative is eligible for this linked terminal right now.')
  const results = targets.map((target) => installTarget(target, creative))
  await requestReload(`MoneyMux refreshed the sponsored card for ${targetSummary(targets)}.`)
  return results
}

async function restoreEditorPlacement() {
  const targets = discoverTargets(vscode)
  const restorable = targets.filter((target) => targetStatus(target).backupAvailable)
  if (restorable.length === 0) {
    await vscode.window.showInformationMessage('No restorable MoneyMux editor placement was found.')
    return []
  }
  const selected = await vscode.window.showWarningMessage(
    `Restore the original ${targetSummary(restorable)} webview bundle${restorable.length === 1 ? '' : 's'}?`,
    { modal: true },
    'Restore originals'
  )
  if (selected !== 'Restore originals') return []
  const results = restorable.map(restoreTarget)
  await vscode.workspace.getConfiguration('moneymux').update(
    ENABLED_SETTING,
    false,
    vscode.ConfigurationTarget.Global
  )
  await requestReload(`MoneyMux restored ${targetSummary(restorable)} from byte-exact backups.`)
  return results
}

async function showEditorPlacementStatus() {
  const targets = discoverTargets(vscode)
  const config = loadSponsorShellConfig()
  const publicConfig = publicConfigSummary(config)
  const lines = targets.length > 0
    ? targets.map((target) => {
      const status = targetStatus(target)
      return `${status.label} ${status.version}: ${status.installed ? 'installed' : 'not installed'}, ` +
        `${status.compatible ? 'compatible' : 'incompatible'}, ` +
        `${status.backupAvailable ? 'backup ready' : 'no backup'}`
    })
    : ['Claude Code/Codex: not installed']
  await vscode.window.showInformationMessage(
    `MoneyMux ${publicConfig.apiBaseUrl} · linked device ${publicConfig.deviceId} · ${lines.join(' · ')}`
  )
  return { config: publicConfig, targets: targets.map(targetStatus) }
}

function guarded(command) {
  return async () => {
    try {
      return await command()
    } catch (error) {
      await vscode.window.showErrorMessage(explainError(error))
      return undefined
    }
  }
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand('moneymux.installEditorPlacement', guarded(installEditorPlacement)),
    vscode.commands.registerCommand('moneymux.refreshEditorPlacement', guarded(refreshEditorPlacement)),
    vscode.commands.registerCommand('moneymux.restoreEditorPlacement', guarded(restoreEditorPlacement)),
    vscode.commands.registerCommand('moneymux.editorPlacementStatus', guarded(showEditorPlacementStatus))
  )
}

function deactivate() {}

module.exports = {
  activate,
  deactivate,
  explainError,
  installEditorPlacement,
  refreshEditorPlacement,
  restoreEditorPlacement,
  showEditorPlacementStatus,
  targetSummary,
}

