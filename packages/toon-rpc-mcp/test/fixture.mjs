/** Shared fixture service for the conformance and transport tests. */

import { CallToolResult, McpError } from '../dist/index.js';

export const fixtureService = {
  serverInfo: () => ({ name: 'fixture-server', version: '1.0.0' }),
  instructions: () => 'Fixture server for conformance tests.',
  listTools: () => [
    {
      name: 'echo',
      title: 'Echo',
      description: 'Echo the supplied text back',
      inputSchema: {
        type: 'object',
        properties: { text: { type: 'string' } },
        required: ['text'],
        additionalProperties: false,
      },
    },
  ],
  listResources: () => [
    {
      uri: 'file:///fixture/readme.md',
      name: 'readme.md',
      mimeType: 'text/markdown',
    },
  ],
  listPrompts: () => [
    {
      name: 'greet',
      description: 'Greet someone',
      arguments: [{ name: 'who', required: true }],
    },
  ],
  readResource: (uri) =>
    uri === 'file:///fixture/readme.md'
      ? [{ uri, mimeType: 'text/markdown', text: '# Fixture' }]
      : [],
  getPrompt: (name, args) => {
    if (name !== 'greet') throw McpError.promptNotFound(name);
    const who = args?.who ?? 'world';
    return {
      resultType: 'complete',
      description: 'Greet someone',
      messages: [{ role: 'user', content: { type: 'text', text: `Hello, ${who}!` } }],
    };
  },
  callTool: (_name, args) =>
    typeof args?.text === 'string'
      ? CallToolResult.text(args.text)
      : CallToolResult.error('missing "text" argument'),
};
