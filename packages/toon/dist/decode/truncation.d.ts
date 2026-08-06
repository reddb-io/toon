import type { DecodeStreamOptions } from './stream.js';
export interface TruncationOptions extends DecodeStreamOptions {
    format?: 'toon' | 'toonl';
}
export interface TruncationReport {
    complete: boolean;
    kind: string;
    line: number | null;
    declared: number | null;
    actual: number | null;
    message: string | null;
}
/** Reports incomplete v4.1 TOON and TOONL without weakening fail-fast decode. */
export declare function detectTruncation(input: string, options?: TruncationOptions): TruncationReport;
