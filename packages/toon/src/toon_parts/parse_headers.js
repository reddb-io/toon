import { toonError } from '../errors.js'
import { DOCUMENT_DELIMITER, findUnquoted, parseKey, parseScalar, splitDelimited } from '../lexical.js'
export function lengthMismatch(lines, index) {
  const line = lines[index] ?? lines[lines.length - 1]
  return toonError(line === undefined ? 1 : line.number, 'array length mismatch')
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

/**
 * Parses `key[N<delim?>]{fields}:` (spec §6). `colon` is the first unquoted colon
 * on the line; the header must terminate exactly there. Throws with `line: 0`, so
 * callers stamp their own line number via {@link atLine}.
 */
export function parseHeader(content, colon) {
  if (colon === -1) {
    throw toonError(0, 'array header missing colon')
  }
  const keyPart = content.slice(0, colon)

  const open = findUnquoted(keyPart, '[', 0)
  if (open === -1) {
    throw toonError(0, 'invalid array header')
  }
  let key
  let keyQuoted
  try {
    ;[key, keyQuoted] = parseKey(keyPart.slice(0, open), 0)
  } catch {
    throw toonError(0, 'invalid array header')
  }

  const rest = keyPart.slice(open + 1)
  const close = rest.indexOf(']')
  if (close === -1) {
    throw toonError(0, 'invalid array header')
  }
  const bracket = rest.slice(0, close)

  const digits = /^[0-9]*/.exec(bracket)[0]
  if (digits === '' || (digits.length > 1 && digits.startsWith('0'))) {
    throw toonError(0, 'invalid array header')
  }
  const len = Number.parseInt(digits, 10)

  let delimiter
  switch (bracket.slice(digits.length)) {
    case '':
      delimiter = ','
      break
    case '\t':
      delimiter = '\t'
      break
    case '|':
      delimiter = '|'
      break
    default:
      throw toonError(0, 'invalid array header')
  }

  const suffix = rest.slice(close + 1)
  let fields
  let fieldTree
  if (suffix === '') {
    fields = undefined
    fieldTree = undefined
  } else if (suffix.startsWith('{') && suffix.endsWith('}') && suffix.length >= 2) {
    try {
      fieldTree = parseArrayHeaderFieldTree(suffix.slice(1, -1), delimiter)
      fields = flattenHeaderFieldTree(fieldTree)
    } catch (error) {
      throw toonError(0, error.reason === 'duplicate key' ? 'duplicate key' : 'invalid array header')
    }
  } else {
    throw toonError(0, 'invalid array header')
  }

  return { key, keyQuoted, len, delimiter, fields, fieldTree }
}

export function parseArrayHeaderFields(source, delimiter) {
  if (delimiter !== DOCUMENT_DELIMITER && source.includes(DOCUMENT_DELIMITER) && source.includes('[')) {
    return parseHeaderFields(source, DOCUMENT_DELIMITER, delimiter)
  }
  try {
    return parseHeaderFields(source, delimiter, delimiter)
  } catch (error) {
    if (delimiter !== DOCUMENT_DELIMITER && error.reason !== 'duplicate key') {
      return parseHeaderFields(source, DOCUMENT_DELIMITER, delimiter)
    }
    throw error
  }
}

export function parseMapHeader(content) {
  const open = findUnquoted(content, '{', 0)
  if (open === -1 || !content.endsWith('}')) {
    throw toonError(0, 'invalid keyed map header')
  }
  let key
  let keyQuoted
  try {
    ;[key, keyQuoted] = parseKey(content.slice(0, open), 0)
  } catch {
    throw toonError(0, 'invalid keyed map header')
  }
  if (key === '' && !keyQuoted) {
    throw toonError(0, 'expected non-empty field name')
  }

  let fieldText = content.slice(open + 1, -1)
  let delimiter = DOCUMENT_DELIMITER
  if (fieldText.startsWith('|') || fieldText.startsWith('\t')) {
    delimiter = fieldText[0]
    fieldText = fieldText.slice(1)
  }

  let fields
  try {
    fields = parseHeaderFields(fieldText, delimiter, delimiter)
  } catch (error) {
    throw toonError(0, error.reason === 'duplicate key' ? 'duplicate key' : 'invalid keyed map header')
  }
  return { key, keyQuoted, delimiter, fields }
}

export function parseHeaderFields(source, delimiter, activeDelimiter) {
  const fields = []
  let index = 0

  function parseList(prefix, nested) {
    let count = 0

    while (index < source.length) {
      if (nested && source[index] === '}') {
        break
      }
      if (source[index] === delimiter || source[index] === '}') {
        throw toonError(0, 'invalid array header')
      }

      const start = index
      while (index < source.length) {
        const character = source[index]
        if (character === '"') {
          skipQuotedHeaderKey()
          continue
        }
        if (character === delimiter || character === '{' || character === '}' || character === '[') {
          break
        }
        index += 1
      }

      const [key] = parseKey(source.slice(start, index), 0)
      if (key === '') {
        throw toonError(0, 'invalid array header')
      }

      count += 1
      if (source[index] === '[') {
        index += 1
        const delimiterStart = index
        while (index < source.length && source[index] !== ']') {
          index += 1
        }
        if (index >= source.length) {
          throw toonError(0, 'invalid array header')
        }
        const listDelimiter = source.slice(delimiterStart, index)
        if (!isValidListDelimiter(listDelimiter, activeDelimiter)) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
        fields.push({ path: [...prefix, key], listDelimiter })
      } else if (source[index] === '{') {
        index += 1
        const before = fields.length
        parseList([...prefix, key], true)
        if (source[index] !== '}' || fields.length === before) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
      } else {
        fields.push({ path: [...prefix, key], listDelimiter: undefined })
      }

      if (source[index] === delimiter) {
        index += 1
        if (index >= source.length || source[index] === '}') {
          throw toonError(0, 'invalid array header')
        }
        continue
      }
      if (nested ? source[index] === '}' : index === source.length) {
        break
      }
      throw toonError(0, 'invalid array header')
    }

    if (count === 0) {
      throw toonError(0, 'invalid array header')
    }
  }

  function skipQuotedHeaderKey() {
    index += 1
    while (index < source.length) {
      const character = source[index]
      index += 1
      if (character === '\\') {
        index += 1
      } else if (character === '"') {
        return
      }
    }
  }

  parseList([], false)
  if (index !== source.length) {
    throw toonError(0, 'invalid array header')
  }

  const seen = new Set()
  for (let index = 0; index < fields.length; index += 1) {
    for (let other = index + 1; other < fields.length; other += 1) {
      if (
        samePath(fields[index].path, fields[other].path) ||
        pathStartsWith(fields[index].path, fields[other].path) ||
        pathStartsWith(fields[other].path, fields[index].path)
      ) {
        throw toonError(0, 'duplicate key')
      }
    }
  }
  for (const field of fields) {
    const path = field.path
    const key = path.join('\u0000')
    if (seen.has(key)) {
      throw toonError(0, 'duplicate key')
    }
    seen.add(key)
  }
  return fields
}

export function parseArrayHeaderFieldTree(source, delimiter) {
  if (
    delimiter !== DOCUMENT_DELIMITER &&
    source.includes(DOCUMENT_DELIMITER) &&
    (source.includes('[') || source.includes('{'))
  ) {
    return parseHeaderFieldTree(source, DOCUMENT_DELIMITER, delimiter)
  }
  try {
    return parseHeaderFieldTree(source, delimiter, delimiter)
  } catch (error) {
    if (delimiter !== DOCUMENT_DELIMITER && error.reason !== 'duplicate key') {
      return parseHeaderFieldTree(source, DOCUMENT_DELIMITER, delimiter)
    }
    throw error
  }
}

export function parseHeaderFieldTree(source, delimiter, activeDelimiter) {
  let index = 0

  function parseList(nested) {
    const fields = []

    while (index < source.length) {
      if (nested && source[index] === '}') {
        break
      }
      if (source[index] === delimiter || source[index] === '}') {
        throw toonError(0, 'invalid array header')
      }

      const start = index
      while (index < source.length) {
        const character = source[index]
        if (character === '"') {
          skipQuotedHeaderKey()
          continue
        }
        if (character === delimiter || character === '{' || character === '}' || character === '[') {
          break
        }
        index += 1
      }

      const [key] = parseKey(source.slice(start, index), 0)
      if (key === '') {
        throw toonError(0, 'invalid array header')
      }

      const field = { key }
      if (source[index] === '[') {
        index += 1
        const bracketStart = index
        while (index < source.length && source[index] !== ']') {
          index += 1
        }
        if (index >= source.length) {
          throw toonError(0, 'invalid array header')
        }
        const bracket = source.slice(bracketStart, index)
        index += 1

        const fixed = parseFixedWidthList(bracket)
        if (fixed !== undefined) {
          if (fixed.delimiter !== activeDelimiter) {
            throw toonError(0, 'invalid array header')
          }
          field.fixedLength = fixed.len
          field.fixedDelimiter = fixed.delimiter
        } else if (isValidListDelimiter(bracket, activeDelimiter)) {
          field.listDelimiter = bracket
        } else {
          throw toonError(0, 'invalid array header')
        }
      } else if (source[index] === '{') {
        index += 1
        field.children = parseList(true)
        if (source[index] !== '}' || field.children.length === 0) {
          throw toonError(0, 'invalid array header')
        }
        index += 1
      }

      fields.push(field)

      if (source[index] === delimiter) {
        index += 1
        if (index >= source.length || source[index] === '}') {
          throw toonError(0, 'invalid array header')
        }
        continue
      }
      if (nested ? source[index] === '}' : index === source.length) {
        break
      }
      throw toonError(0, 'invalid array header')
    }

    if (fields.length === 0) {
      throw toonError(0, 'invalid array header')
    }
    return fields
  }

  function skipQuotedHeaderKey() {
    index += 1
    while (index < source.length) {
      const character = source[index]
      index += 1
      if (character === '\\') {
        index += 1
      } else if (character === '"') {
        return
      }
    }
  }

  const fields = parseList(false)
  if (index !== source.length) {
    throw toonError(0, 'invalid array header')
  }

  const paths = flattenHeaderFieldTree(fields)
  for (let index = 0; index < paths.length; index += 1) {
    for (let other = index + 1; other < paths.length; other += 1) {
      if (
        samePath(paths[index].path, paths[other].path) ||
        pathStartsWith(paths[index].path, paths[other].path) ||
        pathStartsWith(paths[other].path, paths[index].path)
      ) {
        throw toonError(0, 'duplicate key')
      }
    }
  }
  return fields
}

function parseFixedWidthList(value) {
  const digits = /^[0-9]+/.exec(value)?.[0]
  if (digits === undefined || (digits.length > 1 && digits.startsWith('0'))) {
    return undefined
  }
  const suffix = value.slice(digits.length)
  let delimiter
  switch (suffix) {
    case '':
      delimiter = DOCUMENT_DELIMITER
      break
    case '\t':
      delimiter = '\t'
      break
    case '|':
      delimiter = '|'
      break
    default:
      return undefined
  }
  return { len: Number.parseInt(digits, 10), delimiter }
}

export function flattenHeaderFieldTree(fields, prefix = []) {
  return fields.flatMap((field) => {
    const path = [...prefix, field.key]
    if (field.children !== undefined) {
      return flattenHeaderFieldTree(field.children, path)
    }
    return [{ path, listDelimiter: field.listDelimiter }]
  })
}

export function parseTabularCell(field, cell, line) {
  if (field.listDelimiter === undefined) {
    return parseScalar(cell, line)
  }
  return splitDelimited(cell, field.listDelimiter, line).map((value) => parseScalar(value, line))
}

export function isValidListDelimiter(value, activeDelimiter) {
  return (
    value.length === 1 &&
    value !== activeDelimiter &&
    !/[ \t\r\n"[\]{}:]/.test(value)
  )
}

export function samePath(left, right) {
  return left.length === right.length && pathStartsWith(left, right)
}

export function pathStartsWith(path, prefix) {
  return prefix.every((segment, index) => path[index] === segment)
}

export function atLine(error, line) {
  return toonError(line, error.reason ?? String(error.message))
}

// ---------------------------------------------------------------------------
// Field insertion, duplicate keys and path expansion
// ---------------------------------------------------------------------------
