import { createReadStream, createWriteStream } from 'node:fs'
import { finished } from 'node:stream/promises'

import { decodeLines, encodeToonlLines } from './index.js'

export function readToonlFile(path) {
  return decodeLines(createReadStream(path))
}

export async function writeToonlFile(path, records, options) {
  const writer = createWriteStream(path)
  const emitter = encodeToonlLines(options)

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
    // Destroying with buffered writes emits 'error'; without a listener that
    // becomes an uncaught exception on top of the rejection callers observe.
    writer.once('error', () => {})
    writer.destroy()
    throw error
  }
}
