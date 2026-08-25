/**
 * Client for this repository's legacy ACP-style REST contract.
 *
 * **Legacy and terminal.** This is not IBM/BeeAI's Agent Communication
 * Protocol and not Zed's Agent Client Protocol, and it does not interoperate
 * with either — the run envelope, the message-part model and the status
 * vocabulary were invented here. The wire shapes are pinned by
 * `docs/acp-legacy-openapi.yaml` and are frozen: only correctness, safety and
 * lifecycle fixes that keep them byte-identical land here.
 */
import { decode, encode } from '@reddb-io/toon';
export const ACP_API_VERSION = '0.1.0';
const MEDIA_TYPE_TOON = 'application/toon';
const MEDIA_TYPE_JSON = 'application/json';
function mediaType(options) {
    return options.toon ? MEDIA_TYPE_TOON : MEDIA_TYPE_JSON;
}
function parseBody(text, options, what) {
    if (options.toon) {
        return decode(text);
    }
    try {
        return JSON.parse(text);
    }
    catch (cause) {
        throw new Error(`ACP ${what} returned invalid JSON`, { cause });
    }
}
/**
 * Run one fetch with an optional caller signal and an optional timeout, and
 * clean up the timer whichever way the request ends.
 */
async function acpFetch(url, init, options) {
    const { signal, timeoutMs } = options;
    if (timeoutMs === undefined) {
        return await fetch(url, signal ? { ...init, signal } : init);
    }
    const controller = new AbortController();
    const onAbort = () => controller.abort(signal?.reason);
    if (signal) {
        if (signal.aborted) {
            onAbort();
        }
        else {
            signal.addEventListener('abort', onAbort, { once: true });
        }
    }
    const timer = setTimeout(() => controller.abort(new Error(`ACP request timed out after ${timeoutMs}ms`)), timeoutMs);
    try {
        return await fetch(url, { ...init, signal: controller.signal });
    }
    finally {
        clearTimeout(timer);
        signal?.removeEventListener('abort', onAbort);
    }
}
export async function callAgent(baseUrl, agentName, parts, options = {}) {
    const type = mediaType(options);
    const payload = { parts };
    const body = options.toon ? encode(payload) : JSON.stringify(payload);
    const response = await acpFetch(`${baseUrl}/agents/${agentName}/runs`, {
        method: 'POST',
        headers: { 'Content-Type': type, Accept: type },
        body,
    }, options);
    if (!response.ok) {
        throw new Error(`ACP call failed: ${response.status} ${response.statusText}`);
    }
    return parseBody(await response.text(), options, 'call');
}
export async function listAgents(baseUrl, options = {}) {
    const response = await acpFetch(`${baseUrl}/agents`, { headers: { Accept: mediaType(options) } }, options);
    if (!response.ok) {
        throw new Error(`ACP list failed: ${response.status}`);
    }
    return parseBody(await response.text(), options, 'list');
}
//# sourceMappingURL=index.js.map