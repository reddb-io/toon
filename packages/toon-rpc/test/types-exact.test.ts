import type { CallOptions, ErrorObject, Notification, Request } from '../dist/index.js';

// @ts-expect-error exactOptionalPropertyTypes rejects present undefined data.
const error: ErrorObject = { code: 1, message: 'invalid', data: undefined };

// @ts-expect-error exactOptionalPropertyTypes rejects present undefined params.
const request: Request = { toonrpc: '1.0', method: 'call', params: undefined, id: 1 };

// @ts-expect-error exactOptionalPropertyTypes rejects an explicit notification id.
const notification: Notification = { toonrpc: '1.0', method: 'notify', id: undefined };

// @ts-expect-error exactOptionalPropertyTypes rejects an explicit undefined ID.
const callOptions: CallOptions = { id: undefined };

void error;
void request;
void notification;
void callOptions;
