import { canonicalKey, primitiveText } from '../lexical.js';
import { toonError } from '../errors.js';
import { DEFAULT_MAX_DEPTH } from '../toon_parts/constants.js';
import { isPlainObject, isPrimitive, normalizeValue } from './normalize.js';
import { applyReplacer } from './replacer.js';
import { collectLeaves, keyedFields, tabularFields } from './shape.js';
import { cyclicDiscriminatedArrayWire } from '../toon_parts/cyclic.js';
import { serialize as serializeLegacyExtensions } from '../toon_parts/serialize.js';
/** Encodes normalized JSON using the canonical v4.1 forms. */
export function encode(input, options = {}) {
    const delimiter = options.delimiter ?? ',';
    if (![',', '|', '\t'].includes(delimiter))
        throw new TypeError('invalid delimiter');
    const indentSize = Math.max(1, Math.floor(options.indentSize ?? options.indent ?? 2));
    const rawMaxDepth = options.maxDepth ?? DEFAULT_MAX_DEPTH;
    const maxDepth = rawMaxDepth === Number.POSITIVE_INFINITY
        ? 0
        : Math.max(0, Math.floor(rawMaxDepth));
    const normalized = normalizeValue(input);
    const value = options.replacer
        ? applyReplacer(normalized, options.replacer)
        : normalized;
    const resolved = { delimiter, indentSize, maxDepth };
    if (options.cyclicDiscriminatedArrays === true) {
        const cyclic = cyclicDiscriminatedArrayWire(value);
        if (cyclic !== undefined)
            return cyclic.trimEnd();
    }
    if (options.primitiveArrayColumns === true || options.objectArrayColumns === true) {
        const extension = serializeLegacyExtensions(value, {
            delimiter,
            primitiveArrayColumns: options.primitiveArrayColumns === true,
            objectArrayColumns: options.objectArrayColumns === true,
            maxDepth,
        });
        const withoutExtension = serializeLegacyExtensions(value, { delimiter, maxDepth });
        if (extension !== withoutExtension)
            return extension.trimEnd();
    }
    return encodeValue(value, resolved).join('\n');
}
function encodeValue(value, options) {
    if (isPrimitive(value))
        return [primitiveText(value, options.delimiter)];
    if (Array.isArray(value))
        return encodeArray(undefined, value, 0, options);
    const fields = keyedFields(value);
    return fields === undefined
        ? encodeObject(value, 0, options)
        : encodeKeyed(undefined, value, fields, 0, options);
}
function encodeObject(value, depth, options) {
    checkDepth(depth, options);
    return Object.entries(value).flatMap(([key, nested]) => encodeField(key, nested, depth, options));
}
function encodeField(key, value, depth, options) {
    const prefix = indentation(depth, options) + canonicalKey(key);
    if (isPrimitive(value))
        return [`${prefix}: ${primitiveText(value, options.delimiter)}`];
    if (Array.isArray(value))
        return encodeArray(key, value, depth, options);
    const fields = keyedFields(value);
    if (fields !== undefined)
        return encodeKeyed(key, value, fields, depth, options);
    const lines = [`${prefix}:`];
    if (Object.keys(value).length > 0)
        lines.push(...encodeObject(value, depth + 1, options));
    return lines;
}
function encodeKeyed(key, value, fields, depth, options) {
    checkDepth(depth, options);
    const entries = Object.entries(value);
    checkFieldDepth(fields, depth + 1, options);
    const lines = [
        indentation(depth, options) + header(key, entries.length, fields, options.delimiter, true),
    ];
    for (const [entryKey, entryValue] of entries) {
        lines.push(indentation(depth + 1, options) +
            canonicalKey(entryKey) +
            ': ' +
            encodeCells(collectLeaves(entryValue, fields), options.delimiter));
    }
    return lines;
}
function encodeArray(key, value, depth, options) {
    checkDepth(depth, options);
    const prefix = indentation(depth, options);
    if (value.length === 0)
        return [key === undefined ? `${prefix}[]` : `${prefix}${canonicalKey(key)}: []`];
    if (value.every(isPrimitive)) {
        return [
            prefix + header(key, value.length, undefined, options.delimiter) + ' ' + encodeCells(value, options.delimiter),
        ];
    }
    if (value.every(isPlainObject)) {
        const fields = tabularFields(value);
        if (fields !== undefined)
            return encodeTabular(key, value, fields, depth, options);
    }
    const lines = [prefix + header(key, value.length, undefined, options.delimiter)];
    for (const item of value)
        lines.push(...encodeListItem(item, depth + 1, options));
    return lines;
}
function encodeTabular(key, rows, fields, depth, options) {
    checkFieldDepth(fields, depth + 1, options);
    const lines = [indentation(depth, options) + header(key, rows.length, fields, options.delimiter)];
    for (const row of rows) {
        lines.push(indentation(depth + 1, options) + encodeCells(collectLeaves(row, fields), options.delimiter));
    }
    return lines;
}
function encodeListItem(value, depth, options) {
    checkDepth(depth, options);
    const prefix = indentation(depth, options) + '-';
    if (isPrimitive(value))
        return [`${prefix} ${primitiveText(value, options.delimiter)}`];
    if (Array.isArray(value)) {
        if (value.length === 0)
            return [`${prefix} ${header(undefined, 0, undefined, options.delimiter)}`];
        if (value.every(isPrimitive)) {
            return [`${prefix} ${header(undefined, value.length, undefined, options.delimiter)} ${encodeCells(value, options.delimiter)}`];
        }
        const lines = [`${prefix} ${header(undefined, value.length, undefined, options.delimiter)}`];
        for (const item of value)
            lines.push(...encodeListItem(item, depth + 1, options));
        return lines;
    }
    return encodeObjectListItem(value, depth, options);
}
function encodeObjectListItem(value, depth, options) {
    checkDepth(depth, options);
    const entries = Object.entries(value);
    if (entries.length === 0)
        return [indentation(depth, options) + '-'];
    const [[firstKey, firstValue], ...rest] = entries;
    const special = encodeFirstContainer(firstKey, firstValue, depth, options);
    let lines;
    if (special !== undefined) {
        lines = special;
    }
    else if (isPrimitive(firstValue)) {
        lines = [
            indentation(depth, options) +
                '- ' +
                canonicalKey(firstKey) +
                ': ' +
                primitiveText(firstValue, options.delimiter),
        ];
    }
    else if (Array.isArray(firstValue)) {
        if (firstValue.length === 0) {
            lines = [indentation(depth, options) + '- ' + canonicalKey(firstKey) + ': []'];
        }
        else {
            lines = [
                indentation(depth, options) +
                    '- ' +
                    header(firstKey, firstValue.length, undefined, options.delimiter),
            ];
            for (const item of firstValue)
                lines.push(...encodeListItem(item, depth + 2, options));
        }
    }
    else {
        lines = [indentation(depth, options) + '- ' + canonicalKey(firstKey) + ':'];
        if (Object.keys(firstValue).length > 0)
            lines.push(...encodeObject(firstValue, depth + 2, options));
    }
    if (rest.length > 0)
        lines.push(...encodeObject(Object.fromEntries(rest), depth + 1, options));
    return lines;
}
function encodeFirstContainer(key, value, depth, options) {
    if (Array.isArray(value) && value.length > 0 && value.every(isPrimitive)) {
        return [
            indentation(depth, options) +
                '- ' +
                header(key, value.length, undefined, options.delimiter) +
                ' ' +
                encodeCells(value, options.delimiter),
        ];
    }
    if (Array.isArray(value) && value.every(isPlainObject)) {
        const fields = tabularFields(value);
        if (fields !== undefined) {
            checkFieldDepth(fields, depth + 1, options);
            const lines = [
                indentation(depth, options) + '- ' + header(key, value.length, fields, options.delimiter),
            ];
            for (const row of value) {
                lines.push(indentation(depth + 2, options) + encodeCells(collectLeaves(row, fields), options.delimiter));
            }
            return lines;
        }
    }
    if (isPlainObject(value)) {
        const fields = keyedFields(value);
        if (fields !== undefined) {
            checkFieldDepth(fields, depth + 1, options);
            const entries = Object.entries(value);
            const lines = [
                indentation(depth, options) + '- ' + header(key, entries.length, fields, options.delimiter, true),
            ];
            for (const [entryKey, entryValue] of entries) {
                lines.push(indentation(depth + 2, options) +
                    canonicalKey(entryKey) +
                    ': ' +
                    encodeCells(collectLeaves(entryValue, fields), options.delimiter));
            }
            return lines;
        }
    }
    return undefined;
}
function header(key, length, fields, delimiter, keyed = false) {
    const encodedKey = key === undefined ? '' : canonicalKey(key);
    const marker = keyed ? ':' : '';
    const delimiterMarker = delimiter === ',' ? '' : delimiter;
    const fieldText = fields === undefined ? '' : `{${formatFields(fields, delimiter)}}`;
    return `${encodedKey}[${length}${marker}${delimiterMarker}]${fieldText}:`;
}
function formatFields(fields, delimiter) {
    return fields
        .map((field) => canonicalKey(field.name) +
        (field.children === undefined ? '' : `{${formatFields(field.children, delimiter)}}`))
        .join(delimiter);
}
function encodeCells(values, delimiter) {
    return values.map((value) => primitiveText(value, delimiter)).join(delimiter);
}
function indentation(depth, options) {
    return ' '.repeat(depth * options.indentSize);
}
function checkDepth(depth, options) {
    if (options.maxDepth !== 0 && depth > options.maxDepth) {
        throw toonError(0, `maximum nesting depth exceeded (maxDepth ${options.maxDepth})`);
    }
}
function checkFieldDepth(fields, depth, options) {
    checkDepth(depth, options);
    for (const field of fields) {
        if (field.children !== undefined)
            checkFieldDepth(field.children, depth + 1, options);
    }
}
