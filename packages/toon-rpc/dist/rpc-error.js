export class RpcError extends Error {
    code;
    data;
    hasData;
    constructor(code, message, data) {
        super(message);
        this.name = 'RpcError';
        this.code = code;
        this.data = data;
        this.hasData = arguments.length >= 3;
    }
}
//# sourceMappingURL=rpc-error.js.map