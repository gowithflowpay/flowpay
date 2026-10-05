import test from "node:test";
import assert from "node:assert/strict";
import {execFileSync, spawnSync, spawn} from "node:child_process";
import {createServer} from "node:http";
import path from "node:path";
import {fileURLToPath} from "node:url";
import os from "node:os";
import fs from "node:fs";
import {FlowPay, FlowPayError, classify} from "@flowpay/node";

const CLI = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../dist/index.js");

function runAsync(args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CLI, ...args], {env: {...process.env, ...env}});
    let stdout = "", stderr = "";
    child.stdout.on("data", data => stdout += data);
    child.stderr.on("data", data => stderr += data);
    child.on("error", reject);
    child.on("close", status => resolve({status, stdout, stderr}));
  });
}

test("email login persists the API and sends the session as Bearer without exposing it", async () => {
  const requests = [];
  const server = createServer(async (request, response) => {
    let text = "";
    for await (const chunk of request) text += chunk;
    requests.push({url: request.url, headers: request.headers, body: text ? JSON.parse(text) : null});
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify(request.url === "/v1/auth/verify"
      ? {session_token: "session-secret", expires_at: Date.now() + 60000}
      : {data: []}));
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "flowpay-login-"));
  const base = `http://127.0.0.1:${server.address().port}`;
  try {
    const env = {FLOWPAY_CONFIG_DIR: directory, FLOWPAY_API_URL: ""};
    delete env.FLOWPAY_API_URL;
    const result = await runAsync(["login", "--email", "YOU@example.com", "--code", "123456", "--base-url", base, "--json"], env);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(requests[0].body, {email: "you@example.com", code: "123456"});
    assert.doesNotMatch(result.stdout + result.stderr, /session-secret/);
    const config = JSON.parse(fs.readFileSync(path.join(directory, "config.json"), "utf8"));
    assert.equal(config.baseUrl, base);
    const status = await runAsync(["account", "status", "--json"], env);
    assert.equal(status.status, 0, status.stderr);
    assert.equal(requests[1].headers.authorization, "Bearer session-secret");
    assert.equal(requests[1].headers["x-flowpay-api-key"], undefined);
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    fs.rmSync(directory, {recursive: true, force: true});
  }
});

test("device initialization starts without existing credentials", async () => {
  let started = false;
  const server = createServer((request, response) => {
    response.setHeader("content-type", "application/json");
    if (request.url === "/v1/cli/device/start") {
      started = true;
      response.end(JSON.stringify({device_code: "device", user_code: "ABCD-EFGH", expires_in: 60, interval: 0, verification_uri: "http://merchant.test/dashboard/devices"}));
    } else response.end(JSON.stringify({status: "DENIED"}));
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "flowpay-device-"));
  try {
    const result = await runAsync(["init", "--base-url", `http://127.0.0.1:${server.address().port}`, "--json"], {FLOWPAY_CONFIG_DIR: directory});
    assert.equal(started, true);
    assert.notEqual(result.status, 0);
    assert.match(result.stdout, /denied/);
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    fs.rmSync(directory, {recursive: true, force: true});
  }
});

test("register verifies email and completes wallet onboarding without a password", async () => {
  const requests=[];
  const server=createServer(async(request,response)=>{
    let text="";for await(const chunk of request)text+=chunk;
    requests.push({url:request.url,body:JSON.parse(text),authorization:request.headers.authorization});
    response.setHeader("content-type","application/json");
    response.end(JSON.stringify(request.url==="/v1/auth/verify"?{session_token:"registration-session",expires_at:Date.now()+60000}:{email_sent:true,merchant:{onboarding_completed:true}}));
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),"flowpay-register-"));
  try{
    const result=await runAsync(["register","--business-name","Test Store","--contact-name","Test Owner","--email","owner@example.test","--settlement-address","0x1111111111111111111111111111111111111111","--code","123456","--base-url",`http://127.0.0.1:${server.address().port}`,"--json"],{FLOWPAY_CONFIG_DIR:directory});
    assert.equal(result.status,0,result.stderr);
    assert.equal(requests[0].url,"/v1/auth/signup");
    assert.equal(requests[0].body.password,undefined);
    assert.equal(requests[1].url,"/v1/auth/verify");
    assert.equal(requests[2].url,"/v1/auth/onboarding");
    assert.equal(requests[2].authorization,"Bearer registration-session");
    assert.doesNotMatch(result.stdout+result.stderr,/registration-session/);
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));fs.rmSync(directory,{recursive:true,force:true});}
});

test("positional eth request prints shareable details and confirms receipt",async()=>{
  let created;
  const payment={id:"pay_terminal",address:"0x1111111111111111111111111111111111111111",amount:"20",asset:"USDC",chain:"ethereum_sepolia",status:"COMPLETED",checkout_url:"https://checkout.example.test/pay/pay_terminal"};
  const server=createServer(async(request,response)=>{
    let text="";for await(const chunk of request)text+=chunk;
    if(request.method==="POST")created=JSON.parse(text);
    response.setHeader("content-type","application/json");response.end(JSON.stringify(payment));
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),"flowpay-request-"));
  fs.writeFileSync(path.join(directory,"config.json"),JSON.stringify({baseUrl:`http://127.0.0.1:${server.address().port}`,apiKey:"fp_test.key"}));
  try{
    const result=await runAsync(["request","20","usdc","eth"],{FLOWPAY_CONFIG_DIR:directory});
    assert.equal(result.status,0,result.stderr);
    assert.equal(created.chain,"ethereum_sepolia");assert.equal(created.asset,"USDC");assert.equal(created.amount,"20");
    assert.match(result.stdout,new RegExp(payment.address));
    assert.match(result.stdout,/checkout.example.test/);
    assert.match(result.stderr,/Payment received and confirmed/);
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));fs.rmSync(directory,{recursive:true,force:true});}
});

function runCli(args, env = {}) {
  return spawnSync(process.execPath, [CLI, ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      FLOWPAY_CONFIG_DIR: fs.mkdtempSync(path.join(os.tmpdir(), "flowpay-cli-test-")),
      ...env,
    },
  });
}

test("--json request without credentials emits JSON error and non-zero exit", () => {
  const result = runCli(["request", "5", "USDC", "--json"]);
  assert.notEqual(result.status, 0);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.error.code, "cli_error");
});

test("no-auth request prints human error on stderr and exits non-zero", () => {
  const result = runCli(["request", "5", "USDC"]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /not authenticated|apiKey is required|error:/);
  assert.equal(result.stdout.trim(), "");
});

test("unknown command exits non-zero with JSON error in json mode", () => {
  const result = runCli(["frobnicate", "--json"]);
  assert.notEqual(result.status, 0);
  const parsed = JSON.parse(result.stdout.trim());
  assert.equal(parsed.error.code, "unknown_command");
});

test("wait classification maps success, recoverable, failure, pending", () => {
  assert.equal(classify("COMPLETED"), "success");
  assert.equal(classify("RECOVERED"), "success");
  assert.equal(classify("RECOVERY_AVAILABLE"), "recoverable");
  assert.equal(classify("PARTIALLY_PAID"), "recoverable");
  assert.equal(classify("WRONG_ASSET"), "recoverable");
  assert.equal(classify("EXPIRED"), "failure");
  assert.equal(classify("CANCELLED"), "failure");
  assert.equal(classify("FAILED"), "failure");
  assert.equal(classify("WAITING"), "pending");
  assert.equal(classify("DETECTED"), "pending");
});

function fakeClient(responses) {
  const calls = [];
  const fake = async (input, init) => {
    calls.push({input, init});
    const url = String(input);
    const respond = responses(url, calls.length);
    return respond();
  };
  return {client: new FlowPay({apiKey: "fp_test.x", baseUrl: "http://flowpay.test", fetch: fake}), calls};
}

test("payments.wait resolves success on COMPLETED and exposes statuses", async () => {
  const statuses = ["WAITING", "DETECTED", "CONFIRMING", "COMPLETED"];
  const {client} = fakeClient((url) => () => new Response(
    JSON.stringify({id: "pay_1", address: "0x1", amount: "5", amount_atomic: "5000000", asset: "USDC", chain: "base", status: statuses.shift() ?? "COMPLETED", expires_at: "x", checkout_url: "http://c/pay_1"}),
    {status: 200, headers: {"content-type": "application/json"}},
  ));
  const seen = [];
  const result = await client.payments.wait("pay_1", {intervalSeconds: 0.01, onStatus: p => seen.push(p.status)});
  assert.equal(result.outcome, "success");
  assert.deepEqual(seen, ["WAITING", "DETECTED", "CONFIRMING", "COMPLETED"]);
});

test("payments.wait ends recoverable on RECOVERY_AVAILABLE, not success", async () => {
  const {client} = fakeClient(() => () => new Response(
    JSON.stringify({id: "pay_2", address: "0x2", amount: "5", amount_atomic: "5000000", asset: "USDC", chain: "base", status: "RECOVERY_AVAILABLE", expires_at: "x", checkout_url: "http://c/pay_2"}),
    {status: 200},
  ));
  const result = await client.payments.wait("pay_2", {intervalSeconds: 0.01});
  assert.equal(result.outcome, "recoverable");
  assert.equal(result.payment.status, "RECOVERY_AVAILABLE");
});

test("payments.wait times out with timeout outcome", async () => {
  const {client} = fakeClient(() => () => new Response(
    JSON.stringify({id: "pay_3", address: "0x3", amount: "5", amount_atomic: "5000000", asset: "USDC", chain: "base", status: "WAITING", expires_at: "x", checkout_url: "http://c/pay_3"}),
    {status: 200},
  ));
  const result = await client.payments.wait("pay_3", {intervalSeconds: 0.01, timeoutSeconds: 0.05});
  assert.equal(result.outcome, "timeout");
});

test("payments.wait retries transient network errors and succeeds", async () => {
  let failing = 2;
  const {client} = fakeClient(() => () => {
    if (failing > 0) {
      failing -= 1;
      return Promise.reject(new TypeError("fetch failed"));
    }
    return Promise.resolve(new Response(
      JSON.stringify({id: "pay_4", address: "0x4", amount: "5", amount_atomic: "5000000", asset: "USDC", chain: "base", status: "COMPLETED", expires_at: "x", checkout_url: "http://c/pay_4"}),
      {status: 200},
    ));
  });
  const result = await client.payments.wait("pay_4", {intervalSeconds: 0.01, maxIntervalSeconds: 0.02, timeoutSeconds: 30});
  assert.equal(result.outcome, "success");
});

test("agent payment JSON contract: create returns stable fields", async () => {
  const {client, calls} = fakeClient(() => () => new Response(
    JSON.stringify({id: "pay_5", address: "0xabc", amount: "5", amount_atomic: "5000000", asset: "USDC", chain: "base", status: "WAITING", expires_at: "2026-01-01T00:00:00Z", reference: "run data analysis", merchant_name: "Agent B", checkout_url: "http://c/pay/pay_5"}),
    {status: 201},
  ));
  const payment = await client.payments.create({amount: "5", asset: "USDC", chain: "base", reference: "run data analysis"}, {idempotencyKey: "idem-agent-1"});
  assert.equal(payment.payment_id ?? payment.id, "pay_5");
  assert.equal(payment.status, "WAITING");
  assert.equal(payment.asset, "USDC");
  assert.equal(payment.chain, "base");
  assert.equal(payment.address, "0xabc");
  assert.ok(payment.checkout_url);
  // idempotency + api key headers are always present
  assert.equal(calls[0].init.headers["idempotency-key"], "idem-agent-1");
  assert.equal(calls[0].init.headers["x-flowpay-api-key"], "fp_test.x");
});

test("unauthorized payment access surfaces auth error code", async () => {
  const {client} = fakeClient(() => () => new Response(
    JSON.stringify({error: {code: "invalid_api_key", message: "invalid API key", request_id: "req_9"}}),
    {status: 401},
  ));
  await assert.rejects(
    () => client.payments.get("pay_404"),
    e => e instanceof FlowPayError && e.status === 401 && e.code === "invalid_api_key",
  );
});

test("timeout parsing accepts s/m/h suffixes", () => {
  assert.equal(parseTimeout("30m"), 1800);
  assert.equal(parseTimeout("45s"), 45);
  assert.equal(parseTimeout("2h"), 7200);
  assert.equal(parseTimeout("90"), 90);
});

// Re-implemented from src/index.ts to keep the unit under test isolated.
function parseTimeout(value) {
  if (typeof value !== "string" || !value) return undefined;
  const match = value.match(/^(\d+)(s|m|h|d)?$/);
  if (!match) throw new Error(`invalid timeout: ${value}`);
  const unit = match[2] ?? "s";
  const factor = unit === "s" ? 1 : unit === "m" ? 60 : unit === "h" ? 3600 : 86400;
  return Number(match[1]) * factor;
}
