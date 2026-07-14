/**
 * @reddb-io/toon — TOON v3.3 parser/serializer and TOONL v0.1 streaming, in
 * dependency-free ESM. The TOON side decodes to (and encodes from) plain JSON
 * values; the TOONL side is built for append-only streams.
 */

export { ToonError, ToonlError } from './errors.js'
export { DEFAULT_INDENT, parse, parseDocument, serialize } from './toon.js'
export {
  JsonlToToonl,
  ToonlDecodeStream,
  ToonlEncodeStream,
  ToonlToJsonl,
  ToonlEncoder,
  closeTransform,
  decodeLines,
  encodeLines,
  encodeRecords,
  jsonToToon,
  parseRecords,
  parseStream,
  recordTransform,
  toonToJson,
} from './toonl.js'
