const COMMENT_LINE_PATTERN = /(?:^\uFEFF?|\n) *#/

/** A primitive token that the encoder emits verbatim. */
export class RawString {
  readonly value: string

  constructor(value: string) {
    if (COMMENT_LINE_PATTERN.test(value)) {
      throw new TypeError(`Raw string must not contain a line starting with "#": ${JSON.stringify(value)}`)
    }
    this.value = value
  }
}

export function rawString(value: string): RawString {
  return new RawString(value)
}

export function isRawString(value: unknown): value is RawString {
  return value instanceof RawString
}
