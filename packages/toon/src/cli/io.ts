/**
 * Every byte the `toon` CLI reads or writes goes through this seam, so a test
 * can drive a whole run in-process while the real entry point binds it to
 * `process`.
 */

import { createReadStream } from 'node:fs'
import * as fsp from 'node:fs/promises'
import * as path from 'node:path'
import { TextDecoder } from 'node:util'

import { CliError } from './errors.js'

export interface CliIo {
  /** Resolves relative input, output, and label paths. */
  cwd: string
  stdout(text: string): void
  stderr(text: string): void
  stdin(): AsyncIterable<Uint8Array | string>
}

export type InputSource = { type: 'stdin' } | { type: 'file', path: string }

/** Batched write size: large enough to amortize syscalls, small enough to stream. */
const WRITE_BATCH_BYTES = 64 * 1024

/** Reads a whole input as text, replacing ill-formed bytes like Node's stdin does. */
export async function readInput(source: InputSource, io: CliIo): Promise<string> {
  if (source.type === 'file') return fsp.readFile(source.path, 'utf-8')

  const decoder = new TextDecoder('utf-8')
  let text = ''
  for await (const chunk of io.stdin()) {
    text += typeof chunk === 'string' ? chunk : decoder.decode(chunk, { stream: true })
  }
  return text + decoder.decode()
}

/** Streams an input as lines. Strict decoding refuses to substitute U+FFFD. */
export async function* readLinesFromSource(
  source: InputSource,
  strict: boolean,
  io: CliIo,
): AsyncIterable<string> {
  const stream = source.type === 'stdin' ? io.stdin() : createReadStream(source.path)
  // Node's own string decoding substitutes U+FFFD, which a strict decoder MUST NOT do.
  const decoder = new TextDecoder('utf-8', { fatal: strict })
  let buffer = ''

  for await (const chunk of stream) {
    buffer += typeof chunk === 'string' ? chunk : decodeUtf8(decoder, chunk)
    let index: number

    while ((index = buffer.indexOf('\n')) !== -1) {
      yield buffer.slice(0, index)
      buffer = buffer.slice(index + 1)
    }
  }

  buffer += decodeUtf8(decoder)

  if (buffer.length > 0) {
    yield buffer
  }
}

/** Writes the pieces to a file or to stdout, always ending with a newline. */
export async function writeStream(
  pieces: AsyncIterable<string> | Iterable<string>,
  options: { outputPath?: string, separator: string, io: CliIo },
): Promise<void> {
  const { outputPath, separator, io } = options
  const handle = outputPath ? await fsp.open(outputPath, 'w') : undefined

  try {
    // The event stream arrives in token-sized pieces; batching them keeps a
    // large document from costing one write syscall per piece.
    let batch = ''
    const flush = async () => {
      if (batch === '') return
      if (handle) await handle.write(batch)
      else io.stdout(batch)
      batch = ''
    }
    const write = async (text: string) => {
      batch += text
      if (batch.length >= WRITE_BATCH_BYTES) await flush()
    }

    let isFirst = true
    for await (const piece of pieces) {
      if (!isFirst && separator) await write(separator)
      await write(piece)
      isFirst = false
    }

    await write('\n')
    await flush()
  } finally {
    await handle?.close()
  }
}

/** Names an input the way the upstream success lines do. */
export function formatInputLabel(source: InputSource, io: CliIo): string {
  if (source.type === 'stdin') return 'stdin'
  return path.relative(io.cwd, source.path) || path.basename(source.path)
}

function decodeUtf8(decoder: TextDecoder, chunk?: Uint8Array): string {
  try {
    return chunk === undefined ? decoder.decode() : decoder.decode(chunk, { stream: true })
  } catch {
    throw new CliError('Input is not valid UTF-8. Pass --no-strict to replace ill-formed bytes')
  }
}
