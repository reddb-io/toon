export const DELIMITERS = {
  comma: ',',
  tab: '\t',
  pipe: '|',
} as const

export type DelimiterKey = keyof typeof DELIMITERS
export type Delimiter = (typeof DELIMITERS)[DelimiterKey]

export const DEFAULT_DELIMITER: Delimiter = DELIMITERS.comma
