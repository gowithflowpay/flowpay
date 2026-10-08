import {mkdir, readFile, writeFile, chmod, unlink} from "node:fs/promises";
import {existsSync} from "node:fs";
import path from "node:path";
import os from "node:os";

export interface FlowPayConfig {
  baseUrl?: string;
  checkoutUrl?: string;
  apiKey?: string;
  sessionToken?: string;
  sessionExpiresAt?: number;
  merchantEmail?: string;
  recoveryEmail?: string;
  settlementAddress?: string;
  pendingRegistration?: {businessName: string; contactName: string; email: string; settlementAddress: string; baseUrl: string};
  linkedWallet?: string;
  /** Optional external signing command; CLI never stores wallet private keys. */
  signerCommand?: string;
  /** Optional spending policy for `flowpay pay`. */
  payLimit?: string;
}

export class AuthenticationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AuthenticationError";
  }
}

const DIR = process.env.FLOWPAY_CONFIG_DIR
  ? path.resolve(process.env.FLOWPAY_CONFIG_DIR)
  : path.join(os.homedir(), ".flowpay");
const FILE = path.join(DIR, "config.json");

export function configDir(): string {
  return DIR;
}

export function configPath(): string {
  return FILE;
}

export async function loadConfig(): Promise<FlowPayConfig> {
  if (!existsSync(FILE)) return {};
  try {
    const raw = await readFile(FILE, "utf8");
    const parsed = JSON.parse(raw) as FlowPayConfig;
    if (typeof parsed !== "object" || parsed === null) return {};
    return parsed;
  } catch {
    return {};
  }
}

export async function saveConfig(update: Partial<FlowPayConfig>): Promise<FlowPayConfig> {
  await mkdir(DIR, {recursive: true});
  const next = {...(await loadConfig()), ...update};
  for (const key of Object.keys(next) as (keyof FlowPayConfig)[]) {
    if (next[key] === undefined) delete next[key];
  }
  await writeFile(FILE, JSON.stringify(next, null, 2), {mode: 0o600});
  if (process.platform !== "win32") {
    await chmod(FILE, 0o600).catch(() => {});
  }
  return next;
}

export async function clearCredentials(): Promise<void> {
  await saveConfig({apiKey: undefined, sessionToken: undefined, sessionExpiresAt: undefined, merchantEmail: undefined});
  if (!existsSync(FILE)) await unlink(FILE).catch(() => {});
}

export function credential(config: FlowPayConfig): string {
  const token = config.sessionToken;
  const key = config.apiKey;
  if (token && (!config.sessionExpiresAt || config.sessionExpiresAt > Date.now())) return token;
  if (key) return key;
  throw new AuthenticationError("not authenticated; run `flowpay init` or `flowpay login`");
}
