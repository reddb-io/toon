/**
 * Builds a JSON value tree from the decode event stream — the whole-document
 * convenience over `decodeFromLines` (ADR 0006). Mirrors upstream's
 * event-builder layering: the stream is the core, the tree is derived.
 */
import { decodeFromLines as decodeEventsFromLines } from './stream.js';
import { expandCyclicDiscriminatedArrays } from '../toon_parts/cyclic.js';
import { parse as parseLegacyExtensions } from '../toon_parts/parse.js';
import { applyReviver } from './reviver.js';
const UNSET = Symbol('unset');
export function buildValueFromEvents(events) {
    const stack = [];
    let pendingKey;
    let root = UNSET;
    const attach = (value) => {
        const parent = stack[stack.length - 1];
        if (parent === undefined) {
            root = value;
        }
        else if (Array.isArray(parent)) {
            parent.push(value);
        }
        else {
            // duplicate keys are last-write-wins (§14.3); defineProperty keeps
            // prototype keys like __proto__ ordinary own keys (§15)
            Object.defineProperty(parent, pendingKey, {
                value,
                enumerable: true,
                writable: true,
                configurable: true,
            });
        }
    };
    for (const event of events) {
        switch (event.type) {
            case 'startObject': {
                const value = {};
                attach(value);
                stack.push(value);
                break;
            }
            case 'startArray': {
                const value = [];
                attach(value);
                stack.push(value);
                break;
            }
            case 'endObject':
            case 'endArray':
                stack.pop();
                break;
            case 'key':
                pendingKey = event.key;
                break;
            case 'primitive':
                attach(event.value);
                break;
        }
    }
    return root === UNSET ? {} : root;
}
/** Decodes pre-split TOON lines into one JSON value. */
export function decodeFromLines(lines, options) {
    const value = buildValueFromEvents(decodeEventsFromLines(lines, options));
    return options?.reviver ? applyReviver(value, options.reviver) : value;
}
export function decodeValue(input, options) {
    const { reviver, ...streamOptions } = options ?? {};
    let value;
    if (hasPrimitiveArrayColumnHeader(input) ||
        (options?.objectArrayColumns !== false && hasFixedArrayColumnHeader(input))) {
        value = parseLegacyExtensions(input, {
            indent: options?.indentSize ?? options?.indent,
            strict: options?.strict,
            cyclicDiscriminatedArrays: false,
            maxDepth: options?.maxDepth,
        });
    }
    else {
        try {
            value = decodeFromLines(linesFromString(input), streamOptions);
        }
        catch (error) {
            if (options?.objectArrayColumns === false || !hasChildTableHeader(input))
                throw error;
            value = parseLegacyExtensions(input, {
                indent: options?.indentSize ?? options?.indent,
                strict: options?.strict,
                cyclicDiscriminatedArrays: false,
                maxDepth: options?.maxDepth,
            });
        }
    }
    const decoded = (options?.cyclicDiscriminatedArrays === true
        ? expandCyclicDiscriminatedArrays(value)
        : value);
    return reviver ? applyReviver(decoded, reviver) : decoded;
}
/** Iterates string lines without allocating a whole-document line array. */
function* linesFromString(input) {
    let start = 0;
    while (true) {
        const end = input.indexOf('\n', start);
        if (end === -1) {
            yield input.slice(start);
            return;
        }
        yield input.slice(start, end);
        start = end + 1;
    }
}
function hasPrimitiveArrayColumnHeader(input) {
    return headerFieldLists(input).some((fields) => [...fields.matchAll(/\[([^\]]*)\]/g)].some(([, content]) => !/^(?:0|[1-9]\d*)(?:\t|\|)?$/.test(content)));
}
function hasFixedArrayColumnHeader(input) {
    return headerFieldLists(input).some((fields) => /\[(?:0|[1-9]\d*)(?:\t|\|)?\]/.test(fields));
}
function headerFieldLists(input) {
    return input.split(/\r?\n/).flatMap((line) => {
        const outerClose = line.indexOf(']');
        if (outerClose === -1)
            return [];
        const fieldsStart = line.indexOf('{', outerClose + 1);
        if (fieldsStart === -1)
            return [];
        const fieldsEnd = line.lastIndexOf('}');
        return fieldsEnd > fieldsStart ? [line.slice(fieldsStart + 1, fieldsEnd)] : [];
    });
}
function hasChildTableHeader(input) {
    return input.split(/\r?\n/).some((line) => {
        const fieldsStart = line.indexOf('{', line.indexOf(']') + 1);
        const fieldsEnd = line.lastIndexOf('}');
        if (fieldsStart === -1 || fieldsEnd <= fieldsStart)
            return false;
        const fields = line.slice(fieldsStart + 1, fieldsEnd);
        return fields.includes('{') || /\[(?:0|[1-9]\d*)(?:\t|\|)?\]/.test(fields);
    });
}
