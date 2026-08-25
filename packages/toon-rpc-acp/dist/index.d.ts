import type { JsonValue } from '@reddb-io/toon';
export declare const ACP_API_VERSION = "0.1.0";
export type RunStatus = 'created' | 'in_progress' | 'awaiting' | 'cancelled' | 'failed' | 'completed';
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
    error?: {
        code: number;
        message: string;
        data?: JsonValue;
    };
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
export declare function callAgent(baseUrl: string, agentName: string, parts: MessagePart[], options?: AcpOptions): Promise<AgentRun>;
export declare function listAgents(baseUrl: string, options?: AcpOptions): Promise<AgentSummary[]>;
//# sourceMappingURL=index.d.ts.map