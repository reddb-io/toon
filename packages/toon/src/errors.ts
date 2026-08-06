/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */

export class ToonError extends Error {
  line: number
  reason: string
  constructor(line: number, message: string) {
    super(line === 0 ? message : `line ${line}: ${message}`)
    this.name = 'ToonError'
    this.line = line
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

export function toonError(line: number, message: string) {
  return new ToonError(line, message)
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
