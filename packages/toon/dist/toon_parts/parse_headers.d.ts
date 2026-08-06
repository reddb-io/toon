export declare function lengthMismatch(lines: any, index: any): import("../errors.js").ToonError;
/**
 * Parses `key[N<delim?>]{fields}:` (spec §6). `colon` is the first unquoted colon
 * on the line; the header must terminate exactly there. Throws with `line: 0`, so
 * callers stamp their own line number via {@link atLine}.
 */
export declare function parseHeader(content: any, colon: any): {
    key: any;
    keyQuoted: any;
    len: number;
    delimiter: any;
    fields: any;
    fieldTree: any;
};
export declare function parseArrayHeaderFields(source: any, delimiter: any): any[];
export declare function parseMapHeader(content: any): {
    key: any;
    keyQuoted: any;
    delimiter: string;
    fields: any;
};
export declare function parseHeaderFields(source: any, delimiter: any, activeDelimiter: any): any[];
export declare function parseArrayHeaderFieldTree(source: any, delimiter: any): any[];
export declare function parseHeaderFieldTree(source: any, delimiter: any, activeDelimiter: any): any[];
export declare function flattenHeaderFieldTree(fields: any, prefix?: any[]): any;
export declare function parseTabularCell(field: any, cell: any, line: any): any;
export declare function isValidListDelimiter(value: any, activeDelimiter: any): boolean;
export declare function samePath(left: any, right: any): any;
export declare function pathStartsWith(path: any, prefix: any): any;
export declare function atLine(error: any, line: any): import("../errors.js").ToonError;
