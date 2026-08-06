export declare function parse(input: any, options?: any): any;
export declare function detectTruncation(input: any, options?: any): {
    complete: boolean;
    kind: any;
    line: any;
    declared: any;
    actual: any;
    message: any;
};
/** Decodes TOON whose root form is an object. */
export declare function parseDocument(input: any, options: any): any;
