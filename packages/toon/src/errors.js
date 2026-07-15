/**
 * Errors carry the 1-based source line so a decoder failure points at the row
 * that caused it. `line: 0` means "no line context" (encoder-side failures).
 */

export class ToonError extends Error {
  constructor(line, message) {
    super(line === 0 ? message : `line ${line}: ${message}`)
    this.name = 'ToonError'
    this.line = line
    this.reason = message
  }
}

export class ToonlError extends Error {
  constructor(line, message) {
    super(line === 0 ? message : `line ${line}: ${message}`)
    this.name = 'ToonlError'
    this.line = line
    this.reason = message
  }
}

export class ToonlCursorInvalidationError extends ToonlError {
  constructor(condition, message, details = {}) {
    super(0, message)
    this.name = 'ToonlCursorInvalidationError'
    this.condition = condition
    this.details = details
  }
}

export function toonError(line, message) {
  return new ToonError(line, message)
}

export function toonlError(line, message) {
  return new ToonlError(line, message)
}

/** Re-raises a decoder error as a TOONL error, keeping line and reason. */
export function asToonlError(error) {
  if (error instanceof ToonlError) {
    return error
  }
  if (error instanceof ToonError) {
    return new ToonlError(error.line, error.reason)
  }
  return new ToonlError(0, String(error && error.message ? error.message : error))
}
