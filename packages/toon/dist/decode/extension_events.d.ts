/** Event emitters for the reddb array-column extensions. */
import type { ToonEvent } from '../events.js';
export interface ExtensionFieldNode {
    name: string;
    children?: ExtensionFieldNode[];
    listDelimiter?: string;
    fixedLength?: number;
}
export interface ExtensionLine {
    number: number;
    depth: number;
    content: string;
    blankBefore: boolean;
}
export interface ExtensionDecodeOptions {
    strict: boolean;
    objectArrayColumns: boolean;
}
/** Decode a buffered tabular span and expose only JSON-semantic events. */
export declare function emitExtensionRows(fields: ExtensionFieldNode[], lines: ExtensionLine[], length: number, delimiter: string, rowDepth: number, headerLine: number, options: ExtensionDecodeOptions): Generator<ToonEvent>;
