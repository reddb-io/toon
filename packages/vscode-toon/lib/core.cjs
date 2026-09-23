'use strict'

/**
 * Editor-independent logic for the extension. Every function takes the codec
 * module (`@reddb-io/toon`) as an argument, so tests drive it with the
 * workspace build while the extension passes its vendored copy.
 */

/** Turns a decoder failure into a 0-based range the editor can underline. */
function diagnosticFor(error, text) {
  const lines = text.split(/\r?\n/)
  const line = Number.isInteger(error?.line) && error.line > 0 ? Math.min(error.line, lines.length) - 1 : 0
  const content = lines[line] ?? ''
  const indent = content.length - content.trimStart().length
  const start = Number.isInteger(error?.column) ? Math.min(error.column - 1, content.length) : indent
  return {
    line,
    start,
    end: Math.max(start + 1, content.length),
    message: error?.reason ?? error?.message ?? String(error),
    code: error?.kind,
  }
}

/** Strict-decodes a document; returns `null` when it is valid. */
function validate(codec, languageId, text) {
  try {
    if (languageId === 'toonl') codec.parseRecords(text)
    else codec.decode(text)
    return null
  } catch (error) {
    return diagnosticFor(error, text)
  }
}

/**
 * Canonical re-encoding of a TOON document. Formatting goes through the JSON
 * value, so a document with comment lines is left alone rather than stripped.
 */
function formatToon(codec, text, options = {}) {
  if (/^ *#/m.test(text)) return { skipped: 'Formatting would drop the comment lines in this document.' }
  const formatted = codec.encode(codec.decode(text), options)
  return { text: text.endsWith('\n') ? `${formatted}\n` : formatted }
}

function jsonToToon(codec, text, options = {}) {
  return codec.encode(JSON.parse(text), options)
}

function toonToJson(codec, text, languageId = 'toon') {
  const value = languageId === 'toonl' ? codec.parseRecords(text) : codec.decode(text)
  return `${JSON.stringify(value, null, 2)}\n`
}

/** The status-bar label: size and estimated tokens, or the TOON saving for JSON. */
function statusText(codec, estimateTokens, languageId, text) {
  if (languageId === 'json') {
    try {
      const json = estimateTokens(text)
      if (json === 0) return undefined
      const toon = estimateTokens(codec.encode(JSON.parse(text)))
      return `TOON ${Math.round(((toon - json) / json) * 100)}% tokens`
    } catch {
      return undefined
    }
  }
  return `${formatBytes(Buffer.byteLength(text))} · ~${estimateTokens(text)} tok`
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

module.exports = { diagnosticFor, formatToon, jsonToToon, statusText, toonToJson, validate }
