import {FlowPayError} from "@flowpay/node";
import {AuthenticationError} from "./config.js";

export const EXIT: Record<string, number> = {
  ok: 0,
  generic: 1,
  expired: 2,
  cancelled: 3,
  timeout: 4,
  recoverable: 5,
  auth: 6,
};

let jsonMode = false;

export function setJsonMode(value: boolean): void {
  jsonMode = value;
}

export function isJsonMode(): boolean {
  return jsonMode;
}

/** Emit a success payload. JSON mode: stdout gets compact JSON only. */
export function out(payload: unknown, human?: string): void {
  if (jsonMode) {
    process.stdout.write(`${JSON.stringify(payload)}\n`);
  } else if (human !== undefined) {
    process.stdout.write(`${human}\n`);
  } else {
    process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
  }
}

/** Progress/log lines always go to stderr so stdout stays parseable. */
export function log(message: string): void {
  process.stderr.write(`${message}\n`);
}

export function fail(error: unknown): never {
  let exitCode = EXIT.generic;
  let payload: Record<string, unknown>;
  if (error instanceof FlowPayError) {
    const status = error.status;
    if (status === 401 || status === 403) exitCode = EXIT.auth;
    else if (error.code === "payment_expired" || error.code === "expired") exitCode = EXIT.expired;
    else if (error.code === "payment_cancelled" || error.code === "cancelled") exitCode = EXIT.cancelled;
    payload = {error: {code: error.code, message: error.message, request_id: error.requestId}};
  } else if (error instanceof Error) {
    if (error instanceof AuthenticationError) exitCode = EXIT.auth;
    payload = {error: {code: "cli_error", message: error.message}};
  } else {
    payload = {error: {code: "cli_error", message: String(error)}};
  }
  if (jsonMode) {
    process.stdout.write(`${JSON.stringify(payload)}\n`);
  } else {
    const detail = (payload.error as {code: string; message: string});
    process.stderr.write(`error: ${detail.message} (${detail.code})\n`);
  }
  process.exit(exitCode);
}

/** Exit without emitting an error payload (used after a wait outcome). */
export function exitWith(code: number): never {
  process.exit(code);
}
