export declare function serialize(value: any, options?: any): string;
/**
 * Tabular eligibility (§9.3): every element is a non-empty object, all share the
 * first element's key set, and every value is primitive.
 */
export declare function tabularFields(values: any): any[];
