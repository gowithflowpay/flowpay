import {generateKeyPairSync, createPrivateKey, createPublicKey, sign} from "node:crypto";
import {mkdir, readFile, writeFile, chmod} from "node:fs/promises";
import path from "node:path";
import {spawn} from "node:child_process";
import {configDir, AuthenticationError} from "./config.js";

async function windowsProtection(value: string, protect: boolean): Promise<string> {
  const script = `$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.Security; $v=[Console]::In.ReadToEnd(); $b=${protect ? '[Text.Encoding]::UTF8.GetBytes($v)' : '[Convert]::FromBase64String($v)'}; $r=[Security.Cryptography.ProtectedData]::${protect ? 'Protect' : 'Unprotect'}($b,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser); [Console]::Out.Write(${protect ? '[Convert]::ToBase64String($r)' : '[Text.Encoding]::UTF8.GetString($r)'})`;
  return new Promise((resolve, reject) => {
    const child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {windowsHide: true, stdio: ["pipe", "pipe", "pipe"]});
    let output = "";
    const timer = setTimeout(() => {child.kill(); reject(new Error("Device key protection timed out"));}, 15000);
    child.stdout.on("data", chunk => {output += chunk.toString();});
    child.stderr.resume();
    child.on("error", () => {clearTimeout(timer); reject(new Error("Windows device key protection is unavailable"));});
    child.on("close", code => {clearTimeout(timer); code === 0 ? resolve(output) : reject(new Error("Cannot unlock the device key with this Windows account"));});
    child.stdin.on("error", () => {});
    child.stdin.end(value);
  });
}

export function trustedOrigin(base: string): string {
  const url = new URL(base);
  if (url.username || url.password || (url.protocol !== "https:" && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)))) {
    throw new Error("Device authentication requires HTTPS (HTTP is allowed only on localhost)");
  }
  return url.origin;
}

export async function deviceKey(base: string, create = false) {
  const origin = trustedOrigin(base);
  const {createHash} = await import("node:crypto");
  const file = path.join(configDir(), `device-${createHash("sha256").update(origin).digest("hex")}.key`);
  let pem: string;
  try { pem = await readFile(file, "utf8"); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    if (!create) throw new AuthenticationError("No device key for this server. Run flowpay init on this device.");
    await mkdir(configDir(), {recursive: true, mode: 0o700});
    pem = generateKeyPairSync("ed25519").privateKey.export({format: "pem", type: "pkcs8"}).toString();
    const stored = process.platform === "win32" ? `DPAPI:${await windowsProtection(pem, true)}` : pem;
    try { await writeFile(file, stored, {flag: "wx", mode: 0o600}); }
    catch (writeError) {
      if ((writeError as NodeJS.ErrnoException).code !== "EEXIST") throw writeError;
      pem = await readFile(file, "utf8");
    }
    if (process.platform !== "win32") await chmod(file, 0o600);
  }
  if (pem.startsWith("DPAPI:")) pem = await windowsProtection(pem.slice(6), false);
  const privateKey = createPrivateKey(pem);
  const publicKey = createPublicKey(privateKey).export({format: "jwk"}).x!;
  return {publicKey, sign: (message: string) => sign(null, Buffer.from(message, "utf8"), privateKey).toString("base64url")};
}
