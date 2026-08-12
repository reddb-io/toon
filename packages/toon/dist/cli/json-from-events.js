/**
 * Renders the canonical decode event stream as JSON text, one piece at a time,
 * so a decode never materializes the whole document. Mirrors the upstream
 * `@toon-format/cli` writer, including its `indent: 0` compact form.
 */
export async function* jsonStreamFromEvents(events, indent = 2) {
    const stack = [];
    let depth = 0;
    for await (const event of events) {
        const parent = stack.length > 0 ? stack[stack.length - 1] : undefined;
        switch (event.type) {
            case 'startObject': {
                yield* emitValuePrefix(parent, depth, indent);
                yield '{';
                stack.push({ type: 'object', needsComma: false, expectValue: false });
                depth++;
                break;
            }
            case 'endObject': {
                const context = stack.pop();
                if (!context || context.type !== 'object') {
                    throw new Error('Mismatched endObject event');
                }
                depth--;
                if (indent > 0 && context.needsComma) {
                    yield '\n';
                    yield ' '.repeat(depth * indent);
                }
                yield '}';
                markValueComplete(stack[stack.length - 1]);
                break;
            }
            case 'startArray': {
                yield* emitValuePrefix(parent, depth, indent);
                yield '[';
                stack.push({ type: 'array', needsComma: false });
                depth++;
                break;
            }
            case 'endArray': {
                const context = stack.pop();
                if (!context || context.type !== 'array') {
                    throw new Error('Mismatched endArray event');
                }
                depth--;
                if (indent > 0 && context.needsComma) {
                    yield '\n';
                    yield ' '.repeat(depth * indent);
                }
                yield ']';
                markValueComplete(stack[stack.length - 1]);
                break;
            }
            case 'key': {
                if (!parent || parent.type !== 'object') {
                    throw new Error('Key event outside of object context');
                }
                if (parent.needsComma) {
                    yield ',';
                }
                if (indent > 0) {
                    yield '\n';
                    yield ' '.repeat(depth * indent);
                }
                yield JSON.stringify(event.key);
                yield indent > 0 ? ': ' : ':';
                parent.expectValue = true;
                parent.needsComma = true;
                break;
            }
            case 'primitive': {
                if (parent?.type === 'object' && !parent.expectValue) {
                    throw new Error('Primitive event without preceding key in object');
                }
                yield* emitValuePrefix(parent, depth, indent);
                yield JSON.stringify(event.value);
                markValueComplete(parent);
                break;
            }
        }
    }
    if (stack.length !== 0) {
        throw new Error('Incomplete event stream: unclosed objects or arrays');
    }
}
function* emitValuePrefix(parent, depth, indent) {
    if (parent?.type !== 'array')
        return;
    if (parent.needsComma) {
        yield ',';
    }
    if (indent > 0) {
        yield '\n';
        yield ' '.repeat(depth * indent);
    }
}
function markValueComplete(parent) {
    if (parent?.type === 'object') {
        parent.expectValue = false;
        parent.needsComma = true;
    }
    else if (parent?.type === 'array') {
        parent.needsComma = true;
    }
}
