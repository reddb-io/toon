import { createReadStream, createWriteStream } from 'node:fs'
import { finished } from 'node:stream/promises'

import { decodeLines, encodeLines } from './index.js'

export function readToonlFile(path) {
  return decodeLines(createReadStream(path))
}

export async function writeToonlFile(path, records, options) {
  const writer = createWriteStream(path)
  const emitter = encodeLines(options)

  const write = async (chunk) => {
    if (chunk === '') {
      return
    }
    if (!writer.write(chunk)) {
      await new Promise((resolve, reject) => {
        writer.once('drain', resolve)
        writer.once('error', reject)
      })
    }
  }

  try {
    for await (const record of records) {
      await write(emitter.push(record))
    }
    await write(emitter.end())
    writer.end()
    await finished(writer)
  } catch (error) {
    writer.destroy()
    throw error
  }
}
