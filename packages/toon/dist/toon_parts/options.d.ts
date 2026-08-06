export declare function resolveOptions(options?: any): {
    indent: number;
    strict: any;
    expandPaths: boolean;
    cyclicDiscriminatedArrays: boolean;
    maxDepth: number;
};
export declare function collectLines(input: any, options: any): any[];
export declare function checkDepth(depth: any, line: any, options: any): void;
export declare function checkHeaderDepth(header: any, line: any, options: any): void;
/** Decodes TOON per spec §5 root-form discovery. */
