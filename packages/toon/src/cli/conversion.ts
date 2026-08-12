/**
 * The two conversions the `toon` CLI performs, over the canonical event-stream
 * codec: JSON to TOON and TOON back to JSON. Results go to stdout or to
 * `--output`; every diagnostic goes to stderr, so a pipeline stays clean.
 */

import * as path from 'node:path'

import { decodeStream } from '../decode/stream.js'
import { encode, encodeLines } from '../encode/serialize.js'
import { CliError } from './errors.js'
import {
  type CliIo,
  type InputSource,
  formatInputLabel,
  readInput,
  readLinesFromSource,
  writeStream,
} from './io.js'
import { jsonStreamFromEvents } from './json-from-events.js'
import { formatStatistics } from './tokens.js'

export interface ConversionConfig {
  input: InputSource
  output?: string
  indentSize: number
  io: CliIo
}

export async function encodeToToon(
  config: ConversionConfig & { delimiter: ',' | '|' | '\t', shouldPrintStats: boolean },
): Promise<void> {
  const { io } = config
  const jsonContent = await readInput(config.input, io)

  let data: unknown
  try {
    data = JSON.parse(jsonContent)
  } catch (error) {
    throw new CliError(
      `Failed to parse JSON: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    )
  }

  const encodeOptions = { delimiter: config.delimiter, indentSize: config.indentSize }

  if (!config.shouldPrintStats) {
    await writeStream(encodeLines(data, encodeOptions), {
      outputPath: config.output,
      separator: '\n',
      io,
    })
    reportWritten('Encoded', config)
    return
  }

  // Token counting needs the whole document, so the streaming form buys nothing here.
  const toonOutput = encode(data, encodeOptions)
  await writeStream([toonOutput], { outputPath: config.output, separator: '', io })

  reportWritten('Encoded', config)

  const statistics = formatStatistics(jsonContent, toonOutput)
  io.stderr(`● ${statistics.estimates}\n`)
  io.stderr(`✔ ${statistics.saved}\n`)
}

export async function decodeToJson(config: ConversionConfig & { strict: boolean }): Promise<void> {
  const { io } = config
  const lineSource = readLinesFromSource(config.input, config.strict, io)
  const events = decodeStream(lineSource, {
    indentSize: config.indentSize,
    strict: config.strict,
  })

  await writeStream(jsonStreamFromEvents(events, config.indentSize), {
    outputPath: config.output,
    separator: '',
    io,
  })

  reportWritten('Decoded', config)
}

function reportWritten(verb: string, config: ConversionConfig): void {
  if (!config.output) return

  const inputLabel = formatInputLabel(config.input, config.io)
  const outputLabel = path.relative(config.io.cwd, config.output)
  config.io.stderr(`✔ ${verb} \`${inputLabel}\` → \`${outputLabel}\`\n`)
}
