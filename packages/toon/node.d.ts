import type { EncodeLinesOptions, ToonlRecord } from './index.d.ts'

export function readToonlFile(path: string | URL): AsyncGenerator<ToonlRecord, void, undefined>

export function writeToonlFile(
  path: string | URL,
  records: Iterable<ToonlRecord> | AsyncIterable<ToonlRecord>,
  options?: EncodeLinesOptions,
): Promise<void>
