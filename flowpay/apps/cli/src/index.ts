#!/usr/bin/env node
// FlowPay CLI — request, receive, verify, and make payments from any
// terminal, script, bot, or agent. A thin client over @flowpay/node; the
// backend payment engine stays the single source of truth.

import {FlowPay, FlowPayError, classify, WaitResult, type Chain} from "@flowpay/node";
import {loadConfig, saveConfig, clearCredentials, credential, configDir} from "./config.js";
import {prompt} from "./prompt.js";
import {EXIT, setJsonMode, out, log, fail, exitWith} from "./output.js";

interface Parsed {
  args: string[];
  flags: Record<string, string | boolean>;
}

function parse(argv: string[]): Parsed {
  const args: string[] = [];
  const flags: Record<string, string | boolean> = {};
  for (let i = 0; i < argv.length; i++) {
    const token = argv[i];
    if (token.startsWith("--")) {
      const body = token.slice(2);
      const eq = body.indexOf("=");
      if (eq >= 0) {
        flags[body.slice(0, eq)] = body.slice(eq + 1);
      } else {
        const next = argv[i + 1];
        const valueFlags = new Set(["business-name", "contact-name", "settlement-address", "email", "code", "address", "description", "reference", "chain", "asset", "timeout", "interval", "label", "name", "scopes", "limit", "signer", "base-url", "expires-in"]);
        if (next !== undefined && !next.startsWith("--") && valueFlags.has(body)) {
          flags[body] = next;
          i++;
        } else {
          flags[body] = true;
        }
      }
    } else {
      args.push(token);
    }
  }
  return {args, flags};
}

function client(baseUrl?: string, keyOverride?: string): FlowPay {
  const config = loadConfigSync() as Record<string, unknown>;
  return new FlowPay({
    apiKey: keyOverride ?? (config.apiKey as string | undefined),
    sessionToken: !keyOverride && typeof config.sessionToken === "string" && (!config.sessionExpiresAt || Number(config.sessionExpiresAt) > Date.now()) ? config.sessionToken : undefined,
    baseUrl: baseUrl ?? process.env.FLOWPAY_API_URL ?? (config.baseUrl as string | undefined) ?? "https://api.pixuno.xyz",
  });
}

// loadConfig is async (fs promises) but commands are async too; provide a sync
// bridge by reading the file once at startup. Kept trivial and cached.
import {readFileSync, existsSync} from "node:fs";
import path from "node:path";
import os from "node:os";
let cached: ReturnType<typeof JSON.parse> | undefined;
function loadConfigSync(): Record<string, unknown> {
  if (cached !== undefined) return cached;
  const dir = process.env.FLOWPAY_CONFIG_DIR
    ? path.resolve(process.env.FLOWPAY_CONFIG_DIR)
    : path.join(os.homedir(), ".flowpay");
  const file = path.join(dir, "config.json");
  if (!existsSync(file)) {
    cached = {};
    return cached;
  }
  try {
    cached = JSON.parse(readFileSync(file, "utf8"));
  } catch {
    cached = {};
  }
  return cached;
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  const {args, flags} = parse(argv);
  if (flags.json) setJsonMode(true);
  if (flags.help) return printHelp();
  const config = loadConfigSync();

  const [command, subcommand, ...rest] = args;
  void rest;

  switch (command) {
    case "register":
      return await cmdRegister(flags);
    case "init":
      return await cmdInit(flags);
    case "login":
      return await cmdLogin(flags);
    case "account":
      if (subcommand === "status") return await cmdAccountStatus();
      break;
    case "logout":
      return await cmdLogout();
    case "wallet":
      return await cmdWallet(subcommand, flags);
    case "request":
      return await cmdRequest(args.slice(1), flags);
    case "payment":
      return await cmdPayment(subcommand, rest, flags);
    case "pay":
      return await cmdPay(subcommand, flags);
    case "events":
      return await cmdEvents();
    case "help":
    case undefined:
      return printHelp();
    default:
      break;
  }
  out({error: {code: "unknown_command", message: `unknown command: ${command ?? "(none)"}; see --help`}});
  exitWith(EXIT.generic);
}

function printHelp(): void {
  const text = `FlowPay CLI — crypto payments from any terminal, script, or agent

Usage: flowpay <command> [subcommand] [arguments] [--json]

Setup:
  flowpay register                   Register interactively and verify your email
  flowpay login                      Sign in with your email and a one-time code
  flowpay init                       Link this device to your FlowPay account (device flow)
  flowpay account status             Show the current account and credential
  flowpay logout                     Forget local credentials

Wallet:
  flowpay wallet connect             Start wallet linking (prints a sign-in challenge)
  flowpay wallet status              Show linked wallets
  flowpay wallet disconnect --address 0x..  Unlink a wallet

Payments:
  flowpay request 20 usdc eth         Create checkout, print address and wait for payment
  flowpay request 20 USDC base        Choose a network explicitly
  flowpay payment status <payment_id>
  flowpay payment wait <payment_id> [--timeout 30m] [--json]
  flowpay payment cancel <payment_id>
  flowpay pay <payment_id>           Sign an authorization; transfer through your wallet
  flowpay events                     Stream payment events (poll feed)

Automation:
  --json                             stdout carries machine-readable JSON only; logs go to stderr
Exit codes:
  0 success   1 failure   2 expired   3 cancelled   4 timeout   5 recoverable   6 auth/config

Environment:
  FLOWPAY_API_URL      Backend base URL (default https://api.pixuno.xyz)
  FLOWPAY_CONFIG_DIR   Override config directory (default ~/.flowpay)
`;
  process.stdout.write(`${text}\n`);
}

function parseTimeout(value: unknown): number | undefined {
  if (typeof value !== "string" || !value) return undefined;
  const match = value.match(/^(\d+)(s|m|h|d)?$/);
  if (!match) throw new Error(`invalid timeout: ${value}`);
  const unit = match[2] ?? "s";
  const factor = unit === "s" ? 1 : unit === "m" ? 60 : unit === "h" ? 3600 : 86400;
  return Number(match[1]) * factor;
}

function mapWaitOutcome(wait: WaitResult): number {
  if (wait.outcome === "success") return EXIT.ok;
  if (wait.outcome === "recoverable") return EXIT.recoverable;
  if (wait.outcome === "timeout") return EXIT.timeout;
  return EXIT.generic;
}

function paymentView(p: Awaited<ReturnType<FlowPay["payments"]["get"]>>): Record<string, unknown> {
  return {
    payment_id: p.id,
    status: p.status,
    amount: p.amount,
    asset: p.asset,
    chain: p.chain,
    payment_address: p.address,
    checkout_url: p.checkout_url,
    expires_at: p.expires_at,
    ...(p.reference !== undefined ? {reference: p.reference} : {}),
    ...(p.merchant_name ? {merchant_name: p.merchant_name} : {}),
  };
}

async function cmdInit(flags: Record<string, string | boolean>): Promise<void> {
  const api = client(typeof flags["base-url"] === "string" ? flags["base-url"] : undefined);
  const grant = await api.cli.start();
  log("Open this URL and approve the device:");
  log(`  ${grant.verification_uri}`);
  log(`Code: ${grant.user_code}`);
  log("Waiting for approval… (Ctrl+C to cancel)");
  const deadline = Date.now() + grant.expires_in * 1000;
  let intervalMs = grant.interval * 1000;
  for (;;) {
    if (Date.now() >= deadline) {
      fail(new Error("device grant expired; run `flowpay init` again"));
    }
    await new Promise(r => setTimeout(r, intervalMs));
    let result;
    try {
      result = await api.cli.poll(grant.device_code);
    } catch (e) {
      if (e instanceof FlowPayError && e.code === "authorization_pending") {
        continue;
      }
      if (e instanceof FlowPayError && e.code === "slow_down") {
        intervalMs *= 2;
        continue;
      }
      throw e;
    }
    if (result.status === "APPROVED") {
      await saveConfig({apiKey: result.api_key, merchantEmail: result.merchant_email ?? undefined, sessionToken: undefined, sessionExpiresAt: undefined, baseUrl: baseUrl(flags)});
      if (typeof flags.json === "boolean" && flags.json) {
        out({initialized: true, merchant_email: result.merchant_email ?? null, verification_uri: grant.verification_uri});
      } else {
        log("✓ FlowPay account initialized");
        log("✓ Credentials configured");
        log("✓ Ready to accept payments");
      }
      return;
    }
    if (result.status === "DENIED") {
      fail(new Error("device grant was denied"));
    }
  }
}

function baseUrl(flags: Record<string, string | boolean>): string {
  return (typeof flags["base-url"] === "string" ? flags["base-url"] : process.env.FLOWPAY_API_URL ?? String(loadConfigSync().baseUrl ?? "https://api.pixuno.xyz")).replace(/\/$/, "");
}

async function authRequest(base: string, action: string, payload: unknown, token?: string): Promise<any> {
  const response = await fetch(`${base}/v1/auth/${action}`, {
    method: "POST", headers: {"content-type": "application/json", ...(token ? {authorization: `Bearer ${token}`} : {})}, body: JSON.stringify(payload),
    signal: AbortSignal.timeout(30000),
  });
  const body = await response.json().catch(() => ({})) as any;
  if (!response.ok) throw new Error(body?.error?.message ?? `Login failed (${response.status})`);
  return body;
}

async function cmdLogin(flags: Record<string, string | boolean>): Promise<void> {
  if(flags.password)throw new Error("FlowPay CLI uses email codes. Run flowpay login.");
  const email = (typeof flags.email === "string" ? flags.email : await prompt("Email: ")).trim().toLowerCase();
  if (!email) throw new Error("email is required");
  const base = baseUrl(flags);
  let code = typeof flags.code === "string" ? flags.code.trim() : "";
  if (!code) {
    if (!process.stdin.isTTY) throw new Error("Email login requires a terminal; use --code with an existing email code");
    const sent = await authRequest(base, "resend", {email, purpose: "LOGIN"});
    if (sent.sent === false) throw new Error("The email code could not be delivered");
    log(`Check ${email} for your sign-in code.`);
    code = (await prompt("Email code: ", true)).trim();
  }
  if (!/^\d{6}$/.test(code)) throw new Error("Enter the six-digit email code");
  const body = await authRequest(base, "verify", {email, code});
  if (typeof body.session_token !== "string" || !body.session_token) throw new Error("FlowPay did not return a session");
  await saveConfig({apiKey: undefined, sessionToken: body.session_token, sessionExpiresAt: body.expires_at, merchantEmail: email, baseUrl: base});
  out({logged_in: true, merchant_email: email}, `Signed in as ${email}`);
}

async function cmdRegister(flags: Record<string, string | boolean>): Promise<void> {
  const field=async(name:string,label:string)=>typeof flags[name]==="string"?String(flags[name]).trim():(await prompt(label)).trim();
  const businessName=await field("business-name","Business name: ");
  const contactName=await field("contact-name","Your name: ");
  const email=(await field("email","Email: ")).toLowerCase();
  const settlementAddress=await field("settlement-address","Settlement wallet (0x...): ");
  if(!businessName||!email)throw new Error("Business name and email are required");
  if(!/^0x[a-fA-F0-9]{40}$/.test(settlementAddress))throw new Error("Enter a valid EVM settlement wallet address");
  const base=baseUrl(flags);
  const created=await authRequest(base,"signup",{business_name:businessName,contact_name:contactName||undefined,email});
  if(created.email_sent===false)throw new Error("Your account was created but email delivery failed. Run flowpay login to request another code.");
  log(`Check ${email} for your verification code.`);
  const code=(typeof flags.code==="string"?flags.code:await prompt("Email code: ",true)).trim();
  if(!/^\d{6}$/.test(code))throw new Error("Enter the six-digit email code");
  const verified=await authRequest(base,"verify",{email,code});
  if(typeof verified.session_token!=="string"||!verified.session_token)throw new Error("FlowPay did not return a session");
  await saveConfig({apiKey:undefined,sessionToken:verified.session_token,sessionExpiresAt:verified.expires_at,merchantEmail:email,baseUrl:base});
  await authRequest(base,"onboarding",{business_name:businessName,contact_name:contactName||undefined,evm_settlement_address:settlementAddress},verified.session_token);
  out({registered:true,merchant_email:email},`Account registered and email verified.\nSigned in as ${email}\nReady: flowpay request 20 usdc eth`);
}

async function cmdAccountStatus(): Promise<void> {
  const config = loadConfigSync() as Record<string, unknown>;
  const hasKey = Boolean(config.apiKey ?? config.sessionToken);
  if (!hasKey) {
    fail(new Error("not authenticated; run `flowpay init` or `flowpay login`"));
  }
  const api = client();
  try {
    const wallets = await api.wallets.list();
    const payload = {
      authenticated: true,
      merchant_email: (config.merchantEmail as string | undefined) ?? null,
      base_url: (config.baseUrl as string | undefined) ?? process.env.FLOWPAY_API_URL ?? "https://api.pixuno.xyz",
      wallets: wallets.data.map(w => ({address: w.address, label: w.label ?? null})),
    };
    out(payload, humanAccountStatus(payload));
  } catch (e) {
    if (e instanceof FlowPayError && e.status === 401) {
      fail(new Error("credential rejected by the API; run `flowpay init` or `flowpay login` again"));
    }
    throw e;
  }
}

function humanAccountStatus(payload: {merchant_email: string | null; base_url: string; wallets: {address: string; label: string | null}[]}): string {
  const wallets = payload.wallets.length
    ? payload.wallets.map(w => `  ${w.address}${w.label ? ` (${w.label})` : ""}`).join("\n")
    : "  (none — run `flowpay wallet connect`)";
  return `Account: ${payload.merchant_email ?? "(unknown)"}
API: ${payload.base_url}
Wallets:
${wallets}`;
}

async function cmdLogout(): Promise<void> {
  await clearCredentials();
  log("Credentials removed.");
}

async function cmdWallet(subcommand: string | undefined, flags: Record<string, string | boolean>): Promise<void> {
  const api = client();
  switch (subcommand) {
    case "connect": {
      const address = typeof flags.address === "string" ? flags.address : undefined;
      if (!address) fail(new Error("usage: flowpay wallet connect --address 0x…"));
      const challenge = await api.wallets.challenge(address);
      log("Sign this message with your wallet, then run:");
      log(`  flowpay wallet verify --address ${address} --nonce ${challenge.nonce} --signature 0x…`);
      if (flags.json) out({address, nonce: challenge.nonce, message: challenge.message, expires_in: challenge.expires_in});
      return;
    }
    case "verify": {
      const address = typeof flags.address === "string" ? flags.address : undefined;
      const nonce = typeof flags.nonce === "string" ? flags.nonce : undefined;
      const signature = typeof flags.signature === "string" ? flags.signature : undefined;
      if (!address || !nonce || !signature) fail(new Error("usage: flowpay wallet verify --address 0x… --nonce … --signature 0x…"));
      const result = await api.wallets.verify({address, nonce, signature, label: typeof flags.label === "string" ? flags.label : undefined});
      await saveConfig({linkedWallet: result.address});
      out({linked: true, address: result.address}, `✓ Wallet linked: ${result.address}`);
      return;
    }
    case "status": {
      const wallets = await api.wallets.list();
      out({wallets: wallets.data}, wallets.data.length ? wallets.data.map(w => `  ${w.address}${w.label ? ` (${w.label})` : ""}`).join("\n") : "No linked wallets.");
      return;
    }
    case "disconnect": {
      const address = typeof flags.address === "string" ? flags.address : undefined;
      if (!address) fail(new Error("usage: flowpay wallet disconnect --address 0x…"));
      await api.wallets.unlink(address);
      out({unlinked: true, address}, `✓ Wallet unlinked: ${address}`);
      return;
    }
    default:
      fail(new Error("usage: flowpay wallet <connect|verify|status|disconnect>"));
  }
}

async function cmdRequest(args: string[], flags: Record<string, string | boolean>): Promise<void> {
  const [amount, asset, network] = args;
  if (!amount || !asset) fail(new Error("usage: flowpay request <amount> <asset> <network>"));
  const requestedChain = (typeof flags.chain === "string" ? flags.chain : network ?? "base").toLowerCase();
  const chain = ({eth:"ethereum_sepolia",ethereum:"ethereum_sepolia",bnb:"bsc",sepolia:"ethereum_sepolia"} as Record<string,string>)[requestedChain] ?? requestedChain;
  if(!/^\d+(\.\d+)?$/.test(amount)||Number(amount)<=0)throw new Error("Payment amount must be greater than zero");
  const api = client();
  const payment = await api.payments.create(
    {
      amount,
      asset: asset.toUpperCase(),
      chain: chain as Chain,
      reference: typeof flags.description === "string" ? flags.description : typeof flags.reference === "string" ? flags.reference : undefined,
      expires_in_seconds: typeof flags["expires-in"] === "string" ? Number(flags["expires-in"]) : undefined,
    },
  );
  const view = paymentView(payment);
  out(view, humanPayment(payment));
  if (!flags["no-wait"] && !flags.json) {
    log("\nWaiting for payment… (Ctrl+C to stop; the payment stays live)");
    let previousStatus="";
    const wait = await api.payments.wait(payment.id, {timeoutSeconds: parseTimeout(flags.timeout) ?? 1800, intervalSeconds:typeof flags.interval==="string"?Number(flags.interval):3, onStatus: p => {if(p.status!==previousStatus){log(`  ${p.status}`);previousStatus=p.status;}}});
    if(wait.outcome==="success")log(`\nPayment received and confirmed.\n${payment.amount} ${payment.asset} | ${payment.id}`);
    else if(wait.outcome==="recoverable")log(`\nPayment needs attention: ${wait.payment.status}\nReview ${payment.checkout_url}`);
    else if(wait.outcome==="timeout")log(`\nStill awaiting confirmation. Resume: flowpay payment wait ${payment.id}`);
    else log(`\nPayment ended: ${wait.payment.status}`);
    exitWith(mapWaitOutcome(wait));
  }
  if (flags["no-wait"] && !flags.json) exitWith(EXIT.ok);
}

function humanPayment(p: Awaited<ReturnType<FlowPay["payments"]["get"]>>): string {
  return `Payment created
Amount: ${p.amount} ${p.asset}
Network: ${p.chain}
Payment ID: ${p.id}
Wallet address (copy and share):
${p.address}
Checkout:
${p.checkout_url}`;
}

async function cmdPayment(subcommand: string | undefined, rest: string[], flags: Record<string, string | boolean>): Promise<void> {
  const api = client();
  const id = rest[0];
  switch (subcommand) {
    case "status": {
      if (!id) fail(new Error("usage: flowpay payment status <payment_id>"));
      const payment = await api.payments.get(id);
      out(paymentView(payment), humanStatus(payment));
      return;
    }
    case "wait": {
      if (!id) fail(new Error("usage: flowpay payment wait <payment_id> [--timeout 30m]"));
      const lastStatus: {value?: string} = {};
      const wait = await api.payments.wait(id, {
        timeoutSeconds: parseTimeout(flags.timeout) ?? 3600,
        intervalSeconds: typeof flags.interval === "string" ? Number(flags.interval) : 3,
        onStatus: p => {
          if (lastStatus.value !== p.status) {
            lastStatus.value = p.status;
            if (!flags.json) log(`  status: ${p.status}`);
            else out({payment_id: p.id, status: p.status});
          }
        },
      });
      if (wait.payment) {
        out(paymentView(wait.payment), humanStatus(wait.payment));
      } else if (flags.json) {
        out({payment_id: id, status: lastStatus.value ?? "UNKNOWN", outcome: wait.outcome});
      } else {
        log(`Timed out waiting for ${id}${lastStatus.value ? ` (last status: ${lastStatus.value})` : " — payment state could not be confirmed"}.`);
      }
      exitWith(mapWaitOutcome(wait));
      return;
    }
    case "cancel": {
      if (!id) fail(new Error("usage: flowpay payment cancel <payment_id>"));
      await api.payments.cancel(id);
      const payment = await api.payments.get(id);
      out(paymentView(payment), `✓ Payment cancelled: ${payment.id}`);
      exitWith(EXIT.cancelled);
      return;
    }
    default:
      fail(new Error("usage: flowpay payment <status|wait|cancel> <payment_id>"));
  }
}

function humanStatus(p: Awaited<ReturnType<FlowPay["payments"]["get"]>>): string {
  return `Payment ${p.id}
Status: ${p.status}
Amount: ${p.amount} ${p.asset} on ${p.chain}
Checkout: ${p.checkout_url}`;
}

async function cmdPay(id: string | undefined, flags: Record<string, string | boolean>): Promise<void> {
  if (!id) fail(new Error("usage: flowpay pay <payment_id>"));
  const config = loadConfigSync() as Record<string, unknown>;
  const signerCommand = typeof flags.signer === "string" ? flags.signer : (config.signerCommand as string | undefined);
  if (!signerCommand) {
    fail(new Error("no signer configured. FlowPay never stores private keys; set `signerCommand` in ~/.flowpay/config.json to a command that reads the EIP-191 message on stdin and prints a 0x… signature, or use `flowpay payment wait` and pay from an external wallet."));
  }
  const api = client();
  const payment = await api.payments.get(id);
  const limit = typeof flags.limit === "string" ? flags.limit : (config.payLimit as string | undefined);
  if (limit && Number(payment.amount) > Number(limit)) {
    fail(new Error(`payment amount ${payment.amount} exceeds spending limit ${limit}`));
  }
  if (classify(payment.status) === "success") {
    out(paymentView(payment), `Payment already completed: ${payment.id}`);
    return;
  }
  const {execFileSync} = await import("node:child_process");
  const message = `FlowPay payment authorization\nPayment: ${payment.id}\nAmount: ${payment.amount} ${payment.asset}\nChain: ${payment.chain}\nAddress: ${payment.address}`;
  let signature: string;
  try {
    signature = execFileSync(signerCommand, {input: message, encoding: "utf8"}).trim();
  } catch (e) {
    fail(new Error(`signer command failed: ${e instanceof Error ? e.message : String(e)}`));
  }
  if (!signature.startsWith("0x")) fail(new Error("signer did not return a 0x… signature"));
  log("Signature received. Settlement via on-chain transfer is not enabled for autonomous agents in this build; presenting the signed authorization.");
  out({...paymentView(payment), authorization: {message, signature}}, "Authorization recorded.");
}

async function cmdEvents(): Promise<void> {
  const api = client();
  const payments = await (api as unknown as {request:(m:string,p:string)=>Promise<any>}).request("GET", "/v1/payments?limit=25");
  const rows = (payments?.data ?? []) as Array<{id: string; status: string; amount: string; asset: string; chain: string}>;
  if (!rows.length) {
    out({data: []}, "No recent payments.");
    return;
  }
  out({data: rows.map(r => ({payment_id: r.id, status: r.status, amount: r.amount, asset: r.asset, chain: r.chain}))},
    rows.map(r => `  ${r.id}  ${r.status.padEnd(12)} ${r.amount} ${r.asset} (${r.chain})`).join("\n"));
}

main().catch(e => fail(e));
