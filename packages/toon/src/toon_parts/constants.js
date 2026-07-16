/** Spaces per indentation level unless `options.indent` says otherwise. */
export const DEFAULT_INDENT = 2
export const DEFAULT_MAX_DEPTH = 1000
export const CYCLIC_DISCRIMINATOR_KEYS = ['type', 'kind', 'event']
export const CYCLIC_TABLE_DELIMITER = '|'
export const CYCLIC_META_KEYS = new Set(['order', 'discriminator', 'rows', 'common'])
