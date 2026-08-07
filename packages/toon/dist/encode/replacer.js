import { isPlainObject, normalizeValue, setOwn } from './normalize.js';
import { isRawString } from './raw-string.js';
/** Applies the JSON-style replacer before shape detection and emission. */
export function applyReplacer(root, replacer) {
    const replaced = replacer('', root, []);
    // The root cannot be omitted. Undefined there means "keep the original".
    return replaced === undefined
        ? transformChildren(root, replacer, [])
        : transformReplaced(root, replaced, replacer, []);
}
function transformReplaced(original, replaced, replacer, path) {
    if (isRawString(replaced) && (Array.isArray(original) || isPlainObject(original))) {
        return transformChildren(original, replacer, path);
    }
    return transformChildren(normalizeValue(replaced), replacer, path);
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
        setOwn(result, key, transformReplaced(child, replaced, replacer, childPath));
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
        result.push(transformReplaced(value[index], replaced, replacer, childPath));
    }
    return result;
}
