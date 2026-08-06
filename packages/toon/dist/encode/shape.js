import { isPlainObject, isPrimitive } from './normalize.js';
/** Finds the recursive uniform-object shape required by v4.1 tabular form. */
export function tabularFields(rows) {
    const firstKeys = Object.keys(rows[0] ?? {});
    if (firstKeys.length === 0)
        return undefined;
    for (const row of rows) {
        if (!isPlainObject(row) || Object.keys(row).length !== firstKeys.length)
            return undefined;
        if (firstKeys.some((key) => !Object.prototype.hasOwnProperty.call(row, key)))
            return undefined;
    }
    const fields = [];
    for (const name of firstKeys) {
        const values = rows.map((row) => row[name]);
        if (values.every(isPrimitive)) {
            fields.push({ name });
            continue;
        }
        if (!values.every((value) => isPlainObject(value) && Object.keys(value).length > 0)) {
            return undefined;
        }
        const children = tabularFields(values);
        if (children === undefined)
            return undefined;
        fields.push({ name, children });
    }
    return fields;
}
/** Keyed form additionally requires at least two non-empty object entries. */
export function keyedFields(value) {
    const rows = Object.values(value);
    if (rows.length < 2)
        return undefined;
    if (!rows.every((row) => isPlainObject(row) && Object.keys(row).length > 0))
        return undefined;
    return tabularFields(rows);
}
export function collectLeaves(value, fields) {
    const leaves = [];
    for (const field of fields) {
        if (field.children === undefined)
            leaves.push(value[field.name]);
        else
            leaves.push(...collectLeaves(value[field.name], field.children));
    }
    return leaves;
}
