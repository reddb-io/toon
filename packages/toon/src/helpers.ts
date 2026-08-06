/**
 * Consumer-facing helpers layered on the core codec. These grew up in the
 * RedSkills wrapper package and moved upstream so every consumer of the
 * published package gets them (and RedSkills can be a pure npm consumer).
 */

import { serialize } from './toon.js'

/**
 * Encodes an object with a trailing spec-legal `summary:` field.
 *
 * The returned bytes are one conforming TOON document, so `parse(output)`
 * recovers the rollup together with the rest of the payload. Any existing
 * `summary` key is replaced and moved to the end.
 */
export function appendSummaryField(value, summary) {
  const entries = Object.entries(value).filter(([key]) => key !== 'summary')
  entries.push(['summary', summary])
  return serialize(Object.fromEntries(entries))
}

/**
 * Projects object rows onto an explicit minimal schema, preserving allowlist
 * order and dropping all non-allowlisted fields. Fields absent from a row
 * stay absent in the projection (they are not filled with null).
 */
export function projectFields(rows, fields) {
  return rows.map((row) => {
    const projected = {}
    for (const field of fields) {
      if (Object.prototype.hasOwnProperty.call(row, field)) {
        projected[field] = row[field]
      }
    }
    return projected
  })
}
