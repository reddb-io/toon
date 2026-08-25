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
import type { JsonValue } from '@reddb-io/toon';

export const ACP_API_VERSION = '0.1.0';

export type RunStatus =
  | 'created'
  | 'in_progress'
  | 'awaiting'
  | 'cancelled'
  | 'failed'
  | 'completed';

export interface Agent {
  name: string;
  description: string;
  version?: string;
  metadata?: JsonValue;
}

export interface AgentSummary {
  name: string;
  description: string;
  version?: string;
}

export interface MessagePart {
  kind: 'text' | 'file' | 'data' | 'resource' | 'resource_link';
  content_type?: string;
  content?: JsonValue;
  content_encoding?: string;
  content_url?: string;
  status: 'in_progress' | 'done' | 'failed';
}

export interface AgentMessage {
  role: string;
  parts: MessagePart[];
  metadata?: JsonValue;
}

export interface AgentRunInput {
  parts: MessagePart[];
}

export interface AgentRun {
  agentRunId: string;
  agentName: string;
  status: RunStatus;
  input: AgentRunInput;
  output: AgentMessage[];
  error?: { code: number; message: string; data?: JsonValue };
  metadata?: JsonValue;
}

export interface AcpService {
  listAgents(): Agent[];
  getAgent(name: string): Agent | undefined;
  run(agent: string, input: MessagePart[]): AgentRun;
  cancel(runId: string): Promise<void> | void;
}

export interface AcpOptions {
  /**
   * Send and receive TOON instead of JSON. This switches the request body
   * encoding, the `Content-Type`, the `Accept` header and the response parser
   * together — they are never allowed to disagree.
   */
  toon?: boolean;
  /** Abort signal for the underlying fetch. */
  signal?: AbortSignal;
  /** Abort the request after this many milliseconds. */
  timeoutMs?: number;
}

const MEDIA_TYPE_TOON = 'application/toon';
const MEDIA_TYPE_JSON = 'application/json';

function mediaType(options: AcpOptions): string {
  return options.toon ? MEDIA_TYPE_TOON : MEDIA_TYPE_JSON;
}

function parseBody<T>(text: string, options: AcpOptions, what: string): T {
  if (options.toon) {
    return decode(text) as unknown as T;
  }
  try {
    return JSON.parse(text) as T;
  } catch (cause) {
    throw new Error(`ACP ${what} returned invalid JSON`, { cause });
  }
}

/**
 * Run one fetch with an optional caller signal and an optional timeout, and
 * clean up the timer whichever way the request ends.
 */
async function acpFetch(
  url: string,
  init: RequestInit,
  options: AcpOptions,
): Promise<Response> {
  const { signal, timeoutMs } = options;
  if (timeoutMs === undefined) {
    return await fetch(url, signal ? { ...init, signal } : init);
  }
  const controller = new AbortController();
  const onAbort = () => controller.abort(signal?.reason);
  if (signal) {
    if (signal.aborted) {
      onAbort();
    } else {
      signal.addEventListener('abort', onAbort, { once: true });
    }
  }
  const timer = setTimeout(
    () => controller.abort(new Error(`ACP request timed out after ${timeoutMs}ms`)),
    timeoutMs,
  );
  try {
    return await fetch(url, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener('abort', onAbort);
  }
}

export async function callAgent(
  baseUrl: string,
  agentName: string,
  parts: MessagePart[],
  options: AcpOptions = {},
): Promise<AgentRun> {
  const type = mediaType(options);
  const payload = { parts } as unknown as JsonValue;
  const body = options.toon ? encode(payload) : JSON.stringify(payload);
  const response = await acpFetch(
    `${baseUrl}/agents/${agentName}/runs`,
    {
      method: 'POST',
      headers: { 'Content-Type': type, Accept: type },
      body,
    },
    options,
  );
  if (!response.ok) {
    throw new Error(`ACP call failed: ${response.status} ${response.statusText}`);
  }
  return parseBody<AgentRun>(await response.text(), options, 'call');
}

export async function listAgents(
  baseUrl: string,
  options: AcpOptions = {},
): Promise<AgentSummary[]> {
  const response = await acpFetch(
    `${baseUrl}/agents`,
    { headers: { Accept: mediaType(options) } },
    options,
  );
  if (!response.ok) {
    throw new Error(`ACP list failed: ${response.status}`);
  }
  return parseBody<AgentSummary[]>(await response.text(), options, 'list');
}
