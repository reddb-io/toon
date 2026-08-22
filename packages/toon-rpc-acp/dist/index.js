import { decode, encode } from '@reddb-io/toon';
export const ACP_API_VERSION = '0.1.0';
export async function callAgent(baseUrl, agentName, parts, options = {}) {
    const accept = options.toon ? 'application/toon' : 'application/json';
    const body = encode({ parts });
    const response = await fetch(`${baseUrl}/agents/${agentName}/runs`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            Accept: accept,
        },
        body: JSON.stringify({ parts }),
    });
    if (!response.ok) {
        throw new Error(`ACP call failed: ${response.status} ${response.statusText}`);
    }
    const text = await response.text();
    const value = decode(text);
    return value;
}
export async function listAgents(baseUrl, options = {}) {
    const accept = options.toon ? 'application/toon' : 'application/json';
    const response = await fetch(`${baseUrl}/agents`, {
        headers: { Accept: accept },
    });
    if (!response.ok) {
        throw new Error(`ACP list failed: ${response.status}`);
    }
    const text = await response.text();
    return decode(text);
}
//# sourceMappingURL=index.js.map