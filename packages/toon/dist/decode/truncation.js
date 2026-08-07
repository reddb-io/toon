import { ToonDecodeError, ToonError } from '../errors.js';
import { findUnquoted, splitDelimited } from '../lexical.js';
import { detectTruncation as detectLegacyTruncation } from '../toon_parts/parse.js';
import { decodeValue } from './build.js';
/** Reports incomplete v4.1 TOON and TOONL without weakening fail-fast decode. */
export function detectTruncation(input, options = {}) {
    const format = options.format ?? 'toon';
    if (format === 'toonl')
        return detectLegacyTruncation(input, options);
    if (format !== 'toon')
        throw new TypeError('detectTruncation format must be "toon" or "toonl"');
    try {
        decodeValue(input, options);
        return completeReport();
    }
    catch (error) {
        const mismatch = findArrayLengthMismatch(input, options);
        if (mismatch !== undefined)
            return mismatch;
        return {
            complete: false,
            kind: 'invalid',
            line: error instanceof ToonDecodeError || error instanceof ToonError ? error.line ?? 1 : 1,
            declared: null,
            actual: null,
            message: error instanceof ToonDecodeError || error instanceof ToonError
                ? `line ${error.line ?? 1}: ${error.reason}`
                : error instanceof Error ? error.message : String(error),
        };
    }
}
function findArrayLengthMismatch(input, options) {
    const lines = contentLines(input, options.indentSize ?? options.indent ?? 2);
    for (const [index, line] of lines.entries()) {
        const header = arrayHeader(line);
        if (header === undefined)
            continue;
        if (!header.fields && !header.keyed && header.inline !== '') {
            const actual = splitDelimited(header.inline, header.delimiter, line.number).length;
            if (actual < header.declared) {
                return mismatchReport(line.number, header.declared, actual, 'items');
            }
            continue;
        }
        const rowDepth = line.depth + 1;
        let actual = 0;
        for (const nested of lines.slice(index + 1)) {
            if (nested.depth <= line.depth)
                break;
            if (nested.depth === rowDepth)
                actual += 1;
        }
        if (actual < header.declared) {
            return mismatchReport(lines.at(-1)?.number ?? line.number, header.declared, actual, 'rows');
        }
    }
    return undefined;
}
function arrayHeader(line) {
    let open;
    try {
        open = findUnquoted(line.content, '[', line.number);
    }
    catch {
        return undefined;
    }
    if (open === -1)
        return undefined;
    const close = line.content.indexOf(']', open + 1);
    if (close === -1)
        return undefined;
    let segment = line.content.slice(open + 1, close);
    let keyed = false;
    if (segment.includes(':')) {
        keyed = true;
        segment = segment.replace(':', '');
    }
    let delimiter = ',';
    if (segment.endsWith('|') || segment.endsWith('\t')) {
        delimiter = segment.at(-1);
        segment = segment.slice(0, -1);
    }
    if (!/^(?:0|[1-9]\d*)$/.test(segment))
        return undefined;
    const suffix = line.content.slice(close + 1);
    const fields = suffix.startsWith('{');
    const colon = suffix.lastIndexOf(':');
    if (colon === -1)
        return undefined;
    return {
        declared: Number(segment),
        delimiter,
        fields,
        keyed,
        inline: suffix.slice(colon + 1).trim(),
    };
}
function contentLines(input, indentSize) {
    return input
        .split(/\n/)
        .map((raw, index) => ({ raw: raw.endsWith('\r') ? raw.slice(0, -1) : raw, number: index + 1 }))
        .filter(({ raw }) => raw.trim() !== '' && !/^ *#/.test(raw))
        .map(({ raw, number }) => {
        const spaces = raw.length - raw.replace(/^ +/, '').length;
        return { number, depth: Math.floor(spaces / indentSize), content: raw.slice(spaces) };
    });
}
function mismatchReport(line, declared, actual, unit) {
    return {
        complete: false,
        kind: 'array_length_mismatch',
        line,
        declared,
        actual,
        message: `declared ${declared} ${unit} but received ${actual}`,
    };
}
function completeReport() {
    return {
        complete: true,
        kind: 'complete',
        line: null,
        declared: null,
        actual: null,
        message: null,
    };
}
