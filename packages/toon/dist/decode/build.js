/**
 * Builds a JSON value tree from the decode event stream — the whole-document
 * convenience over `decodeStreamSync` (ADR 0006). Mirrors upstream's
 * event-builder layering: the stream is the core, the tree is derived.
 */
import { decodeStreamSync } from './stream.js';
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
export function decodeValue(input, options) {
    return buildValueFromEvents(decodeStreamSync(input.split('\n'), options));
}
