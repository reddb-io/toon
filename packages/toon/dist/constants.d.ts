export declare const DELIMITERS: {
    readonly comma: ",";
    readonly tab: "\t";
    readonly pipe: "|";
};
export type DelimiterKey = keyof typeof DELIMITERS;
export type Delimiter = (typeof DELIMITERS)[DelimiterKey];
export declare const DEFAULT_DELIMITER: Delimiter;
/** Spaces per indentation level unless `options.indent` says otherwise. */
export declare const DEFAULT_INDENT = 2;
export declare const DEFAULT_MAX_DEPTH = 1000;
export declare const CYCLIC_DISCRIMINATOR_KEYS: string[];
export declare const CYCLIC_TABLE_DELIMITER = "|";
export declare const CYCLIC_META_KEYS: Set<string>;
