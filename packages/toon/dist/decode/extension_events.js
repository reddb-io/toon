/** Event emitters for the reddb array-column extensions. */
import { toonError } from '../errors.js';
import { findUnquoted, parseScalar, splitDelimited } from '../lexical.js';
/** Decode a buffered tabular span and expose only JSON-semantic events. */
export function* emitExtensionRows(fields, lines, length, delimiter, rowDepth, headerLine, options) {
    const cursor = { index: 0 };
    yield* emitStructuredRows(length, fields, delimiter, lines, cursor, rowDepth, options, true, headerLine);
}
function* emitStructuredRows(length, fields, delimiter, lines, cursor, rowDepth, options, root, fallbackLine) {
    const childTableFields = inferChildTableFields(length, fields, delimiter, lines, cursor.index, rowDepth, options);
    let rows = 0;
    while (rows < length) {
        const line = lines[cursor.index];
        if (line === undefined || line.depth < rowDepth)
            break;
        if (line.depth > rowDepth)
            throw toonError(line.number, 'invalid indentation');
        if (line.blankBefore && options.strict)
            throw toonError(line.number, 'blank line inside array');
        if (!isTabularRow(line.content, delimiter, line.number))
            break;
        const cells = splitDelimited(line.content, delimiter, line.number);
        const state = {
            cellIndex: 0,
            nextIndex: cursor.index + 1,
            flatWidth: leafWidth(fields),
            childTableFields,
        };
        if (root && fields.length === 1 && fields[0].fixedLength !== undefined) {
            yield* emitFixedList(fields[0], cells, state, line.number);
        }
        else {
            yield { type: 'startObject', line: line.number };
            yield* emitStructuredFields(fields, cells, line, lines, state, rowDepth + 1, delimiter, options);
            yield { type: 'endObject', line: line.number };
        }
        if (state.cellIndex !== cells.length)
            throw toonError(line.number, 'array row length mismatch');
        cursor.index = state.nextIndex;
        rows++;
    }
    if (rows !== length)
        throw lengthMismatch(lines, cursor.index, fallbackLine);
    const next = lines[cursor.index];
    if (next !== undefined && next.depth >= rowDepth && isTabularRow(next.content, delimiter, next.number)) {
        throw toonError(next.number, 'array length mismatch');
    }
}
function* emitStructuredFields(fields, cells, line, lines, state, childDepth, delimiter, options) {
    for (let index = 0; index < fields.length; index++) {
        const field = fields[index];
        yield { type: 'key', key: field.name, line: line.number };
        yield* emitStructuredField(field, fields.slice(index + 1), cells, line, lines, state, childDepth, delimiter, options);
    }
}
function* emitStructuredField(field, remainingFields, cells, line, lines, state, childDepth, delimiter, options) {
    if (field.fixedLength !== undefined) {
        yield* emitFixedList(field, cells, state, line.number);
        return;
    }
    if (field.listDelimiter !== undefined) {
        const cell = takeCell(cells, state, line.number);
        const values = splitDelimited(cell, field.listDelimiter, line.number);
        yield { type: 'startArray', length: values.length, line: line.number };
        for (const value of values) {
            yield { type: 'primitive', value: parseScalar(value, line.number), line: line.number };
        }
        yield { type: 'endArray', line: line.number };
        return;
    }
    if (field.children !== undefined) {
        const flatWidth = leafWidth(field.children);
        const countCell = cells[state.cellIndex];
        const count = parseChildCount(countCell);
        const cellsAfterCount = cells.length - state.cellIndex - 1;
        const childLine = lines[state.nextIndex];
        const hasChildRows = childLine !== undefined && childLine.depth === childDepth;
        const knownChildTable = state.childTableFields?.has(field);
        const childTable = knownChildTable ?? (count !== undefined &&
            (hasChildRows || (cells.length !== state.flatWidth &&
                cellsAfterCount < flatWidth + minimumRowWidth(remainingFields))));
        if (childTable) {
            if (!options.objectArrayColumns || count === undefined) {
                throw toonError(line.number, 'array row length mismatch');
            }
            state.cellIndex++;
            yield { type: 'startArray', length: count, line: line.number };
            const childCursor = { index: state.nextIndex };
            yield* emitStructuredRows(count, field.children, delimiter, lines, childCursor, childDepth, options, false, line.number);
            state.nextIndex = childCursor.index;
            yield { type: 'endArray', line: lines[state.nextIndex - 1]?.number ?? line.number };
            return;
        }
        yield { type: 'startObject', line: line.number };
        yield* emitStructuredFields(field.children, cells, line, lines, state, childDepth, delimiter, options);
        yield { type: 'endObject', line: line.number };
        return;
    }
    const cell = takeCell(cells, state, line.number);
    yield { type: 'primitive', value: parseScalar(cell, line.number), line: line.number };
}
function* emitFixedList(field, cells, state, line) {
    const length = field.fixedLength;
    if (state.cellIndex + length > cells.length)
        throw toonError(line, 'array row length mismatch');
    yield { type: 'startArray', length, line };
    for (const cell of cells.slice(state.cellIndex, state.cellIndex + length)) {
        yield { type: 'primitive', value: parseScalar(cell, line), line };
    }
    state.cellIndex += length;
    yield { type: 'endArray', line };
}
function takeCell(cells, state, line) {
    const cell = cells[state.cellIndex];
    if (cell === undefined)
        throw toonError(line, 'array row length mismatch');
    state.cellIndex++;
    return cell;
}
function inferChildTableFields(length, fields, delimiter, lines, startIndex, rowDepth, options) {
    const candidates = fields.filter((field) => field.children !== undefined);
    if (candidates.length === 0)
        return new Set();
    if (candidates.length > 12)
        return undefined;
    let best;
    for (let mask = 0; mask < 1 << candidates.length; mask++) {
        const selected = new Set();
        candidates.forEach((field, index) => {
            if ((mask & (1 << index)) !== 0)
                selected.add(field);
        });
        const result = validateRows(length, fields, selected, delimiter, lines, startIndex, rowDepth, options);
        if (result !== undefined &&
            (best === undefined || result.consumed > best.consumed ||
                (result.consumed === best.consumed && selected.size < best.fields.size))) {
            best = { fields: selected, consumed: result.consumed };
        }
    }
    return best?.fields;
}
function validateRows(length, fields, childTableFields, delimiter, lines, startIndex, rowDepth, options) {
    let index = startIndex;
    let consumed = 0;
    try {
        for (let row = 0; row < length; row++) {
            const line = lines[index];
            if (line === undefined || line.depth !== rowDepth ||
                (line.blankBefore && options.strict) ||
                !isTabularRow(line.content, delimiter, line.number))
                return undefined;
            const result = validateRow(fields, childTableFields, splitDelimited(line.content, delimiter, line.number), delimiter, lines, index + 1, rowDepth + 1, options);
            if (result === undefined || lines[result.nextIndex]?.depth > rowDepth)
                return undefined;
            index = result.nextIndex;
            consumed += result.consumed;
        }
        const next = lines[index];
        if (next !== undefined && next.depth >= rowDepth && isTabularRow(next.content, delimiter, next.number)) {
            return undefined;
        }
    }
    catch {
        return undefined;
    }
    return { nextIndex: index, consumed };
}
function validateRow(fields, childTableFields, cells, delimiter, lines, startIndex, childDepth, options) {
    let cellIndex = 0;
    let nextIndex = startIndex;
    let consumed = 0;
    for (const field of fields) {
        if (field.fixedLength !== undefined) {
            cellIndex += field.fixedLength;
        }
        else if (field.children !== undefined) {
            if (childTableFields.has(field)) {
                const count = parseChildCount(cells[cellIndex]);
                if (count === undefined)
                    return undefined;
                cellIndex++;
                const nested = inferChildTableFields(count, field.children, delimiter, lines, nextIndex, childDepth, options);
                if (nested === undefined)
                    return undefined;
                const result = validateRows(count, field.children, nested, delimiter, lines, nextIndex, childDepth, options);
                if (result === undefined)
                    return undefined;
                nextIndex = result.nextIndex;
                consumed += count + result.consumed;
            }
            else {
                cellIndex += leafWidth(field.children);
            }
        }
        else {
            cellIndex++;
        }
        if (cellIndex > cells.length)
            return undefined;
    }
    return cellIndex === cells.length ? { nextIndex, consumed } : undefined;
}
function parseChildCount(value) {
    return value !== undefined && /^(0|[1-9][0-9]*)$/.test(value) ? Number(value) : undefined;
}
function leafWidth(fields) {
    return fields.reduce((total, field) => total + fieldWidth(field), 0);
}
function fieldWidth(field) {
    if (field.fixedLength !== undefined)
        return field.fixedLength;
    if (field.children !== undefined)
        return leafWidth(field.children);
    return 1;
}
function minimumRowWidth(fields) {
    return fields.reduce((total, field) => total + (field.children === undefined ? fieldWidth(field) : 1), 0);
}
function isTabularRow(content, delimiter, line) {
    const colon = findUnquoted(content, ':', line);
    if (colon === -1)
        return true;
    const delimiterIndex = findUnquoted(content, delimiter, line);
    return delimiterIndex !== -1 && delimiterIndex < colon;
}
function lengthMismatch(lines, index, fallbackLine) {
    return toonError(lines[index]?.number ?? lines[lines.length - 1]?.number ?? fallbackLine, 'array length mismatch');
}
