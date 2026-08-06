export interface FieldNode {
    name: string;
    children?: FieldNode[];
}
/** Finds the recursive uniform-object shape required by v4.1 tabular form. */
export declare function tabularFields(rows: Record<string, any>[]): FieldNode[] | undefined;
/** Keyed form additionally requires at least two non-empty object entries. */
export declare function keyedFields(value: Record<string, any>): FieldNode[] | undefined;
export declare function collectLeaves(value: Record<string, any>, fields: FieldNode[]): any[];
