import { toonError } from '../errors.js'
import { splitLines } from '../lexical.js'
import { DEFAULT_INDENT, DEFAULT_MAX_DEPTH } from './constants.js'
export function resolveOptions(options = {}) {
  const rawMaxDepth = options.maxDepth ?? DEFAULT_MAX_DEPTH
  const maxDepth = rawMaxDepth === Number.POSITIVE_INFINITY ? 0 : Math.max(0, Math.floor(rawMaxDepth))
  return {
    indent: Math.max(1, options.indent ?? DEFAULT_INDENT),
    strict: options.strict ?? true,
    // The spec spells this `expandPaths: "safe"`; a boolean is accepted too.
    expandPaths: options.expandPaths === 'safe' || options.expandPaths === true,
    cyclicDiscriminatedArrays: options.cyclicDiscriminatedArrays !== false,
    maxDepth,
  }
}

// ---------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------

export function collectLines(input, options) {
  const lines = []
  let blankBefore = false

  splitLines(input).forEach((rawLine, index) => {
    const number = index + 1
    if (rawLine.trim() === '') {
      blankBefore = true
      return
    }

    const spaces = rawLine.length - rawLine.replace(/^ +/, '').length
    if (rawLine[spaces] === '\t') {
      throw toonError(number, 'invalid indentation')
    }
    if (options.strict && spaces % options.indent !== 0) {
      throw toonError(number, 'invalid indentation')
    }

    const depth = Math.floor(spaces / options.indent)
    checkDepth(depth, number, options)

    lines.push({
      number,
      depth,
      content: rawLine.slice(spaces),
      blankBefore,
    })
    blankBefore = false
  })

  return lines
}

export function checkDepth(depth, line, options) {
  if (options.maxDepth !== 0 && depth > options.maxDepth) {
    throw toonError(line, `maximum nesting depth exceeded (maxDepth ${options.maxDepth})`)
  }
}

export function checkHeaderDepth(header, line, options) {
  if (options.maxDepth === 0) {
    return
  }
  let depth = 0
  let inString = false
  let escaped = false
  for (const character of header) {
    if (escaped) {
      escaped = false
      continue
    }
    if (inString && character === '\\') {
      escaped = true
      continue
    }
    if (character === '"') {
      inString = !inString
      continue
    }
    if (inString) {
      continue
    }
    if (character === '{') {
      depth += 1
      checkDepth(depth, line, options)
    } else if (character === '}') {
      depth = Math.max(0, depth - 1)
    }
  }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/** Decodes TOON per spec §5 root-form discovery. */
