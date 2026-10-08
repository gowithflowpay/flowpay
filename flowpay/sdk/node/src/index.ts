export type Chain = "base" | "bsc" | "ethereum_sepolia" | "base_sepolia" | "arbitrum_sepolia" | "bsc_testnet" | "optimism_sepolia" | "polygon_amoy" | "monad_testnet" | `custom:${string}`;
export type PaymentStatus = "CREATED"|"WAITING"|"DETECTED"|"CONFIRMING"|"PARTIALLY_PAID"|"OVERPAID"|"WRONG_ASSET"|"WRONG_CHAIN_CLAIMED"|"CONFIRMED"|"SETTLING"|"COMPLETED"|"EXPIRED"|"FAILED"|"CLAIM_PENDING"|"RECOVERY_AVAILABLE"|"RECOVERY_PENDING"|"RECOVERED"|"ESCALATED"|"CANCELLED";
export interface Payment { id:string; address:string; amount:string; amount_atomic:string; asset:string; chain:Chain; status:PaymentStatus; expires_at:string; reference?:string|null; merchant_name?:string|null; checkout_url:string }
export interface CreatePaymentInput { amount:string; asset:string; chain:Chain; reference?:string; expires_in_seconds?:number; overpayment_policy?:"ACCEPT_AND_RECORD"|"REQUIRE_REVIEW"|"REJECT_SETTLEMENT" }
export interface CreateClaimInput { payment_id:string; transaction_hash?:string; actual_chain?:Chain; actual_asset?:string; originating_wallet?:string; recovery_destination:string; explanation:string }
export interface FlowPayOptions { apiKey?:string; sessionToken?:string; baseUrl?:string; fetch?:typeof globalThis.fetch }

/** States that end a `wait` as a success. */
export const SUCCESS_STATES: readonly PaymentStatus[] = ["COMPLETED","RECOVERED"];
/** States that end a `wait` with a recoverable (non-success) outcome. */
export const RECOVERABLE_STATES: readonly PaymentStatus[] = ["PARTIALLY_PAID","OVERPAID","WRONG_ASSET","WRONG_CHAIN_CLAIMED","CLAIM_PENDING","RECOVERY_AVAILABLE","RECOVERY_PENDING","ESCALATED"];
/** Terminal failure states for a `wait`. */
export const FAILURE_STATES: readonly PaymentStatus[] = ["EXPIRED","CANCELLED","FAILED"];

export function classify(status:PaymentStatus): "success"|"recoverable"|"failure"|"pending" {
  if (SUCCESS_STATES.includes(status)) return "success";
  if (RECOVERABLE_STATES.includes(status)) return "recoverable";
  if (FAILURE_STATES.includes(status)) return "failure";
  return "pending";
}

/** Wait options: poll interval, cap, and deadline. */
export interface WaitOptions { intervalSeconds?:number; maxIntervalSeconds?:number; timeoutSeconds?:number; signal?:AbortSignal; onStatus?:(p:Payment)=>void }
export interface WaitResult { payment:Payment; outcome:"success"|"recoverable"|"failure"|"timeout" }

export interface DeviceStart { device_code:string; user_code:string; expires_in:number; interval:number; verification_uri:string }
export type DevicePollResult = { status:"PENDING" } | { status:"APPROVED"; api_key:string; merchant_email?:string|null } | { status:"DENIED" };
export interface LinkedWallet { address:string; label?:string|null; linked_at?:number }

export class FlowPayError extends Error { constructor(public status:number, public code:string, message:string, public requestId?:string){super(message);this.name="FlowPayError";} }

export class FlowPay {
  readonly payments:{create:(i:CreatePaymentInput,o?:{idempotencyKey?:string})=>Promise<Payment>;get:(id:string)=>Promise<Payment>;cancel:(id:string)=>Promise<unknown>;deposits:(id:string)=>Promise<unknown>;wait:(id:string,o?:WaitOptions)=>Promise<WaitResult>};
  readonly claims:{create:(i:CreateClaimInput,o?:{idempotencyKey?:string})=>Promise<any>;get:(id:string)=>Promise<any>;evidence:(id:string,input:Record<string,unknown>)=>Promise<any>;authorize:(id:string,signature:string)=>Promise<any>;fund:(id:string)=>Promise<any>;approve:(id:string)=>Promise<any>};
  readonly webhooks:{list:()=>Promise<any>;create:(url:string,events?:string[])=>Promise<any>;test:()=>Promise<any>};
  readonly wallets:{list:()=>Promise<{data:LinkedWallet[]}>;challenge:(address:string)=>Promise<{nonce:string;message:string;expires_in:number}>;verify:(input:{address:string;nonce:string;signature:string;label?:string})=>Promise<{verified:boolean;address:string}>;unlink:(address:string)=>Promise<unknown>};
  readonly cli:{start:()=>Promise<DeviceStart>;poll:(deviceCode:string)=>Promise<DevicePollResult>};
  private readonly apiKey:string; private readonly sessionToken?:string; private readonly baseUrl:string; private readonly fetcher:typeof globalThis.fetch;
  constructor(options:FlowPayOptions){
    this.apiKey=options.apiKey??"";this.sessionToken=options.sessionToken;this.baseUrl=(options.baseUrl??"https://api.pixuno.xyz").replace(/\/$/,"");this.fetcher=options.fetch??globalThis.fetch;
    this.payments={
      create:(i,o)=>this.request("POST","/v1/payments",i,o?.idempotencyKey??crypto.randomUUID()),
      get:id=>this.request("GET",`/v1/payments/${encodeURIComponent(id)}`),
      cancel:id=>this.request("POST",`/v1/payments/${encodeURIComponent(id)}/cancel`),
      deposits:id=>this.request("GET",`/v1/payments/${encodeURIComponent(id)}/deposits`),
      wait:async(id,o={})=>{
        const interval=Math.max(1,o.intervalSeconds??3), maxInterval=Math.max(interval,o.maxIntervalSeconds??15), deadline=Date.now()+(o.timeoutSeconds??3600)*1000;
        let delay=interval;
        for(;;){
          if(o.signal?.aborted)throw new Error("wait aborted");
          let payment:Payment;
          try{ payment=await this.payments.get(id); delay=interval; }
          catch(e){
            // Auth failures are terminal; 404 stays terminal; anything else
            // (network blips, 5xx) retries with backoff until the deadline.
            if(e instanceof FlowPayError&&(e.status===404||e.status===401||e.status===403))throw e;
            if(Date.now()>=deadline){ return {payment:await this.payments.get(id).catch(()=>undefined as unknown as Payment),outcome:"timeout" as const}; }
            await sleep(Math.min(delay,maxInterval)); delay*=2; continue;
          }
          o.onStatus?.(payment);
          const kind=classify(payment.status);
          if(kind!=="pending")return {payment,outcome:kind};
          if(Date.now()>=deadline)return {payment,outcome:"timeout" as const};
          await sleep(Math.min(delay,maxInterval)); delay*=2;
        }
      },
    };
    this.claims={create:(i,o)=>this.request("POST","/v1/claims",i,o?.idempotencyKey??crypto.randomUUID()),get:id=>this.request("GET",`/v1/claims/${encodeURIComponent(id)}`),evidence:(id,input)=>this.request("POST",`/v1/claims/${encodeURIComponent(id)}/evidence`,input),authorize:(id,signature)=>this.request("POST",`/v1/claims/${encodeURIComponent(id)}/authorize`,{signature}),fund:id=>this.request("POST",`/v1/claims/${encodeURIComponent(id)}/fund`),approve:id=>this.request("POST",`/v1/claims/${encodeURIComponent(id)}/approve`)};
    this.webhooks={list:()=>this.request("GET","/v1/webhooks"),create:(url,events)=>this.request("POST","/v1/webhooks",{url,events}),test:()=>this.request("POST","/v1/webhooks/test")};
    this.wallets={
      list:()=>this.request("GET","/v1/wallets"),
      challenge:address=>this.request("POST","/v1/wallets/challenge",{address}),
      verify:input=>this.request("POST","/v1/wallets/verify",input),
      unlink:address=>this.request("POST","/v1/wallets/unlink",{address}),
    };
    this.cli={
      start:()=>this.request("POST","/v1/cli/device/start",{}),
      poll:deviceCode=>this.request("POST","/v1/cli/device/poll",{device_code:deviceCode}),
    };
  }
  private async request<T>(method:string,path:string,body?:unknown,idempotencyKey?:string):Promise<T>{const publicRoute=path==="/v1/cli/device/start"||path==="/v1/cli/device/poll";if(!publicRoute&&!this.apiKey&&!this.sessionToken)throw new Error("not authenticated; run `flowpay init` or `flowpay login`");const headers:Record<string,string>={"accept":"application/json"};if(this.sessionToken)headers.authorization=`Bearer ${this.sessionToken}`;else if(this.apiKey)headers["x-flowpay-api-key"]=this.apiKey;if(body!==undefined)headers["content-type"]="application/json";if(idempotencyKey)headers["idempotency-key"]=idempotencyKey;const response=await this.fetcher(this.baseUrl+path,{method,headers,body:body===undefined?undefined:JSON.stringify(body)});const text=await response.text();let parsed:any={};try{parsed=text?JSON.parse(text):{};}catch{parsed={message:text};}if(!response.ok){const e=parsed?.error??{};throw new FlowPayError(response.status,e.code??"http_error",e.message??`FlowPay request failed (${response.status})`,e.request_id);}return parsed as T;}
}

function sleep(ms:number){return new Promise<void>(r=>setTimeout(r,ms));}

export function verifyWebhookSignature(rawBody:string|Uint8Array,signatureHeader:string,secret:string,options:{nowSeconds?:number;toleranceSeconds?:number}={}):Promise<boolean>{
  const parts=Object.fromEntries(signatureHeader.split(",").map(p=>p.split("=",2) as [string,string]));const ts=Number(parts.t);if(!Number.isFinite(ts)||!parts.v1)return Promise.resolve(false);const now=options.nowSeconds??Math.floor(Date.now()/1000);if(Math.abs(now-ts)>(options.toleranceSeconds??300))return Promise.resolve(false);const bytes=typeof rawBody==="string"?new TextEncoder().encode(rawBody):rawBody;const prefix=new TextEncoder().encode(`${ts}.`);const input=new Uint8Array(prefix.length+bytes.length);input.set(prefix);input.set(bytes,prefix.length);return crypto.subtle.importKey("raw",new TextEncoder().encode(secret),{name:"HMAC",hash:"SHA-256"},false,["sign"]).then(async key=>{const mac=new Uint8Array(await crypto.subtle.sign("HMAC",key,input));const expected=Array.from(mac,b=>b.toString(16).padStart(2,"0")).join("");if(expected.length!==parts.v1.length)return false;let diff=0;for(let i=0;i<expected.length;i++)diff|=expected.charCodeAt(i)^parts.v1.charCodeAt(i);return diff===0;});
}
