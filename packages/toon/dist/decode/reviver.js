import { isPlainObject, normalizeValue, setOwn } from '../encode/normalize.js';
/** Apply an experimental decode reviver depth-first, from leaves to root. */
export function applyReviver(root, reviver) {
    const transformed = transformChildren(root, reviver, []);
    const revivedRoot = reviver('', transformed, []);
    return revivedRoot === undefined ? transformed : normalizeValue(revivedRoot);
}
function transformChildren(value, reviver, path) {
    if (Array.isArray(value))
        return transformArray(value, reviver, path);
    if (isPlainObject(value))
        return transformObject(value, reviver, path);
    return value;
}
function transformObject(object, reviver, path) {
    const result = {};
    for (const [key, value] of Object.entries(object)) {
        if (value === undefined)
            continue;
        const childPath = [...path, key];
        const revived = reviver(key, transformChildren(value, reviver, childPath), childPath);
        if (revived !== undefined)
            setOwn(result, key, normalizeValue(revived));
    }
    return result;
}
function transformArray(array, reviver, path) {
    const result = [];
    for (let index = 0; index < array.length; index += 1) {
        const childPath = [...path, index];
        const revived = reviver(String(index), transformChildren(array[index], reviver, childPath), childPath);
        if (revived !== undefined)
            result.push(normalizeValue(revived));
    }
    return result;
}
