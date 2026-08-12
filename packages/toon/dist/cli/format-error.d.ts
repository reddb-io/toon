import type { ToonDecodeError, ToonError } from '../errors.js';
/**
 * Renders a decode failure as a header, the offending source line, and a caret
 * under the first character that could have caused it — the upstream
 * `@toon-format/cli` rendering, byte for byte.
 */
export declare function formatDecodeError(error: ToonDecodeError | ToonError): string;
