/**
 * Scalars, quoted strings, keys, delimiters and numbers — the lexical layer
 * TOON (§4, §7, §11) and TOONL both build on.
 */

import { toonError } from './errors.js'

/** The document delimiter of the default profile (spec §11.1). */
export const DOCUMENT_DELIMITER = ','

/**
 * Splits like Rust's `str::lines`: on `\n`, dropping the trailing empty piece a
 * final newline would otherwise produce, and stripping a `\r` before each `\n`.
 */
export function splitLines(input) {
  const lines = input.split('\n')
  if (lines.length > 0 && lines[lines.length - 1] === '') {
    lines.pop()
  }
  return lines.map((line) => (line.endsWith('\r') ? line.slice(0, -1) : line))
}

function invalidQuotedString(line) {
  return toonError(line, 'invalid quoted string')
}

/** Decodes a scalar token (spec §4): quoted string, literal, number, or bare string. */
export function parseScalar(value, line) {
  if (value === '') {
    return ''
  }
  if (value.startsWith('"')) {
    return parseQuotedString(value, line)
  }
  if (value.includes('"')) {
    throw invalidQuotedString(line)
  }
  if (value === 'true') return true
  if (value === 'false') return false
  if (value === 'null') return null
  if (isNumberToken(value)) return Number(value)
  return value
}

/** Returns `[key, quoted]`. An empty key is only legal when it was quoted. */
export function parseKey(value, line) {
  const trimmed = value.trim()
  if (trimmed.startsWith('"')) {
    return [parseQuotedString(trimmed, line), true]
  }
  if (trimmed.includes('"') || /\s/.test(trimmed)) {
    throw toonError(line, 'expected non-empty field name')
  }
  return [trimmed, false]
}

/**
 * Characters that interrupt a plain run inside a quoted token: the closing
 * quote, an escape, or a C0 control that §7.1 forbids literally (HTAB is
 * tolerated; LF/CR cannot appear because the input is already line-split, but
 * they are still matched so any stray occurrence is rejected).
 */
const QUOTED_STRING_SPECIAL = /["\\\u0000-\u0008\u000a-\u001f]/g

export function parseQuotedString(value, line) {
  if (value.charCodeAt(0) !== 0x22) {
    throw invalidQuotedString(line)
  }

  // Jumps between special characters with a regex scan and copies the plain
  // runs in between with slice — appending one character at a time thrashes
  // the GC on large strings (e.g. HTML payloads).
  let output = ''
  let index = 1
  while (index < value.length) {
    QUOTED_STRING_SPECIAL.lastIndex = index
    const match = QUOTED_STRING_SPECIAL.exec(value)
    if (match === null) {
      break
    }

    if (match[0] === '"') {
      // The closing quote must end the token; only trailing whitespace may follow.
      if (value.slice(match.index + 1).trim() === '') {
        return output + value.slice(index, match.index)
      }
      throw invalidQuotedString(line)
    }

    if (match[0] !== '\\') {
      // A literal C0 control other than HTAB must be escaped (§7.1).
      throw invalidQuotedString(line)
    }

    output += value.slice(index, match.index)
    const escaped = value[match.index + 1]
    index = match.index + 2
    switch (escaped) {
      case '"':
        output += '"'
        break
      case '\\':
        output += '\\'
        break
      case 'n':
        output += '\n'
        break
      case 'r':
        output += '\r'
        break
      case 't':
        output += '\t'
        break
      case 'u': {
        const digits = value.slice(index, index + 4)
        if (!/^[0-9a-fA-F]{4}$/.test(digits)) {
          throw invalidQuotedString(line)
        }
        const code = Number.parseInt(digits, 16)
        // Lone surrogates are rejected, as §7.1 requires.
        if (code >= 0xd800 && code <= 0xdfff) {
          throw invalidQuotedString(line)
        }
        output += String.fromCharCode(code)
        index += 4
        break
      }
      default:
        throw invalidQuotedString(line)
    }
  }

  throw invalidQuotedString(line)
}

/**
 * Advances past the quoted-string content starting right after an opening
 * quote at `index - 1`, jumping between `\` and `"` with `indexOf` instead of
 * walking character by character. Returns the index just past the closing
 * quote, or throws when the string is unterminated or ends in a dangling
 * escape.
 */
function skipQuotedRun(value, index, line) {
  while (true) {
    const quote = value.indexOf('"', index)
    const backslash = value.indexOf('\\', index)
    if (backslash !== -1 && (quote === -1 || backslash < quote)) {
      if (backslash === value.length - 1) {
        throw invalidQuotedString(line)
      }
      index = backslash + 2
      continue
    }
    if (quote === -1) {
      throw invalidQuotedString(line)
    }
    return quote + 1
  }
}

/** Splits on unquoted occurrences of `delimiter`, preserving empty tokens (§11.2). */
export function splitDelimited(value, delimiter, line) {
  if (value === '') {
    return []
  }

  const values = []
  let start = 0
  let index = 0

  while (index < value.length) {
    const quote = value.indexOf('"', index)
    const delim = value.indexOf(delimiter, index)
    if (delim !== -1 && (quote === -1 || delim < quote)) {
      values.push(value.slice(start, delim).trim())
      start = delim + 1
      index = delim + 1
    } else if (quote !== -1) {
      index = skipQuotedRun(value, quote + 1, line)
    } else {
      break
    }
  }

  values.push(value.slice(start).trim())
  return values
}

/** Index of the first `needle` outside a quoted string, or `-1`. */
export function findUnquoted(value, needle, line) {
  let index = 0

  while (index < value.length) {
    const quote = value.indexOf('"', index)
    const found = value.indexOf(needle, index)
    if (found !== -1 && (quote === -1 || found < quote)) {
      return found
    }
    if (quote === -1) {
      return -1
    }
    index = skipQuotedRun(value, quote + 1, line)
  }

  return -1
}

/**
 * A decoder-visible number: `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
 * Leading zeros in the integer part make the token a string (§4).
 */
export function isNumberToken(value) {
  return /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$/.test(value)
}

/**
 * The §7.2 "numeric-like" test used for quoting: unlike {@link isNumberToken} it
 * also matches leading-zero forms such as `05`, which decode as strings but must
 * still be quoted so they never decode as numbers.
 */
export function isNumericLike(value) {
  return /^-?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$/.test(value)
}

/** Canonical decimal form per §2. JS already prints the shortest round-trip form. */
export function numberText(value) {
  if (Object.is(value, -0)) {
    return '0'
  }
  if (!Number.isFinite(value)) {
    throw toonError(0, 'number is not representable in TOON')
  }
  return String(value)
}

export function isPrimitive(value) {
  return (
    value === null ||
    typeof value === 'boolean' ||
    typeof value === 'number' ||
    typeof value === 'string'
  )
}

export function primitiveText(value, delimiter) {
  if (value === null) return 'null'
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  if (typeof value === 'number') return numberText(value)
  if (typeof value === 'string') return canonicalString(value, delimiter)
  throw toonError(0, 'not a primitive')
}

/** Unquoted keys must match `^[A-Za-z_][A-Za-z0-9_.]*$` (§7.3). */
export function isBareKey(value) {
  return /^[A-Za-z_][A-Za-z0-9_.]*$/.test(value)
}

export function canonicalKey(value) {
  return isBareKey(value) ? value : quoteString(value)
}

export function canonicalString(value, delimiter) {
  return needsQuotes(value, delimiter) ? quoteString(value) : value
}

/** The §7.2 quoting checklist. */
export function needsQuotes(value, delimiter) {
  return (
    value === '' ||
    value.trim() !== value ||
    value === 'true' ||
    value === 'false' ||
    value === 'null' ||
    isNumericLike(value) ||
    /[:"\\[\]{}]/.test(value) ||
    /[\u0000-\u001f]/.test(value) ||
    value.includes(delimiter) ||
    value.startsWith('-')
  )
}

const QUOTE_ESCAPED = /["\\\u0000-\u001f]/g

export function quoteString(value) {
  // Jumps between characters that need escaping with a regex scan and copies
  // the plain runs in between with slice — appending one character at a time
  // thrashes the GC on large strings (e.g. HTML payloads).
  let output = '"'
  let runStart = 0
  QUOTE_ESCAPED.lastIndex = 0
  let match
  while ((match = QUOTE_ESCAPED.exec(value)) !== null) {
    const code = value.charCodeAt(match.index)
    let escape
    if (code === 0x22) {
      escape = '\\"'
    } else if (code === 0x5c) {
      escape = '\\\\'
    } else if (code === 0x0a) {
      escape = '\\n'
    } else if (code === 0x0d) {
      escape = '\\r'
    } else if (code === 0x09) {
      escape = '\\t'
    } else {
      escape = `\\u${code.toString(16).padStart(4, '0')}`
    }
    output += value.slice(runStart, match.index) + escape
    runStart = match.index + 1
  }
  return `${output}${value.slice(runStart)}"`
}

/**
 * Defines an own enumerable property even when the key is `__proto__`, which a
 * plain assignment would silently route to the prototype instead of the object.
 */
export function setKey(object, key, value) {
  Object.defineProperty(object, key, {
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  })
}
