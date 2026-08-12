/**
 * Renders the canonical decode event stream as JSON text, one piece at a time,
 * so a decode never materializes the whole document. Mirrors the upstream
 * `@toon-format/cli` writer, including its `indent: 0` compact form.
 */
import type { ToonEvent } from '../events.js';
export declare function jsonStreamFromEvents(events: AsyncIterable<ToonEvent>, indent?: number): AsyncIterable<string>;
