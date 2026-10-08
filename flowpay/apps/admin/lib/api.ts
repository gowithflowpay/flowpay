import "server-only";
import path from "node:path";
import { loadEnvConfig } from "@next/env";

// Route handlers run in their own runtime and do not inherit variables loaded
// while Next evaluates next.config.mjs.
loadEnvConfig(path.resolve(process.cwd(), "../.."));

export async function adminApi<T>(pathname: string): Promise<T> {
  const base = (process.env.FLOWPAY_API_URL ?? "http://127.0.0.1:8080").replace(/\/$/, "");
  const key = process.env.FLOWPAY_ADMIN_KEY;
  if (!key) throw new Error("FLOWPAY_ADMIN_KEY is required on the admin server");
  const response = await fetch(base + pathname, {
    cache: "no-store",
    headers: {
      "x-flowpay-admin-key": key,
      accept: "application/json",
    },
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value?.error?.message ?? "FlowPay admin request failed");
  return value as T;
}
