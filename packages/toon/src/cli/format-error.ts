import type { ToonDecodeError, ToonError } from '../errors.js'

/**
 * Renders a decode failure as a header, the offending source line, and a caret
 * under the first character that could have caused it — the upstream
 * `@toon-format/cli` rendering, byte for byte.
 */
export function formatDecodeError(error: ToonDecodeError | ToonError): string {
  // Both positioned errors keep the unprefixed text in `reason`, so the
  // `Line N: ` the message carries never has to be stripped back off.
  const header = `Failed to decode TOON at line ${error.line}: ${error.reason}`

  if (error.source === undefined) {
    return header
  }

  const visibleSource = error.source.replace(/\t/g, '→')
  const firstNonWhitespaceIndex = visibleSource.search(/\S/)
  const gutter = `  ${error.line} | `
  const caretIndent = ' '.repeat(gutter.length + Math.max(firstNonWhitespaceIndex, 0))

  return `${header}\n\n${gutter}${visibleSource}\n${caretIndent}^`
}
