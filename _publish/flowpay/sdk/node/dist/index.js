export class FlowPayError extends Error {
    status;
    code;
    requestId;
    constructor(status, code, message, requestId) {
        super(message);
        this.status = status;
        this.code = code;
        this.requestId = requestId;
        this.name = "FlowPayError";
    }
}
export class FlowPay {
    payments;
    claims;
    webhooks;
    apiKey;
    baseUrl;
    fetcher;
    constructor(options) {
        if (!options.apiKey)
            throw new Error("FlowPay apiKey is required");
        this.apiKey = options.apiKey;
        this.baseUrl = (options.baseUrl ?? "http://127.0.0.1:8080").replace(/\/$/, "");
        this.fetcher = options.fetch ?? globalThis.fetch;
        this.payments = { create: (i, o) => this.request("POST", "/v1/payments", i, o?.idempotencyKey ?? crypto.randomUUID()), get: id => this.request("GET", `/v1/payments/${encodeURIComponent(id)}`), cancel: id => this.request("POST", `/v1/payments/${encodeURIComponent(id)}/cancel`), deposits: id => this.request("GET", `/v1/payments/${encodeURIComponent(id)}/deposits`) };
        this.claims = { create: (i, o) => this.request("POST", "/v1/claims", i, o?.idempotencyKey ?? crypto.randomUUID()), get: id => this.request("GET", `/v1/claims/${encodeURIComponent(id)}`), evidence: (id, input) => this.request("POST", `/v1/claims/${encodeURIComponent(id)}/evidence`, input), authorize: (id, signature) => this.request("POST", `/v1/claims/${encodeURIComponent(id)}/authorize`, { signature }), fund: id => this.request("POST", `/v1/claims/${encodeURIComponent(id)}/fund`), approve: id => this.request("POST", `/v1/claims/${encodeURIComponent(id)}/approve`) };
        this.webhooks = { list: () => this.request("GET", "/v1/webhooks"), create: (url, events) => this.request("POST", "/v1/webhooks", { url, events }), test: () => this.request("POST", "/v1/webhooks/test") };
    }
    async request(method, path, body, idempotencyKey) { const headers = { "x-flowpay-api-key": this.apiKey, "accept": "application/json" }; if (body !== undefined)
        headers["content-type"] = "application/json"; if (idempotencyKey)
        headers["idempotency-key"] = idempotencyKey; const response = await this.fetcher(this.baseUrl + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) }); const text = await response.text(); let parsed = {}; try {
        parsed = text ? JSON.parse(text) : {};
    }
    catch {
        parsed = { message: text };
    } if (!response.ok) {
        const e = parsed?.error ?? {};
        throw new FlowPayError(response.status, e.code ?? "http_error", e.message ?? `FlowPay request failed (${response.status})`, e.request_id);
    } return parsed; }
}
export function verifyWebhookSignature(rawBody, signatureHeader, secret, options = {}) {
    const parts = Object.fromEntries(signatureHeader.split(",").map(p => p.split("=", 2)));
    const ts = Number(parts.t);
    if (!Number.isFinite(ts) || !parts.v1)
        return Promise.resolve(false);
    const now = options.nowSeconds ?? Math.floor(Date.now() / 1000);
    if (Math.abs(now - ts) > (options.toleranceSeconds ?? 300))
        return Promise.resolve(false);
    const bytes = typeof rawBody === "string" ? new TextEncoder().encode(rawBody) : rawBody;
    const prefix = new TextEncoder().encode(`${ts}.`);
    const input = new Uint8Array(prefix.length + bytes.length);
    input.set(prefix);
    input.set(bytes, prefix.length);
    return crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]).then(async (key) => { const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, input)); const expected = Array.from(mac, b => b.toString(16).padStart(2, "0")).join(""); if (expected.length !== parts.v1.length)
        return false; let diff = 0; for (let i = 0; i < expected.length; i++)
        diff |= expected.charCodeAt(i) ^ parts.v1.charCodeAt(i); return diff === 0; });
}
