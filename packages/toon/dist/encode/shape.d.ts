export interface FieldNode {
    name: string;
    children?: FieldNode[];
    childTable?: boolean;
    fixedLength?: number;
    listDelimiter?: ';';
    self?: boolean;
}
export interface ShapeOptions {
    objectArrayColumns?: boolean;
    primitiveArrayColumns?: boolean;
}
/** Finds the recursive uniform shape required by v4.1 and extension tables. */
export declare function tabularFields(rows: any[], options?: ShapeOptions): FieldNode[] | undefined;
/** Keyed form additionally requires at least two non-empty object entries. */
export declare function keyedFields(value: Record<string, any>, options?: ShapeOptions): FieldNode[] | undefined;
export declare function collectLeaves(value: Record<string, any>, fields: FieldNode[]): any[];
