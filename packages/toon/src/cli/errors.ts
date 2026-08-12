/**
 * The `toon` CLI error boundary, mirroring the upstream `@toon-format/cli`
 * presentation: a condition the CLI recognized and phrased for a human prints
 * one clean line; anything else is a defect in the tool and prints its stack
 * unasked. `--verbose` adds the cause chain and stack to both.
 */

import { ToonDecodeError, ToonError } from '../errors.js'
import { formatDecodeError } from './format-error.js'

/** Raised for a condition the CLI recognized and phrased for a human. */
export class CliError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options)
    this.name = 'CliError'
  }
}

/** Renders a recognized error for a human — the boundary appends the stack itself. */
export function describeError(error: unknown): string {
  if (isPositionedDecodeError(error)) return formatDecodeError(error)
  return error instanceof Error ? error.message : String(error)
}

/**
 * Reports whether the CLI raised this error deliberately rather than tripping
 * over it. A Node system error carries a string `code` and reaches the boundary
 * as the honest answer to what the user asked for, so it reads as deliberate too.
 */
export function isExpectedError(error: unknown): boolean {
  if (error instanceof CliError) return true
  if (error instanceof ToonDecodeError || error instanceof ToonError) return true
  return error instanceof Error && typeof (error as { code?: unknown }).code === 'string'
}

/** Builds the stderr body for a failed run, without the `✖ ` prefix. */
export function formatReport(error: unknown, isVerbose: boolean): string {
  const sections = [describeError(error)]

  if (isVerbose || !isExpectedError(error)) {
    const causeChain = formatCauseChain(error)
    if (causeChain) sections.push(causeChain)
    if (error instanceof Error && error.stack) sections.push(error.stack)
  }

  return sections.join('\n\n')
}

function isPositionedDecodeError(error: unknown): error is ToonDecodeError | ToonError {
  return (
    (error instanceof ToonDecodeError || error instanceof ToonError)
    && error.line !== undefined
  )
}

function formatCauseChain(error: unknown): string {
  const causeLines: string[] = []
  let current: unknown = error instanceof Error ? error.cause : undefined

  while (current instanceof Error) {
    causeLines.push(`Caused by: ${current.name || 'Error'}: ${current.message}`)
    current = current.cause
  }

  return causeLines.join('\n')
}
