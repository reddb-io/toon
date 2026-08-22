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
  toon?: boolean;
}

export async function callAgent(
  baseUrl: string,
  agentName: string,
  parts: MessagePart[],
  options: AcpOptions = {},
): Promise<AgentRun> {
  const accept = options.toon ? 'application/toon' : 'application/json';
  const body = encode({ parts } as unknown as JsonValue);
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
  const value = decode(text) as unknown as AgentRun;
  return value;
}

export async function listAgents(
  baseUrl: string,
  options: AcpOptions = {},
): Promise<AgentSummary[]> {
  const accept = options.toon ? 'application/toon' : 'application/json';
  const response = await fetch(`${baseUrl}/agents`, {
    headers: { Accept: accept },
  });
  if (!response.ok) {
    throw new Error(`ACP list failed: ${response.status}`);
  }
  const text = await response.text();
  return decode(text) as unknown as AgentSummary[];
}
