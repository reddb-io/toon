import { ToonDecodeError, ToonError } from '../errors.js';
import { decodeFromLines } from './stream.js';
/** Reports incomplete v4.1 TOON and TOONL without weakening fail-fast decode. */
export function detectTruncation(input, options = {}) {
    const format = options.format ?? 'toon';
    if (format === 'toonl')
        return detectToonlTruncation(input);
    if (format !== 'toon')
        throw new TypeError('detectTruncation format must be "toon" or "toonl"');
    try {
        for (const _event of decodeFromLines(linesFromString(input), options)) {
            // Exhausting the event decoder is the acceptance verdict.
        }
        return completeReport();
    }
    catch (error) {
        const mismatch = arrayMismatch(error);
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
function arrayMismatch(error) {
    if (!(error instanceof ToonDecodeError || error instanceof ToonError))
        return undefined;
    const match = /^expected (\d+) (inline-form values|list items|tabular rows|entry rows), but got (\d+)$/.exec(error.reason);
    if (match === null)
        return undefined;
    return mismatchReport(error.line ?? 1, Number(match[1]), Number(match[3]), match[2] === 'inline-form values' ? 'items' : 'rows');
}
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
function detectToonlTruncation(input) {
    let open;
    for (const [offset, raw] of input.split(/\n/).entries()) {
        const lineNumber = offset + 1;
        const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw;
        if (line === '')
            continue;
        if (line.startsWith('[=') && line.endsWith(']')) {
            const text = line.slice(2, -1);
            const declared = /^\d+$/.test(text) ? Number(text) : null;
            if (open === undefined) {
                return truncatedReport('invalid', lineNumber, declared, null, 'trailer without header');
            }
            if (declared !== open.actual) {
                return truncatedReport('toonl_trailer_count_mismatch', lineNumber, declared, open.actual, `trailer declared ${declared ?? 'invalid'} rows but received ${open.actual}`);
            }
            open = undefined;
        }
        else if (line.startsWith('[') && line.endsWith(':') && open === undefined) {
            open = { line: lineNumber, actual: 0 };
        }
        else if (open !== undefined) {
            open.actual++;
        }
    }
    return open === undefined
        ? completeReport()
        : truncatedReport('toonl_missing_trailer', open.line, null, open.actual, `stream ended without a trailer after ${open.actual} rows`);
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
function truncatedReport(kind, line, declared, actual, message) {
    return { complete: false, kind, line, declared, actual, message };
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
