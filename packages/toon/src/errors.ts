/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */

/** A stable, coarse classification shared with the Rust decoder's `ErrorKind`. */
export type ToonErrorKind =
  | 'syntax'
  | 'indentation'
  | 'length-mismatch'
  | 'duplicate-key'
  | 'depth-limit'
  | 'input-limit'

export class ToonDecodeError extends SyntaxError {
  readonly line?: number
  /** 1-based column, when the decoder knows where on the line it failed. */
  readonly column?: number
  readonly source?: string
  readonly reason: string
  readonly kind: ToonErrorKind

  constructor(
    message: string,
    context: { line?: number, column?: number, source?: string, cause?: unknown } = {},
  ) {
    const prefix = context.line === undefined || context.line === 0 ? '' : `Line ${context.line}: `
    super(prefix + message, context.cause === undefined ? undefined : { cause: context.cause })
    this.name = 'ToonDecodeError'
    this.line = context.line
    this.column = context.column
    this.source = context.source
    this.reason = message
    this.kind = errorKind(message)
  }
}

/** Classifies a decoder reason; message wording may change, kinds do not. */
export function errorKind(reason: string): ToonErrorKind {
  if (/^(over-indented line|invalid indentation|tab used as indentation)$/.test(reason)) return 'indentation'
  if (/length mismatch|count mismatch|^expected \d+ .*, but got \d+$/.test(reason)) return 'length-mismatch'
  if (/^duplicate (object key|field name in header)$/.test(reason)) return 'duplicate-key'
  if (reason.startsWith('maximum nesting depth exceeded')) return 'depth-limit'
  if (/ exceeds max(InputBytes|ArrayLength|Keys) \(/.test(reason)) return 'input-limit'
  return 'syntax'
}

/**
 * Positioned error raised inside the decoder. `decode` re-raises it as a
 * [`ToonDecodeError`] at the public boundary; the streaming and TOONL entry
 * points surface it directly.
 */
export class ToonError extends SyntaxError {
  readonly line: number
  readonly column?: number
  readonly source?: string
  readonly reason: string
  readonly kind: ToonErrorKind

  constructor(
    line: number,
    message: string,
    context: { column?: number, source?: string, cause?: unknown } = {},
  ) {
    super(
      line === 0 ? message : `line ${line}: ${message}`,
      context.cause === undefined ? undefined : { cause: context.cause },
    )
    this.name = 'ToonError'
    this.line = line
    this.column = context.column
    this.source = context.source
    this.reason = message
    this.kind = errorKind(message)
  }
}

export class ToonlError extends Error {
  line: number
  reason: string
  constructor(line: number, message: string) {
    super(line === 0 ? message : `line ${line}: ${message}`)
    this.name = 'ToonlError'
    this.line = line
    this.reason = message
  }
}

export class ToonlCursorInvalidationError extends ToonlError {
  condition: string
  details: Record<string, unknown>
  constructor(condition: string, message: string, details: Record<string, unknown> = {}) {
    super(0, message)
    this.name = 'ToonlCursorInvalidationError'
    this.condition = condition
    this.details = details
  }
}

export function toonError(
  line: number,
  message: string,
  context: { column?: number, source?: string, cause?: unknown } = {},
) {
  return new ToonError(line, message, context)
}

export function toonlError(line: number, message: string) {
  return new ToonlError(line, message)
}

/** Re-raises a decoder error as a TOONL error, keeping line and reason. */
export function asToonlError(error: any) {
  if (error instanceof ToonlError) {
    return error
  }
  if (error instanceof ToonError) {
    return new ToonlError(error.line, error.reason)
  }
  return new ToonlError(0, String(error && error.message ? error.message : error))
}
