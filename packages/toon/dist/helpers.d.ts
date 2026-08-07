/**
 * Consumer-facing helpers layered on the core codec. These grew up in the
 * RedSkills wrapper package and moved upstream so every consumer of the
 * published package gets them (and RedSkills can be a pure npm consumer).
 */
/**
 * Encodes an object with a trailing spec-legal `summary:` field.
 *
 * The returned bytes are one conforming TOON document, so `decode(output)`
 * recovers the rollup together with the rest of the payload. Any existing
 * `summary` key is replaced and moved to the end.
 */
export declare function appendSummaryField(value: any, summary: any): string;
/**
 * Projects object rows onto an explicit minimal schema, preserving allowlist
 * order and dropping all non-allowlisted fields. Fields absent from a row
 * stay absent in the projection (they are not filled with null).
 */
export declare function projectFields(rows: any, fields: any): any;
