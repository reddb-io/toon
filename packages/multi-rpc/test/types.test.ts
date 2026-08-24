import { encodeMessage } from '../dist/index.js';
import type { MessageDocument } from '../dist/index.js';

const batch = [
  { jsonrpc: '2.0', method: 'first', id: 1 },
  { jsonrpc: '2.0', method: 'second', id: 2 },
] as const;
const document: MessageDocument = batch;

encodeMessage(batch, 'jsonrpc');
encodeMessage(document, 'toonrpc');

void document;
