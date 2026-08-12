/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */

export class ToonDecodeError extends SyntaxError {
  readonly line?: number
  readonly source?: string
  readonly reason: string

  constructor(message: string, context: { line?: number, source?: string, cause?: unknown } = {}) {
    const prefix = context.line === undefined || context.line === 0 ? '' : `Line ${context.line}: `
    super(prefix + message, context.cause === undefined ? undefined : { cause: context.cause })
    this.name = 'ToonDecodeError'
    this.line = context.line
    this.source = context.source
    this.reason = message
  }
}

/**
 * Positioned error raised inside the decoder. `decode` re-raises it as a
 * [`ToonDecodeError`] at the public boundary; the streaming and TOONL entry
 * points surface it directly.
 */
export class ToonError extends SyntaxError {
  readonly line: number
  readonly source?: string
  readonly reason: string

  constructor(line: number, message: string, context: { source?: string, cause?: unknown } = {}) {
    super(
      line === 0 ? message : `line ${line}: ${message}`,
      context.cause === undefined ? undefined : { cause: context.cause },
    )
    this.name = 'ToonError'
    this.line = line
    this.source = context.source
    this.reason = message
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
  context: { source?: string, cause?: unknown } = {},
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
