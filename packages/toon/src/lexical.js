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

export function parseQuotedString(value, line) {
  if (value[0] !== '"') {
    throw invalidQuotedString(line)
  }

  let output = ''
  let index = 1
  while (index < value.length) {
    const character = value[index]
    index += 1

    if (character === '"') {
      // The closing quote must end the token; only trailing whitespace may follow.
      if (value.slice(index).trim() === '') {
        return output
      }
      throw invalidQuotedString(line)
    }

    if (character === '\\') {
      const escaped = value[index]
      index += 1
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
      continue
    }

    // Literal HTAB is tolerated; other C0 controls must be escaped (§7.1).
    if (character < ' ' && character !== '\t') {
      throw invalidQuotedString(line)
    }
    output += character
  }

  throw invalidQuotedString(line)
}

/** Splits on unquoted occurrences of `delimiter`, preserving empty tokens (§11.2). */
export function splitDelimited(value, delimiter, line) {
  if (value === '') {
    return []
  }

  const values = []
  let start = 0
  let inString = false
  let escaped = false

  for (let index = 0; index < value.length; index += 1) {
    const character = value[index]
    if (escaped) {
      escaped = false
      continue
    }
    if (character === '\\' && inString) {
      escaped = true
    } else if (character === '"') {
      inString = !inString
    } else if (character === delimiter && !inString) {
      values.push(value.slice(start, index).trim())
      start = index + 1
    }
  }

  if (inString || escaped) {
    throw invalidQuotedString(line)
  }

  values.push(value.slice(start).trim())
  return values
}

/** Index of the first `needle` outside a quoted string, or `-1`. */
export function findUnquoted(value, needle, line) {
  let inString = false
  let escaped = false

  for (let index = 0; index < value.length; index += 1) {
    const character = value[index]
    if (escaped) {
      escaped = false
      continue
    }
    if (character === '\\' && inString) {
      escaped = true
    } else if (character === '"') {
      inString = !inString
    } else if (character === needle && !inString) {
      return index
    }
  }

  if (inString || escaped) {
    throw invalidQuotedString(line)
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

export function quoteString(value) {
  let output = '"'
  for (const character of value) {
    switch (character) {
      case '"':
        output += '\\"'
        break
      case '\\':
        output += '\\\\'
        break
      case '\n':
        output += '\\n'
        break
      case '\r':
        output += '\\r'
        break
      case '\t':
        output += '\\t'
        break
      default:
        if (character < ' ') {
          output += `\\u${character.charCodeAt(0).toString(16).padStart(4, '0')}`
        } else {
          output += character
        }
    }
  }
  return `${output}"`
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
