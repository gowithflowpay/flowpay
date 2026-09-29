export type Chain = "base" | "bsc";
export type PaymentStatus = "CREATED" | "WAITING" | "DETECTED" | "CONFIRMING" | "PARTIALLY_PAID" | "OVERPAID" | "WRONG_ASSET" | "WRONG_CHAIN_CLAIMED" | "CONFIRMED" | "SETTLING" | "COMPLETED" | "EXPIRED" | "FAILED" | "CLAIM_PENDING" | "RECOVERY_AVAILABLE" | "RECOVERY_PENDING" | "RECOVERED" | "ESCALATED" | "CANCELLED";
export interface Payment {
    id: string;
    address: string;
    amount: string;
    amount_atomic: string;
    asset: string;
    chain: Chain;
    status: PaymentStatus;
    expires_at: string;
    reference?: string | null;
    merchant_name?: string | null;
    checkout_url: string;
}
export interface CreatePaymentInput {
    amount: string;
    asset: string;
    chain: Chain;
    reference?: string;
    expires_in_seconds?: number;
    overpayment_policy?: "ACCEPT_AND_RECORD" | "REQUIRE_REVIEW" | "REJECT_SETTLEMENT";
}
export interface CreateClaimInput {
    payment_id: string;
    transaction_hash?: string;
    actual_chain?: Chain;
    actual_asset?: string;
    originating_wallet?: string;
    recovery_destination: string;
    explanation: string;
}
export interface FlowPayOptions {
    apiKey: string;
    baseUrl?: string;
    fetch?: typeof globalThis.fetch;
}
export declare class FlowPayError extends Error {
    status: number;
    code: string;
    requestId?: string | undefined;
    constructor(status: number, code: string, message: string, requestId?: string | undefined);
}
export declare class FlowPay {
    readonly payments: {
        create: (i: CreatePaymentInput, o?: {
            idempotencyKey?: string;
        }) => Promise<Payment>;
        get: (id: string) => Promise<Payment>;
        cancel: (id: string) => Promise<unknown>;
        deposits: (id: string) => Promise<unknown>;
    };
    readonly claims: {
        create: (i: CreateClaimInput, o?: {
            idempotencyKey?: string;
        }) => Promise<any>;
        get: (id: string) => Promise<any>;
        evidence: (id: string, input: Record<string, unknown>) => Promise<any>;
        authorize: (id: string, signature: string) => Promise<any>;
        fund: (id: string) => Promise<any>;
        approve: (id: string) => Promise<any>;
    };
    readonly webhooks: {
        list: () => Promise<any>;
        create: (url: string, events?: string[]) => Promise<any>;
        test: () => Promise<any>;
    };
    private readonly apiKey;
    private readonly baseUrl;
    private readonly fetcher;
    constructor(options: FlowPayOptions);
    private request;
}
export declare function verifyWebhookSignature(rawBody: string | Uint8Array, signatureHeader: string, secret: string, options?: {
    nowSeconds?: number;
    toleranceSeconds?: number;
}): Promise<boolean>;
