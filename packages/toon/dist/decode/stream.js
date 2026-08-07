/**
 * Event-based streaming decoder (ADR 0006), targeting TOON spec v4.1.
 *
 * Consumes TOON lines (without newlines) and yields the six JSON-semantic
 * events, each carrying its 1-based source line. The parser requests lines on
 * demand and retains at most two classified content lines: the current line
 * plus one lookahead needed to distinguish a root scalar from a document.
 * Errors are fail-fast positioned `ToonError`s; strict-mode policy is resolved
 * here at the public boundary.
 *
 * Layering (§5–§12): line classification (comments, blanks, indentation) →
 * header grammar (§6) → scope emitters for objects (§8), arrays (§9.1–§9.4),
 * keyed tabular objects (§9.5) and objects as list items (§10).
 */
import { ToonDecodeError, ToonError, toonError } from '../errors.js';
import { findUnquoted, parseKey, parseScalar } from '../lexical.js';
import { DEFAULT_MAX_DEPTH } from '../toon_parts/constants.js';
/** A full-line comment: only U+0020 spaces before `#` (§5.1). */
function isCommentLine(raw) {
    let i = 0;
    while (i < raw.length && raw[i] === ' ')
        i++;
    return raw[i] === '#';
}
class LineClassifier {
    ctx;
    sourceByLine = new Map();
    number = 0;
    blankPending = false;
    firstLine = true;
    constructor(ctx) {
        this.ctx = ctx;
    }
    push(input) {
        let raw = input;
        this.number++;
        this.sourceByLine.set(this.number, input);
        if (this.firstLine) {
            // A single leading U+FEFF is a byte-order mark, not content (§12).
            if (raw.startsWith('﻿'))
                raw = raw.slice(1);
            this.firstLine = false;
        }
        if (raw.endsWith('\r'))
            raw = raw.slice(0, -1);
        // Trailing spaces are not part of the line's content (§12).
        raw = raw.replace(/ +$/, '');
        if (raw.trim() === '') {
            this.blankPending = true;
            return undefined;
        }
        if (isCommentLine(raw))
            return undefined;
        let i = 0;
        let tabs = 0;
        let spaces = 0;
        while (i < raw.length && (raw[i] === ' ' || raw[i] === '\t')) {
            if (raw[i] === '\t') {
                if (this.ctx.strict)
                    throw toonError(this.number, 'tab used as indentation');
                tabs++;
            }
            else {
                spaces++;
            }
            i++;
        }
        let depth;
        if (spaces % this.ctx.indentSize === 0) {
            depth = spaces / this.ctx.indentSize;
        }
        else if (this.ctx.strict) {
            throw toonError(this.number, 'invalid indentation');
        }
        else {
            depth = Math.floor(spaces / this.ctx.indentSize);
        }
        // Non-strict tab leniency (§12): each leading tab counts as one level.
        depth += tabs;
        checkDepth(depth, this.number, this.ctx);
        const line = {
            number: this.number,
            depth,
            content: raw.slice(i),
            blankBefore: this.blankPending,
        };
        this.blankPending = false;
        return line;
    }
}
function* classifyLines(source, classifier) {
    for (const raw of source) {
        const line = classifier.push(raw);
        if (line !== undefined)
            yield line;
    }
}
async function* classifyLinesAsync(source, classifier) {
    for await (const raw of source) {
        const line = classifier.push(raw);
        if (line !== undefined)
            yield line;
    }
}
const LENGTH_RE = /^(0|[1-9]\d*)$/;
/**
 * Parses one line's content as a header. Returns the header, `null` when the
 * content is not header-shaped at all, and throws on a malformed header —
 * callers decide the non-strict fall-through to key-value parsing (§6, §14.2).
 */
function parseHeader(content, line) {
    const bracket = findUnquoted(content, '[', line);
    if (bracket === -1)
        return null;
    const colon = findUnquoted(content, ':', line);
    if (colon !== -1 && colon < bracket)
        return null;
    const keyText = content.slice(0, bracket);
    if (keyText !== '' && /\s$/.test(keyText)) {
        throw toonError(line, 'whitespace between key and bracket segment');
    }
    const close = content.indexOf(']', bracket);
    if (close === -1)
        return null;
    let segment = content.slice(bracket + 1, close);
    let keyed = false;
    let delimiter = ',';
    // A colon immediately after the length marks a keyed header (§9.5).
    const segmentColon = segment.indexOf(':');
    if (segmentColon !== -1) {
        const after = segment.slice(segmentColon + 1);
        segment = segment.slice(0, segmentColon);
        keyed = true;
        if (after === '\t' || after === '|')
            delimiter = after;
        else if (after !== '')
            throw toonError(line, 'malformed bracket segment');
    }
    else if (segment.endsWith('\t') || segment.endsWith('|')) {
        delimiter = segment.slice(-1);
        segment = segment.slice(0, -1);
    }
    if (!LENGTH_RE.test(segment)) {
        throw toonError(line, 'malformed array header length');
    }
    const length = Number(segment);
    let rest = content.slice(close + 1);
    let fields;
    if (rest.startsWith('{')) {
        const endBrace = matchBrace(rest, line);
        fields = parseFieldList(rest.slice(1, endBrace), delimiter, line);
        rest = rest.slice(endBrace + 1);
    }
    if (keyed && fields === undefined) {
        throw toonError(line, 'keyed header requires a field list');
    }
    if (!rest.startsWith(':'))
        throw toonError(line, 'expected colon after array header');
    const inlineContent = trimSpaces(rest.slice(1));
    if ((fields !== undefined || keyed) && inlineContent !== '') {
        throw toonError(line, 'unexpected content after fields-bearing header colon');
    }
    const key = keyText === '' ? undefined : parseKey(keyText, line)[0];
    return {
        key,
        length,
        delimiter,
        fields,
        keyed,
        inline: inlineContent === '' ? undefined : inlineContent,
    };
}
/** Index of the matching `}` for a field list starting at `{`, ignoring quoted names. */
function matchBrace(text, line) {
    let depth = 0;
    let inQuotes = false;
    for (let i = 0; i < text.length; i++) {
        const ch = text[i];
        if (inQuotes) {
            if (ch === '\\')
                i++;
            else if (ch === '"')
                inQuotes = false;
            continue;
        }
        if (ch === '"')
            inQuotes = true;
        else if (ch === '{')
            depth++;
        else if (ch === '}') {
            depth--;
            if (depth === 0)
                return i;
        }
    }
    throw toonError(line, 'malformed tabular header fields');
}
function parseFieldList(text, delimiter, line) {
    const entries = [];
    let start = 0;
    let depth = 0;
    let inQuotes = false;
    const push = (chunk) => {
        entries.push(parseFieldEntry(chunk, delimiter, line));
    };
    for (let i = 0; i < text.length; i++) {
        const ch = text[i];
        if (inQuotes) {
            if (ch === '\\')
                i++;
            else if (ch === '"')
                inQuotes = false;
            continue;
        }
        if (ch === '"')
            inQuotes = true;
        else if (ch === '{')
            depth++;
        else if (ch === '}')
            depth--;
        else if (ch === delimiter && depth === 0) {
            push(text.slice(start, i));
            start = i + 1;
        }
    }
    push(text.slice(start));
    return entries;
}
function parseFieldEntry(chunk, delimiter, line) {
    const trimmed = trimSpaces(chunk);
    if (trimmed === '')
        throw toonError(line, 'empty field entry in header');
    const brace = trimmed.startsWith('"')
        ? trimmed.indexOf('{', closingQuoteIndex(trimmed, line) + 1)
        : trimmed.indexOf('{');
    if (brace === -1) {
        return { name: parseKey(trimmed, line)[0] };
    }
    const end = matchBrace(trimmed.slice(brace), line) + brace;
    if (end !== trimmed.length - 1)
        throw toonError(line, 'malformed tabular header fields');
    const children = parseFieldList(trimmed.slice(brace + 1, end), delimiter, line);
    if (children.length === 0)
        throw toonError(line, 'empty field entry in header');
    return { name: parseKey(trimmed.slice(0, brace), line)[0], children };
}
function closingQuoteIndex(text, line) {
    for (let i = 1; i < text.length; i++) {
        if (text[i] === '\\')
            i++;
        else if (text[i] === '"')
            return i;
    }
    throw toonError(line, 'invalid quoted string');
}
function countLeaves(fields) {
    let count = 0;
    for (const field of fields) {
        count += field.children === undefined ? 1 : countLeaves(field.children);
    }
    return count;
}
/** Duplicate field names within one field list are a header defect (§9.3, §14.2). */
function assertNoDuplicateFields(fields, line, ctx) {
    const seen = new Set();
    for (const field of fields) {
        if (seen.has(field.name) && ctx.strict)
            throw toonError(line, 'duplicate field name in header');
        seen.add(field.name);
        if (field.children !== undefined)
            assertNoDuplicateFields(field.children, line, ctx);
    }
}
// #endregion
/** Token trimming is exactly U+0020 (§12). */
function trimSpaces(text) {
    let start = 0;
    let end = text.length;
    while (start < end && text[start] === ' ')
        start++;
    while (end > start && text[end - 1] === ' ')
        end--;
    return text.slice(start, end);
}
/**
 * Splits on the active delimiter, quote-aware, preserving empty tokens and
 * trimming exactly U+0020 around each token (§11.2). Content that trims to
 * nothing is zero cells.
 */
function splitCells(content, delimiter, line) {
    if (trimSpaces(content) === '')
        return [];
    const cells = [];
    let start = 0;
    let inQuotes = false;
    for (let i = 0; i < content.length; i++) {
        const ch = content[i];
        if (inQuotes) {
            if (ch === '\\')
                i++;
            else if (ch === '"')
                inQuotes = false;
            continue;
        }
        if (ch === '"')
            inQuotes = true;
        else if (ch === delimiter) {
            cells.push(trimSpaces(content.slice(start, i)));
            start = i + 1;
        }
    }
    cells.push(trimSpaces(content.slice(start)));
    return cells;
}
const NEED_LINE = Symbol('need line');
const MAX_CLASSIFIED_LOOKAHEAD = 2;
class Reader {
    lines = [];
    ended = false;
    previous;
    /** Depth of open header spans — blank lines inside one are strict errors (§12). */
    spanActive = 0;
    *peek(offset = 0) {
        if (offset >= MAX_CLASSIFIED_LOOKAHEAD) {
            throw new Error(`decoder lookahead exceeds ${MAX_CLASSIFIED_LOOKAHEAD} classified lines`);
        }
        while (!this.ended && this.lines.length <= offset) {
            const line = yield NEED_LINE;
            if (line === undefined)
                this.ended = true;
            else
                this.lines.push(line);
        }
        return this.lines[offset];
    }
    *take(ctx) {
        const line = yield* this.peek();
        if (line === undefined)
            throw new Error('decoder attempted to read past end of input');
        this.lines.shift();
        if (ctx.strict && this.spanActive > 0 && line.blankBefore) {
            throw toonError(line.number, 'blank line inside a header span');
        }
        this.previous = line;
        return line;
    }
    lastNumber(fallback) {
        return this.previous === undefined ? fallback : this.previous.number;
    }
    stop() {
        this.lines.length = 0;
        this.ended = true;
    }
}
function isKeyValueContent(content, line) {
    return findUnquoted(content, ':', line) !== -1;
}
/** §7.4: decoders accept any token as a key; quoted keys unescape per §7.1. */
function decodeKey(token, line) {
    try {
        return parseKey(token, line)[0];
    }
    catch (error) {
        if (token.startsWith('"'))
            throw error;
        return token;
    }
}
function decodeContext(options) {
    const rawMaxDepth = options?.maxDepth ?? DEFAULT_MAX_DEPTH;
    return {
        indentSize: options?.indentSize ?? options?.indent ?? 2,
        strict: options?.strict ?? true,
        maxDepth: rawMaxDepth === Number.POSITIVE_INFINITY
            ? 0
            : Math.max(0, Math.floor(rawMaxDepth)),
    };
}
function* parseEvents(ctx) {
    const reader = new Reader();
    const first = yield* reader.peek();
    if (first === undefined) {
        yield { type: 'startObject', line: 1 };
        yield { type: 'endObject', line: 1 };
        return;
    }
    if (first.depth !== 0)
        throw toonError(first.number, 'invalid indentation');
    // Fill the sole lookahead slot before root events so lexical/indentation
    // failures on the next content line retain their fail-before-event ordering.
    yield* reader.peek(1);
    // Root form discovery (§5).
    if (first.content === '[]') {
        yield* reader.take(ctx);
        yield { type: 'startArray', length: 0, line: first.number };
        yield { type: 'endArray', line: first.number };
        yield* expectEndOfDocument(reader, ctx);
        return;
    }
    let header = null;
    let headerFailed = false;
    try {
        header = parseHeader(first.content, first.number);
    }
    catch (error) {
        if (ctx.strict)
            throw error;
        headerFailed = true;
    }
    if (header !== null && header !== undefined)
        assertHeaderDepth(header.fields, first.number, ctx);
    if (header !== null && header !== undefined && !headerFailed && header.key === undefined) {
        yield* reader.take(ctx);
        if (header.keyed) {
            yield* emitKeyedObject(reader, first, header, ctx);
        }
        else {
            yield* emitArray(reader, first, header, ctx);
        }
        yield* expectEndOfDocument(reader, ctx);
        return;
    }
    if ((header === null || headerFailed) &&
        !isKeyValueContent(first.content, first.number) &&
        (yield* reader.peek(1)) === undefined) {
        yield* reader.take(ctx);
        yield {
            type: 'primitive',
            value: parseScalar(trimSpaces(first.content), first.number),
            line: first.number,
        };
        return;
    }
    yield* emitObject(reader, 0, first.number, ctx);
    yield* expectEndOfDocument(reader, ctx);
}
function* expectEndOfDocument(reader, ctx) {
    const trailing = yield* reader.peek();
    if (trailing !== undefined && ctx.strict) {
        throw toonError(trailing.number, 'expected end of document');
    }
    reader.stop();
}
// #region Objects (§8)
function* emitObject(reader, depth, startLine, ctx) {
    yield { type: 'startObject', line: startLine };
    const seen = new Set();
    while (true) {
        const line = yield* reader.peek();
        if (line === undefined || line.depth < depth)
            break;
        if (line.depth > depth)
            throw toonError(line.number, 'over-indented line');
        yield* reader.take(ctx);
        yield* emitEntry(reader, line, line.content, depth, ctx, seen);
    }
    yield { type: 'endObject', line: reader.lastNumber(startLine) };
}
/** One object entry whose content sits at `depth` (possibly carried on a hyphen line, §10). */
function* emitEntry(reader, line, content, depth, ctx, seen) {
    let header = null;
    try {
        header = parseHeader(content, line.number);
    }
    catch (error) {
        if (ctx.strict)
            throw error;
        header = null;
    }
    if (header !== null && header !== undefined) {
        assertHeaderDepth(header.fields, line.number, ctx);
        if (header.key === undefined) {
            if (ctx.strict) {
                throw toonError(line.number, 'keyless header is only valid at the root or as a list item');
            }
            header = null;
        }
        else {
            recordKey(seen, header.key, line.number, ctx);
            yield { type: 'key', key: header.key, line: line.number };
            if (header.keyed) {
                yield* emitKeyedObject(reader, { ...line, depth }, header, ctx);
            }
            else {
                yield* emitArray(reader, { ...line, depth }, header, ctx);
            }
            return;
        }
    }
    const colon = findUnquoted(content, ':', line.number);
    if (colon === -1)
        throw toonError(line.number, 'expected key-value line');
    const key = decodeKey(trimSpaces(content.slice(0, colon)), line.number);
    const rest = trimSpaces(content.slice(colon + 1));
    recordKey(seen, key, line.number, ctx);
    yield { type: 'key', key, line: line.number };
    if (rest === '') {
        const child = yield* reader.peek();
        if (child !== undefined && child.depth > depth) {
            if (child.depth !== depth + 1)
                throw toonError(child.number, 'over-indented line');
            yield* emitObject(reader, depth + 1, child.number, ctx);
        }
        else {
            yield { type: 'startObject', line: line.number };
            yield { type: 'endObject', line: line.number };
        }
        return;
    }
    if (rest === '[]') {
        yield { type: 'startArray', length: 0, line: line.number };
        yield { type: 'endArray', line: line.number };
        return;
    }
    yield { type: 'primitive', value: parseScalar(rest, line.number), line: line.number };
}
/** §14.3: duplicate sibling keys are a strict-mode error; non-strict is LWW. */
function recordKey(seen, key, line, ctx) {
    if (seen.has(key)) {
        if (ctx.strict)
            throw toonError(line, 'duplicate object key');
    }
    seen.add(key);
}
// #endregion
// #region Arrays (§9.1, §9.2, §9.4) and list items (§10)
function* emitArray(reader, header, info, ctx) {
    yield { type: 'startArray', length: info.length, line: header.number };
    if (info.fields !== undefined) {
        assertNoDuplicateFields(info.fields, header.number, ctx);
        yield* emitTabularRows(reader, header, info, ctx);
        return;
    }
    if (info.inline !== undefined) {
        const values = splitCells(info.inline, info.delimiter, header.number);
        assertCount(values.length, info.length, header.number, 'inline-form values', ctx);
        for (const value of values) {
            yield { type: 'primitive', value: parseScalar(value, header.number), line: header.number };
        }
        yield { type: 'endArray', line: header.number };
        return;
    }
    // List form: items at depth +1, each `- …` or the bare `-` (§9.4, §10).
    let items = 0;
    while (true) {
        const line = yield* reader.peek();
        if (line === undefined || line.depth <= header.depth)
            break;
        if (line.depth !== header.depth + 1)
            throw toonError(line.number, 'over-indented line');
        if (!line.content.startsWith('- ') && line.content !== '-')
            break;
        yield* reader.take(ctx);
        if (items === 0)
            reader.spanActive++;
        items++;
        yield* emitListItem(reader, line, ctx);
    }
    if (items > 0)
        reader.spanActive--;
    const endLine = reader.lastNumber(header.number);
    if (ctx.strict && items !== info.length) {
        throw toonError(endLine, `expected ${info.length} list items, but got ${items}`);
    }
    yield { type: 'endArray', line: endLine };
}
function* emitListItem(reader, line, ctx) {
    if (line.content === '-') {
        yield { type: 'startObject', line: line.number };
        yield { type: 'endObject', line: line.number };
        return;
    }
    const trimmed = trimSpaces(line.content.slice(2));
    if (trimmed === '[]') {
        yield { type: 'startArray', length: 0, line: line.number };
        yield { type: 'endArray', line: line.number };
        return;
    }
    let header = null;
    try {
        header = parseHeader(trimmed, line.number);
    }
    catch (error) {
        if (ctx.strict)
            throw error;
        header = null;
    }
    if (header !== null && header !== undefined && header.key === undefined) {
        assertHeaderDepth(header.fields, line.number, ctx);
        // A keyless non-keyed, non-fields header on a hyphen line is the item
        // itself; a fields-bearing or keyed one is only valid at the root (§6, §10).
        if (header.keyed || header.fields !== undefined) {
            if (ctx.strict) {
                throw toonError(line.number, 'keyless fields-bearing header is only valid at the root');
            }
            yield { type: 'primitive', value: parseScalar(trimmed, line.number), line: line.number };
            return;
        }
        yield* emitArray(reader, line, header, ctx);
        return;
    }
    if ((header !== null && header !== undefined && header.key !== undefined) ||
        isKeyValueContent(trimmed, line.number)) {
        // Object as list item: the first field stands at depth d+1 (§10).
        yield { type: 'startObject', line: line.number };
        const seen = new Set();
        yield* emitEntry(reader, line, trimmed, line.depth + 1, ctx, seen);
        while (true) {
            const next = yield* reader.peek();
            if (next === undefined || next.depth !== line.depth + 1)
                break;
            if (next.content.startsWith('- ') || next.content === '-')
                break;
            yield* reader.take(ctx);
            yield* emitEntry(reader, next, next.content, line.depth + 1, ctx, seen);
        }
        yield { type: 'endObject', line: reader.lastNumber(line.number) };
        return;
    }
    yield { type: 'primitive', value: parseScalar(trimmed, line.number), line: line.number };
}
// #endregion
// #region Tabular rows (§9.3)
function* emitTabularRows(reader, header, info, ctx) {
    const fields = info.fields;
    const leafCount = countLeaves(fields);
    const rowDepth = header.depth + 1;
    let rows = 0;
    while (true) {
        const line = yield* reader.peek();
        if (line === undefined || line.depth <= header.depth)
            break;
        if (line.depth !== rowDepth)
            throw toonError(line.number, 'over-indented line');
        if (!isRowLine(line.content, info.delimiter, line.number))
            break;
        yield* reader.take(ctx);
        if (rows === 0)
            reader.spanActive++;
        rows++;
        const cells = splitCells(line.content, info.delimiter, line.number);
        assertCount(cells.length, leafCount, line.number, 'row cells', ctx);
        yield* emitRowObject(fields, cells, { next: 0 }, line.number);
    }
    if (rows > 0)
        reader.spanActive--;
    const endLine = reader.lastNumber(header.number);
    if (ctx.strict && rows !== info.length) {
        throw toonError(endLine, `expected ${info.length} tabular rows, but got ${rows}`);
    }
    yield { type: 'endArray', line: endLine };
}
/** Row/key-value disambiguation at row depth (§9.3). */
function isRowLine(content, delimiter, line) {
    const colon = findUnquoted(content, ':', line);
    if (colon === -1)
        return true;
    const delim = findUnquoted(content, delimiter, line);
    if (delim === -1)
        return false;
    return delim < colon;
}
function* emitRowObject(fields, cells, cursor, line) {
    yield { type: 'startObject', line };
    for (const field of fields) {
        yield { type: 'key', key: field.name, line };
        if (field.children === undefined) {
            const cell = cells[cursor.next++] ?? '';
            yield { type: 'primitive', value: parseScalar(cell, line), line };
        }
        else {
            yield* emitRowObject(field.children, cells, cursor, line);
        }
    }
    yield { type: 'endObject', line };
}
// #endregion
// #region Keyed tabular objects (§9.5)
function* emitKeyedObject(reader, header, info, ctx) {
    const fields = info.fields;
    assertNoDuplicateFields(fields, header.number, ctx);
    const leafCount = countLeaves(fields);
    const entryDepth = header.depth + 1;
    yield { type: 'startObject', line: header.number };
    const seen = new Set();
    let rows = 0;
    while (true) {
        const line = yield* reader.peek();
        if (line === undefined || line.depth <= header.depth)
            break;
        if (line.depth !== entryDepth)
            throw toonError(line.number, 'over-indented line');
        const colon = findUnquoted(line.content, ':', line.number);
        if (colon === -1) {
            if (ctx.strict)
                throw toonError(line.number, 'expected a keyed entry row');
            yield* reader.take(ctx);
            continue;
        }
        yield* reader.take(ctx);
        if (rows === 0)
            reader.spanActive++;
        rows++;
        const key = decodeKey(trimSpaces(line.content.slice(0, colon)), line.number);
        recordKey(seen, key, line.number, ctx);
        yield { type: 'key', key, line: line.number };
        const cells = splitCells(line.content.slice(colon + 1), info.delimiter, line.number);
        assertCount(cells.length, leafCount, line.number, 'entry row cells', ctx);
        yield* emitRowObject(fields, cells, { next: 0 }, line.number);
    }
    if (rows > 0)
        reader.spanActive--;
    const endLine = reader.lastNumber(header.number);
    if (ctx.strict && rows !== info.length) {
        throw toonError(endLine, `expected ${info.length} entry rows, but got ${rows}`);
    }
    yield { type: 'endObject', line: endLine };
}
// #endregion
function assertCount(got, expected, line, what, ctx) {
    if (ctx.strict && got !== expected) {
        throw toonError(line, `expected ${expected} ${what}, but got ${got}`);
    }
}
function checkDepth(depth, line, ctx) {
    if (ctx.maxDepth !== 0 && depth > ctx.maxDepth) {
        throw toonError(line, `maximum nesting depth exceeded (maxDepth ${ctx.maxDepth})`);
    }
}
function assertHeaderDepth(fields, line, ctx, depth = 1) {
    if (fields === undefined)
        return;
    checkDepth(depth, line, ctx);
    for (const field of fields) {
        if (field.children !== undefined)
            assertHeaderDepth(field.children, line, ctx, depth + 1);
    }
}
export { ToonError };
/**
 * Synchronously decodes lines with at most two classified lines of lookahead.
 */
export function* decodeStreamSync(source, options) {
    const context = decodeContext(options);
    const classifier = new LineClassifier(context);
    const lines = classifyLines(source, classifier);
    const parser = parseEvents(context);
    let step = parser.next();
    try {
        while (!step.done) {
            if (step.value === NEED_LINE) {
                const next = lines.next();
                step = parser.next(next.done ? undefined : next.value);
            }
            else {
                yield step.value;
                step = parser.next();
            }
        }
    }
    catch (error) {
        throw withSource(error, classifier);
    }
    finally {
        parser.return();
        lines.return(undefined);
    }
}
/** Asynchronously decodes lines with the same bounded-lookahead parser. */
export async function* decodeStream(source, options) {
    const context = decodeContext(options);
    const classifier = new LineClassifier(context);
    const lines = classifyLinesAsync(source, classifier);
    const parser = parseEvents(context);
    let step = parser.next();
    try {
        while (!step.done) {
            if (step.value === NEED_LINE) {
                const next = await lines.next();
                step = parser.next(next.done ? undefined : next.value);
            }
            else {
                yield step.value;
                step = parser.next();
            }
        }
    }
    catch (error) {
        throw withSource(error, classifier);
    }
    finally {
        parser.return();
        await lines.return(undefined);
    }
}
function withSource(error, classifier) {
    if (!(error instanceof ToonError) || error.source !== undefined)
        return error;
    return new ToonDecodeError(error.reason, {
        line: error.line,
        source: classifier.sourceByLine.get(error.line),
        cause: error,
    });
}
export function decodeFromLines(source, options) {
    return Symbol.asyncIterator in source
        ? decodeStream(source, options)
        : decodeStreamSync(source, options);
}
