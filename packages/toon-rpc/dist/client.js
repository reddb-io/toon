import { decode, encode } from '@reddb-io/toon';
import { TOONRPC_VERSION, isId, snapshotRequestObject, snapshotResponse, } from './protocol.js';
import { RpcError } from './rpc-error.js';
export class ClientClosedError extends Error {
    constructor(message = 'TOON-RPC client is closed') {
        super(message);
        this.name = 'ClientClosedError';
    }
}
export class ClientAbortError extends Error {
    constructor() {
        super('TOON-RPC operation was aborted');
        this.name = 'ClientAbortError';
    }
}
export class ClientTimeoutError extends Error {
    constructor(timeoutMs) {
        super(`TOON-RPC call timed out after ${timeoutMs}ms`);
        this.name = 'ClientTimeoutError';
    }
}
export class ClientProtocolError extends Error {
    constructor(message) {
        super(message);
        this.name = 'ClientProtocolError';
    }
}
const hasOwn = Object.hasOwn;
export class Client {
    transport;
    options;
    idCounter = 0;
    pending = new Map();
    state = 'idle';
    terminalError;
    terminalController = new AbortController();
    resolveTermination;
    termination = new Promise((resolve) => {
        this.resolveTermination = resolve;
    });
    startPromise;
    receivePromise;
    transportClosePromise;
    closePromise;
    constructor(transport, options = {}) {
        this.transport = transport;
        this.options = options;
    }
    get status() {
        return this.state;
    }
    get pendingCallCount() {
        return this.pending.size;
    }
    start() {
        return this.ensureOpen();
    }
    call(method, params, options = {}) {
        let id;
        let document;
        let timeoutMs;
        let signal;
        try {
            signal = options.signal;
            if (signal?.aborted)
                return Promise.reject(new ClientAbortError());
            timeoutMs = validateTimeout(options.timeoutMs);
            id = hasOwn(options, 'id') ? options.id : this.allocateId();
            if (!isId(id))
                throw new TypeError('TOON-RPC call ID must be a string, safe integer, or null');
            if (this.pending.has(id))
                throw new Error(`TOON-RPC call ID is already pending: ${String(id)}`);
            document = encodeRequest(method, params, id);
        }
        catch (error) {
            return Promise.reject(asError(error));
        }
        return new Promise((resolve, reject) => {
            const pending = { resolve, reject, controller: new AbortController() };
            this.pending.set(id, pending);
            if (signal) {
                pending.signal = signal;
                pending.abort = () => this.rejectPending(id, new ClientAbortError());
                signal.addEventListener('abort', pending.abort, { once: true });
                if (signal.aborted)
                    pending.abort();
            }
            if (timeoutMs !== undefined && this.pending.has(id)) {
                pending.timer = setTimeout(() => this.rejectPending(id, new ClientTimeoutError(timeoutMs)), timeoutMs);
            }
            if (this.pending.has(id))
                void this.dispatchCall(id, document);
        });
    }
    async notify(method, params, options = {}) {
        const timeoutMs = validateTimeout(options.timeoutMs);
        if (options.signal?.aborted)
            throw new ClientAbortError();
        const document = encodeRequest(method, params);
        const operation = new OperationScope(options.signal, timeoutMs, this.terminalController.signal, () => this.terminalError ?? new ClientClosedError());
        try {
            await operation.race(this.ensureOpen());
            this.assertOpen();
            if (this.transport.kind === 'duplex') {
                await operation.race(this.transport.send(document, operationOptions(operation.signal)));
            }
            else {
                const response = await operation.race(this.transport.request(document, operationOptions(operation.signal)));
                if (response && response.length > 0)
                    this.processDocument(response, {});
            }
        }
        finally {
            operation.dispose();
        }
    }
    close() {
        if (this.closePromise)
            return this.closePromise;
        if (this.state !== 'closed' && this.state !== 'failed') {
            this.terminate('closed', new ClientClosedError());
        }
        this.closePromise = (async () => {
            const [transportResult, receiveResult] = await Promise.allSettled([
                this.closeTransport(),
                this.receivePromise ?? Promise.resolve(),
            ]);
            if (transportResult.status === 'rejected')
                throw transportResult.reason;
            if (receiveResult.status === 'rejected')
                throw receiveResult.reason;
        })();
        return this.closePromise;
    }
    async dispatchCall(id, document) {
        try {
            await this.ensureOpen();
            const pending = this.pending.get(id);
            if (!pending)
                return;
            this.assertOpen();
            if (this.transport.kind === 'duplex') {
                await this.transport.send(document, operationOptions(pending.controller.signal));
                return;
            }
            const response = await this.transport.request(document, operationOptions(pending.controller.signal));
            if (!this.pending.has(id))
                return;
            if (!response || response.length === 0) {
                this.rejectPending(id, new ClientProtocolError('Request/response transport returned no response'));
                return;
            }
            this.processDocument(response, { id });
            if (this.pending.has(id)) {
                this.rejectPending(id, new ClientProtocolError('Request/response document did not contain the matching response'));
            }
        }
        catch (error) {
            this.rejectPending(id, asError(error));
        }
    }
    ensureOpen() {
        if (this.state === 'open')
            return Promise.resolve();
        if (this.state === 'closed' || this.state === 'failed') {
            return Promise.reject(this.terminalError ?? new ClientClosedError());
        }
        if (this.startPromise)
            return this.startPromise;
        this.state = 'opening';
        this.startPromise = (async () => {
            try {
                const opening = this.transport.open?.({ signal: this.terminalController.signal }) ?? Promise.resolve();
                await Promise.race([
                    opening,
                    this.termination.then((error) => Promise.reject(error)),
                ]);
                if (this.state !== 'opening')
                    throw this.terminalError ?? new ClientClosedError();
                this.state = 'open';
                if (this.transport.kind === 'duplex') {
                    this.receivePromise = this.receiveLoop();
                }
            }
            catch (error) {
                const failure = asError(error);
                if (this.state !== 'closed' && this.state !== 'failed')
                    this.terminate('failed', failure);
                throw failure;
            }
        })();
        return this.startPromise;
    }
    async receiveLoop() {
        if (this.transport.kind !== 'duplex')
            return;
        try {
            for await (const document of this.transport.receive({ signal: this.terminalController.signal })) {
                if (this.state !== 'open')
                    return;
                this.processDocument(document);
            }
            if (this.state === 'open') {
                this.terminate('closed', new ClientClosedError('TOON-RPC transport closed'));
            }
        }
        catch (error) {
            if (this.state === 'open')
                this.terminate('failed', asError(error));
        }
    }
    processDocument(document, scope) {
        let value;
        try {
            const text = new TextDecoder('utf-8', { fatal: true }).decode(document);
            value = decode(text);
        }
        catch (error) {
            this.diagnostic({ reason: 'parse-error', error });
            return;
        }
        if (!Array.isArray(value)) {
            const response = snapshotResponse(value);
            if (!response) {
                this.diagnostic({ reason: 'invalid-response' });
                return;
            }
            this.settleResponse(response, undefined, scope);
            return;
        }
        if (value.length === 0) {
            this.diagnostic({ reason: 'invalid-response' });
            return;
        }
        const settledIds = new Set();
        value.forEach((entry, index) => {
            const response = snapshotResponse(entry);
            if (!response) {
                this.diagnostic({ reason: 'invalid-response', index });
            }
            else if (settledIds.has(response.id)) {
                this.diagnostic({ reason: 'duplicate-id', id: response.id, index });
            }
            else if (this.settleResponse(response, index, scope)) {
                settledIds.add(response.id);
            }
        });
    }
    settleResponse(response, index, scope) {
        if (scope && (!hasOwn(scope, 'id') || response.id !== scope.id)) {
            this.diagnostic({ reason: 'unknown-id', id: response.id, ...(index === undefined ? {} : { index }) });
            return false;
        }
        const pending = this.takePending(response.id);
        if (!pending) {
            this.diagnostic({ reason: 'unknown-id', id: response.id, ...(index === undefined ? {} : { index }) });
            return false;
        }
        if ('error' in response) {
            const error = hasOwn(response.error, 'data')
                ? new RpcError(response.error.code, response.error.message, response.error.data)
                : new RpcError(response.error.code, response.error.message);
            pending.reject(error);
        }
        else {
            pending.resolve(response.result);
        }
        return true;
    }
    rejectPending(id, error) {
        const pending = this.takePending(id);
        if (!pending)
            return false;
        pending.reject(error);
        return true;
    }
    takePending(id) {
        const pending = this.pending.get(id);
        if (!pending)
            return undefined;
        this.pending.delete(id);
        if (pending.timer !== undefined)
            clearTimeout(pending.timer);
        if (pending.signal && pending.abort)
            pending.signal.removeEventListener('abort', pending.abort);
        pending.controller.abort();
        return pending;
    }
    rejectAll(error) {
        for (const id of [...this.pending.keys()])
            this.rejectPending(id, error);
    }
    terminate(status, error) {
        if (this.state === 'closed' || this.state === 'failed')
            return;
        this.state = status;
        this.terminalError = error;
        this.resolveTermination(error);
        this.terminalController.abort(error);
        this.rejectAll(error);
        void this.closeTransport().catch(() => { });
    }
    closeTransport() {
        this.transportClosePromise ??= Promise.resolve().then(() => this.transport.close());
        return this.transportClosePromise;
    }
    assertOpen() {
        if (this.state !== 'open')
            throw this.terminalError ?? new ClientClosedError();
    }
    diagnostic(diagnostic) {
        try {
            this.options.onDiagnostic?.(diagnostic);
        }
        catch {
            // Diagnostics cannot take ownership of the receive loop.
        }
    }
    allocateId() {
        while (this.pending.has(this.idCounter))
            this.idCounter += 1;
        if (!Number.isSafeInteger(this.idCounter))
            throw new Error('TOON-RPC numeric ID space exhausted');
        return this.idCounter++;
    }
}
function encodeRequest(method, params, id) {
    const source = {
        toonrpc: TOONRPC_VERSION,
        method,
        ...(params === undefined ? {} : { params }),
        ...(arguments.length >= 3 ? { id } : {}),
    };
    const request = snapshotRequestObject(source);
    if (!request)
        throw new TypeError('Invalid TOON-RPC request');
    return new TextEncoder().encode(encode(request));
}
function validateTimeout(timeoutMs) {
    if (timeoutMs === undefined)
        return undefined;
    if (!Number.isFinite(timeoutMs) || timeoutMs < 0 || timeoutMs > 2147483647) {
        throw new RangeError('TOON-RPC timeout must be between 0 and 2147483647ms');
    }
    return timeoutMs;
}
function operationOptions(signal) {
    return signal ? { signal } : undefined;
}
function asError(error) {
    return error instanceof Error ? error : new Error(String(error));
}
class OperationScope {
    callerSignal;
    terminalSignal;
    terminalError;
    controller = new AbortController();
    signal = this.controller.signal;
    cancellation;
    rejectCancellation;
    timer;
    active = true;
    constructor(callerSignal, timeoutMs, terminalSignal, terminalError) {
        this.callerSignal = callerSignal;
        this.terminalSignal = terminalSignal;
        this.terminalError = terminalError;
        this.cancellation = new Promise((_, reject) => {
            this.rejectCancellation = reject;
        });
        callerSignal?.addEventListener('abort', this.abortFromCaller, { once: true });
        terminalSignal.addEventListener('abort', this.abortFromTerminal, { once: true });
        if (timeoutMs !== undefined) {
            this.timer = setTimeout(() => this.cancel(new ClientTimeoutError(timeoutMs)), timeoutMs);
        }
        if (callerSignal?.aborted)
            this.abortFromCaller();
        else if (terminalSignal.aborted)
            this.abortFromTerminal();
    }
    race(operation) {
        return Promise.race([operation, this.cancellation]);
    }
    dispose() {
        if (!this.active)
            return;
        this.active = false;
        if (this.timer !== undefined)
            clearTimeout(this.timer);
        this.callerSignal?.removeEventListener('abort', this.abortFromCaller);
        this.terminalSignal.removeEventListener('abort', this.abortFromTerminal);
    }
    abortFromCaller = () => this.cancel(new ClientAbortError());
    abortFromTerminal = () => this.cancel(this.terminalError());
    cancel(error) {
        if (!this.active)
            return;
        this.dispose();
        this.controller.abort(error);
        this.rejectCancellation(error);
    }
}
//# sourceMappingURL=client.js.map