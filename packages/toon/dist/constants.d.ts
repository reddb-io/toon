export declare const DELIMITERS: {
    readonly comma: ",";
    readonly tab: "\t";
    readonly pipe: "|";
};
export type DelimiterKey = keyof typeof DELIMITERS;
export type Delimiter = (typeof DELIMITERS)[DelimiterKey];
export declare const DEFAULT_DELIMITER: Delimiter;
