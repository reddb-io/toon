/** Converts host values to the JSON data model before replacement and encoding. */
export declare function normalizeValue(value: unknown): any;
export declare function isPlainObject(value: unknown): value is Record<string, any>;
export declare function isPrimitive(value: unknown): boolean;
export declare function setOwn(target: object, key: string, value: any): void;
