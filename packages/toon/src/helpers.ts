/**
 * Consumer-facing helpers layered on the core codec. These grew up in the
 * RedSkills wrapper package and moved upstream so every consumer of the
 * published package gets them (and RedSkills can be a pure npm consumer).
 */

import { encode } from './encode/serialize.js'

/**
 * Encodes an object with a trailing spec-legal `summary:` field.
 *
 * The returned bytes are one conforming TOON document, so `decode(output)`
 * recovers the rollup together with the rest of the payload. Any existing
 * `summary` key is replaced and moved to the end.
 */
export function appendSummaryField(value, summary) {
  const entries = Object.entries(value).filter(([key]) => key !== 'summary')
  entries.push(['summary', summary])
  return encode(Object.fromEntries(entries))
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

/**
 * Renders an MCP `tools/list` result as a compact TOON manifest for a prompt.
 *
 * Each tool keeps its name and description, and its input schema flattens to
 * one tabular `params` row per property (`name,type,required,description`),
 * which is where most of the JSON-Schema punctuation goes. The manifest is a
 * prompt-facing summary, not a schema round-trip: nested object schemas are
 * reduced to their type, and the host still validates calls against the full
 * `inputSchema`.
 */
export function encodeToolManifest(tools, options = {}) {
  return encode({ tools: tools.map(toolEntry) }, options)
}

function toolEntry(tool) {
  const schema = tool.inputSchema ?? {}
  const required = new Set(schema.required ?? [])
  const params = Object.entries(schema.properties ?? {}).map(([name, property]: [string, any]) => ({
    name,
    type: schemaType(property),
    required: required.has(name),
    description: property?.description ?? '',
  }))
  return { name: tool.name, description: tool.description ?? '', params }
}

/** `string`, `array<integer>`, `string|null`, or `enum(a|b)`; `any` when unstated. */
function schemaType(property) {
  if (!property || typeof property !== 'object') return 'any'
  if (Array.isArray(property.enum)) return `enum(${property.enum.map(String).join('|')})`
  const type = Array.isArray(property.type) ? property.type.join('|') : property.type
  if (type === 'array') return `array<${schemaType(property.items)}>`
  return type ?? 'any'
}
