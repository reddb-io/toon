import { isPlainObject, normalizeValue, setOwn } from './normalize.js';
/** Applies the JSON-style replacer before shape detection and emission. */
export function applyReplacer(root, replacer) {
    const replaced = replacer('', root, []);
    // The root cannot be omitted. Undefined there means "keep the original".
    return replaced === undefined
        ? transformChildren(root, replacer, [])
        : transformChildren(normalizeValue(replaced), replacer, []);
}
function transformChildren(value, replacer, path) {
    if (Array.isArray(value))
        return transformArray(value, replacer, path);
    if (isPlainObject(value))
        return transformObject(value, replacer, path);
    return value;
}
function transformObject(value, replacer, path) {
    const result = {};
    for (const [key, child] of Object.entries(value)) {
        const childPath = [...path, key];
        const replaced = replacer(key, child, childPath);
        if (replaced === undefined)
            continue;
        setOwn(result, key, transformChildren(normalizeValue(replaced), replacer, childPath));
    }
    return result;
}
function transformArray(value, replacer, path) {
    const result = [];
    for (let index = 0; index < value.length; index += 1) {
        const childPath = [...path, index];
        const replaced = replacer(String(index), value[index], childPath);
        if (replaced === undefined)
            continue;
        result.push(transformChildren(normalizeValue(replaced), replacer, childPath));
    }
    return result;
}
