/**
 * Token estimates compatible with tokenx 1.3.0, the estimator the pinned
 * upstream TOON CLI uses for `--stats`. Kept in lockstep with the Rust port in
 * `crates/tq/src/cli/token_stats.rs` so both front-ends report the same numbers.
 */

/** Estimates the token count of `text` the way tokenx 1.3.0 does. */
export function estimateTokenCount(text: string): number {
  const characters = [...text]
  let total = 0
  let index = 0

  while (index < characters.length) {
    const first = characters[index]

    if (isJavaScriptWhitespace(first)) {
      index = takeSegment(characters, index, isJavaScriptWhitespace)
    } else if (isPunctuation(first)) {
      const end = takeSegment(characters, index, isPunctuation)
      total += Math.ceil(utf16Length(characters, index, end) / 2)
      index = end
    } else {
      const end = takeSegment(
        characters,
        index,
        (character) => !isJavaScriptWhitespace(character) && !isPunctuation(character),
      )
      total += estimateSegment(characters.slice(index, end))
      index = end
    }
  }

  return total
}

/** Formats the `--stats` report body, matching upstream wording and rounding. */
export function formatStatistics(json: string, toon: string): { estimates: string, saved: string } {
  const jsonTokens = estimateTokenCount(json)
  const toonTokens = estimateTokenCount(toon)
  const difference = jsonTokens - toonTokens
  const percent = ((difference / jsonTokens) * 100).toFixed(1)

  return {
    estimates: `Token estimates: ~${jsonTokens} (JSON) → ~${toonTokens} (TOON)`,
    saved: `Saved ~${difference} tokens (-${percent}%)`,
  }
}

function takeSegment(
  characters: readonly string[],
  start: number,
  matches: (character: string) => boolean,
): number {
  let end = start
  while (end < characters.length && matches(characters[end])) end++
  return end
}

function utf16Length(characters: readonly string[], start: number, end: number): number {
  let length = 0
  for (let index = start; index < end; index++) length += characters[index].length
  return length
}

function estimateSegment(segment: readonly string[]): number {
  if (segment.some(isCjk)) {
    return segment.length
  }
  if (segment.every((character) => character >= '0' && character <= '9')) {
    return 1
  }

  const length = utf16Length(segment, 0, segment.length)
  if (length <= 3) {
    return 1
  }

  switch (languageCharsPerToken(segment)) {
    case 3:
      return Math.ceil(length / 3)
    case 7:
      return Math.ceil((length * 2) / 7)
    default:
      return Math.ceil(length / 6)
  }
}

function languageCharsPerToken(segment: readonly string[]): number | undefined {
  if (segment.some(isGerman) || segment.some(isRomance)) {
    return 3
  }
  if (segment.some(isCentralEuropean)) {
    // `7` represents tokenx's 3.5 characters per token without floats.
    return 7
  }
  return undefined
}

/** ECMAScript whitespace plus line terminators, as tokenx's `\s` matches them. */
const JAVASCRIPT_WHITESPACE_RANGES: readonly (readonly [number, number])[] = [
  [0x0009, 0x000D], [0x0020, 0x0020], [0x00A0, 0x00A0], [0x1680, 0x1680],
  [0x2000, 0x200A], [0x2028, 0x2029], [0x202F, 0x202F], [0x205F, 0x205F],
  [0x3000, 0x3000], [0xFEFF, 0xFEFF],
]

const PUNCTUATION = new Set([
  '.', ',', '!', '?', ';', '(', ')', '{', '}', '[', ']', '<', '>', ':', '/', '\\',
  '|', '@', '#', '$', '%', '^', '&', '*', '+', '=', '`', '~', '_', '-',
])

const CJK_RANGES: readonly (readonly [number, number])[] = [
  [0x4E00, 0x9FFF], [0x3400, 0x4DBF], [0x3000, 0x303F], [0xFF00, 0xFFEF],
  [0x30A0, 0x30FF], [0x2E80, 0x2EFF], [0x31C0, 0x31EF], [0x3200, 0x32FF],
  [0x3300, 0x33FF], [0xAC00, 0xD7AF], [0x1100, 0x11FF], [0x3130, 0x318F],
  [0xA960, 0xA97F], [0xD7B0, 0xD7FF],
]

const GERMAN = new Set(['ä', 'ö', 'ü', 'ß', 'ẞ', 'Ä', 'Ö', 'Ü'])

const ROMANCE = new Set([
  'é', 'è', 'ê', 'ë', 'à', 'â', 'î', 'ï', 'ô', 'û', 'ù', 'ü', 'ÿ', 'ç', 'œ', 'æ',
  'á', 'í', 'ó', 'ú', 'ñ',
])

const CENTRAL_EUROPEAN = new Set([
  'ą', 'ć', 'ę', 'ł', 'ń', 'ó', 'ś', 'ź', 'ż', 'ě', 'š', 'č', 'ř', 'ž', 'ý', 'ů',
  'ú', 'ď', 'ť', 'ň',
])

function isJavaScriptWhitespace(character: string): boolean {
  return inRanges(JAVASCRIPT_WHITESPACE_RANGES, character)
}

function isPunctuation(character: string): boolean {
  return PUNCTUATION.has(character)
}

function isCjk(character: string): boolean {
  return inRanges(CJK_RANGES, character)
}

function inRanges(
  ranges: readonly (readonly [number, number])[],
  character: string,
): boolean {
  const code = character.codePointAt(0)
  return ranges.some(([start, end]) => code >= start && code <= end)
}

function isGerman(character: string): boolean {
  return GERMAN.has(character)
}

function isRomance(character: string): boolean {
  return ROMANCE.has(lowercase(character))
}

function isCentralEuropean(character: string): boolean {
  return CENTRAL_EUROPEAN.has(lowercase(character))
}

/** Rust's `to_lowercase().next()`: the first character of the lowered form. */
function lowercase(character: string): string {
  return [...character.toLowerCase()][0] ?? character
}
