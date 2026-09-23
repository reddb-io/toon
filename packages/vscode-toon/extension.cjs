'use strict'

/**
 * VS Code wiring for the RedDB Toon extension: strict-decode diagnostics,
 * canonical formatting, JSON↔TOON conversion commands and a size/token status
 * item. The logic lives in `lib/core.cjs`; the codec is the vendored
 * `@reddb-io/toon` build under `dist/codec/` (see `scripts/vendor-codec.mjs`).
 */

const path = require('node:path')
const { pathToFileURL } = require('node:url')
const vscode = require('vscode')

const core = require('./lib/core.cjs')

const TOON_LANGUAGES = new Set(['toon', 'toonl'])
const VALIDATE_DELAY_MS = 250

let codecPromise
let tokensPromise

/** The codec is ESM; a CommonJS extension host reaches it through import(). */
function loadCodec() {
  codecPromise ??= import(pathToFileURL(path.join(__dirname, 'dist', 'codec', 'index.js')).href)
  return codecPromise
}

function loadTokenEstimator() {
  tokensPromise ??= import(pathToFileURL(path.join(__dirname, 'dist', 'codec', 'cli', 'tokens.js')).href)
    .then((module) => module.estimateTokenCount)
  return tokensPromise
}

function settings() {
  return vscode.workspace.getConfiguration('reddbToon')
}

function encodeOptions(indentSize) {
  const delimiter = { comma: ',', tab: '\t', pipe: '|' }[settings().get('delimiter', 'comma')] ?? ','
  return { delimiter, indentSize }
}

function activate(context) {
  const diagnostics = vscode.languages.createDiagnosticCollection('toon')
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100)
  const timers = new Map()
  context.subscriptions.push(diagnostics, status)

  async function validate(document) {
    if (!TOON_LANGUAGES.has(document.languageId)) return
    if (!settings().get('validate', true)) {
      diagnostics.delete(document.uri)
      return
    }
    const found = core.validate(await loadCodec(), document.languageId, document.getText())
    if (!found) {
      diagnostics.set(document.uri, [])
      return
    }
    const range = new vscode.Range(found.line, found.start, found.line, found.end)
    const diagnostic = new vscode.Diagnostic(range, found.message, vscode.DiagnosticSeverity.Error)
    diagnostic.source = 'toon'
    if (found.code) diagnostic.code = found.code
    diagnostics.set(document.uri, [diagnostic])
  }

  function scheduleValidation(document) {
    const key = document.uri.toString()
    clearTimeout(timers.get(key))
    timers.set(key, setTimeout(() => {
      timers.delete(key)
      validate(document)
    }, VALIDATE_DELAY_MS))
  }

  async function updateStatus() {
    const document = vscode.window.activeTextEditor?.document
    if (!document || (!TOON_LANGUAGES.has(document.languageId) && document.languageId !== 'json')) {
      status.hide()
      return
    }
    const label = core.statusText(await loadCodec(), await loadTokenEstimator(), document.languageId, document.getText())
    if (!label) {
      status.hide()
      return
    }
    status.text = label
    status.show()
  }

  async function convert(target) {
    const editor = vscode.window.activeTextEditor
    if (!editor) return
    const document = editor.document
    const text = editor.selection.isEmpty ? document.getText() : document.getText(editor.selection)
    try {
      const codec = await loadCodec()
      const content = target === 'toon'
        ? core.jsonToToon(codec, text, encodeOptions(2))
        : core.toonToJson(codec, text, document.languageId)
      const opened = await vscode.workspace.openTextDocument({ language: target === 'toon' ? 'toon' : 'json', content })
      await vscode.window.showTextDocument(opened, { preview: false })
    } catch (error) {
      vscode.window.showErrorMessage(`RedDB Toon: ${error.message}`)
    }
  }

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(validate),
    vscode.workspace.onDidChangeTextDocument((event) => {
      scheduleValidation(event.document)
      if (event.document === vscode.window.activeTextEditor?.document) updateStatus()
    }),
    vscode.workspace.onDidCloseTextDocument((document) => diagnostics.delete(document.uri)),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration('reddbToon')) vscode.workspace.textDocuments.forEach(validate)
    }),
    vscode.window.onDidChangeActiveTextEditor(updateStatus),
    vscode.languages.registerDocumentFormattingEditProvider('toon', {
      async provideDocumentFormattingEdits(document, options) {
        try {
          const result = core.formatToon(await loadCodec(), document.getText(), encodeOptions(options.tabSize))
          if (result.skipped) {
            vscode.window.showInformationMessage(`RedDB Toon: ${result.skipped}`)
            return []
          }
          const whole = new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length))
          return [vscode.TextEdit.replace(whole, result.text)]
        } catch {
          // Diagnostics already point at the error; formatting leaves invalid text alone.
          return []
        }
      },
    }),
    vscode.commands.registerCommand('reddbToon.convertJsonToToon', () => convert('toon')),
    vscode.commands.registerCommand('reddbToon.convertToonToJson', () => convert('json')),
  )

  vscode.workspace.textDocuments.forEach(validate)
  updateStatus()
}

function deactivate() {}

module.exports = { activate, deactivate }
