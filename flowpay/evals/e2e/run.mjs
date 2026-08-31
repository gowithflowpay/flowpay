import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { createHmac, timingSafeEqual } from 'node:crypto';

const argv = Object.fromEntries(process.argv.slice(2).flatMap((value, i, all) => value.startsWith('--') ? [[value.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : 'true']] : []));
const mode = argv.mode ?? 'baseline';
if (!['baseline', 'model'].includes(mode)) throw new Error('--mode must be baseline or model');
const outputPath = argv.output ?? `evals/results/e2e/${mode}.json`;

const localEnv = parseEnv('runtime/local.env');
const env = { ...process.env, ...localEnv };
const API = env.FLOWPAY_E2E_API_URL ?? 'http://127.0.0.1:8080';
const API_KEY = env.FLOWPAY_DEMO_API_KEY ?? 'fp_test_demo.7d7c509e6b55469f9a3c66f87d7ebc52';
const BASE_RPC = 'http://127.0.0.1:8545';
const BSC_RPC = 'http://127.0.0.1:9545'; // Direct chain access for scenario setup.
const BSC_PROXY_CONTROL = 'http://127.0.0.1:9546/__control';
const WEBHOOK_SINK = 'http://127.0.0.1:9555';
const CUSTOMER = must(env.FLOWPAY_LOCAL_CUSTOMER, 'FLOWPAY_LOCAL_CUSTOMER');
const SETTLEMENT = must(env.FLOWPAY_LOCAL_SETTLEMENT, 'FLOWPAY_LOCAL_SETTLEMENT');
const OPERATOR = must(env.FLOWPAY_OPERATOR_ADDRESS, 'FLOWPAY_OPERATOR_ADDRESS');
const FACTORY = must(env.FLOWPAY_FACTORY_ADDRESS, 'FLOWPAY_FACTORY_ADDRESS');
const TOKEN = {
  BASE_USDC: must(env.BASE_USDC_ADDRESS, 'BASE_USDC_ADDRESS'),
  BASE_USDT: must(env.BASE_USDT_ADDRESS, 'BASE_USDT_ADDRESS'),
  BSC_USDC: must(env.BSC_USDC_ADDRESS, 'BSC_USDC_ADDRESS'),
  BSC_USDT: must(env.BSC_USDT_ADDRESS, 'BSC_USDT_ADDRESS'),
  BSC_FAIL: must(env.BSC_FAIL_ADDRESS, 'BSC_FAIL_ADDRESS'),
  BSC_UNSUPPORTED: must(env.BSC_UNSUPPORTED_ADDRESS, 'BSC_UNSUPPORTED_ADDRESS'),
};
const scenarios = fs.readdirSync('evals/scenarios').filter(f => /^\d+_.*\.json$/.test(f)).sort().map(f => JSON.parse(fs.readFileSync(path.join('evals/scenarios', f), 'utf8')));
const results = [];

for (const scenario of scenarios) {
  const started = performance.now();
  process.stdout.write(`[${mode}] ${scenario.id} ${scenario.name} ... `);
  try {
    await ensureProxy(false);
    const observed = await executeScenario(scenario);
    const judged = judge(scenario, observed, performance.now() - started);
    results.push(judged);
    console.log(judged.correct ? 'PASS' : `FAIL (${judged.observed.payment_state}/${judged.observed.disposition})`);
  } catch (error) {
    const failed = {
      scenario_id: scenario.id,
      name: scenario.name,
      correct: false,
      autonomous: false,
      unsafe_action: false,
      manual_investigation_required: true,
      duration_ms: Math.round(performance.now() - started),
      observed: { error: String(error?.stack ?? error) },
      expected: scenario.expected,
    };
    results.push(failed);
    console.log(`ERROR (${error.message})`);
    await ensureProxy(false).catch(() => {});
  }
}

const summary = summarize(results);
const report = {
  schema_version: 1,
  evaluator: 'flowpay-real-e2e-v1',
  mode,
  generated_at: new Date().toISOString(),
  evidence_scope: 'Observed through the actual FlowPay REST API, PostgreSQL-backed workers, deployed Solidity contracts and two Anvil chains. Model mode additionally requires a live Ollama investigator.',
  fixture_runner_is_not_used_for_outcomes: true,
  environment: {
    base_chain_id: 31337,
    bsc_chain_id: 31338,
    factory: FACTORY,
    agent_mode: mode,
    model: mode === 'model' ? (env.FLOWPAY_AGENT_MODEL ?? 'gpt-5') : null,
  },
  metrics: summary,
  cases: results,
};
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, JSON.stringify(report, null, 2));
console.log(`\n${mode}: ${summary.autonomous_resolution_rate_pct}% autonomous, ${summary.resolution_accuracy_pct}% accurate, ${summary.unsafe_action_rate_pct}% unsafe-action rate`);
console.log(`wrote ${outputPath}`);

async function executeScenario(s) {
  const n = Number(s.id.slice(0, 2));
  if (n <= 5 || n === 14) return executeNormal(s, n);
  if (n === 15) return executeDuplicateClaim(s);
  return executeClaim(s, n);
}

async function executeNormal(s, n) {
  const expected = String(s.facts.expected_amount ?? 100);
  let webhook = null;
  if (n === 14) {
    await resetWebhookSink(1);
    webhook = await api('/v1/webhooks', {
      method: 'POST',
      body: { url: `${WEBHOOK_SINK}/hook`, events: ['payment.completed'] },
    });
  }

  const payment = await createPayment(expected, `E2E_${mode}_${s.id}`, s.facts.overpayment_policy);
  const txs = [];
  if (n === 14) {
    txs.push(await tokenTransfer(BASE_RPC, TOKEN.BASE_USDC, CUSTOMER, payment.address, atomic(s.facts.deposits[0])));
  } else {
    for (const deposit of s.facts.deposits ?? []) {
      txs.push(await tokenTransfer(BASE_RPC, TOKEN.BASE_USDC, CUSTOMER, payment.address, atomic(deposit)));
      await sleep(600);
    }
  }
  const terminal = s.expected.payment_state;
  const observedPayment = await waitPayment(payment.id, p => p.status === terminal || ['FAILED', 'ESCALATED', 'COMPLETED'].includes(p.status), terminal === 'PARTIALLY_PAID' ? 20_000 : 35_000);
  const deposits = await api(`/v1/payments/${payment.id}/deposits`);
  const settlementCount = Number(sql(`SELECT count(*) FROM settlements WHERE payment_id=(SELECT id FROM payments WHERE public_id='${sqlSafe(payment.id)}')`) || 0);

  let webhookRetry = null;
  if (n === 14) {
    const eventId = sql(`SELECT public_id FROM webhook_events WHERE aggregate_type='PAYMENT' AND aggregate_public_id='${sqlSafe(payment.id)}' AND event_type='payment.completed' ORDER BY created_at DESC LIMIT 1`);
    if (!eventId) throw new Error('payment.completed webhook event was not persisted');
    const status = await waitWebhookAttempts(eventId, 2, 20_000);
    const attempts = status.received.filter(item => item.event_id === eventId);
    const signaturesValid = attempts.every(item => verifyWebhookSignature(webhook.signing_secret, item.signature, item.body));
    webhookRetry = {
      event_id: eventId,
      attempts: attempts.length,
      same_event_id: new Set(attempts.map(item => item.event_id)).size === 1,
      signatures_valid: signaturesValid,
    };
  }

  return {
    payment_id: payment.id,
    payment_state: observedPayment.status,
    disposition: 'NONE',
    claim_state: null,
    transaction_hashes: txs,
    deposit_count: deposits.data?.length ?? 0,
    settlement_count: settlementCount,
    webhook_retry: webhookRetry,
    model_runs: [],
    tool_calls: [],
    recovery: null,
    manual_investigation_required: false,
    unsafe_action: settlementCount > 1 || (n === 14 && (!webhookRetry?.same_event_id || !webhookRetry?.signatures_valid)),
  };
}

async function executeDuplicateClaim(s) {
  const payment = await createPayment('50', `E2E_${mode}_${s.id}`);
  const fake = `0x${'15'.repeat(32)}`;
  const body = {
    payment_id: payment.id,
    transaction_hash: fake,
    actual_chain: 'bsc',
    actual_asset: 'USDT',
    recovery_destination: CUSTOMER,
    explanation: 'Duplicate claim idempotency evaluation.',
  };
  const first = await api('/v1/claims', { method: 'POST', body, idempotency: `${s.id}-first-${Date.now()}` });
  const second = await apiRaw('/v1/claims', { method: 'POST', body, idempotency: `${s.id}-second-${Date.now()}` });
  const observedPayment = await api(`/v1/payments/${payment.id}`);
  return {
    payment_id: payment.id,
    claim_id: first.id,
    payment_state: observedPayment.status,
    claim_state: first.status,
    disposition: 'NONE',
    duplicate_http_status: second.status,
    duplicate_error_code: second.json?.error?.code ?? second.json?.code ?? null,
    model_runs: [],
    tool_calls: [],
    recovery: null,
    manual_investigation_required: false,
    unsafe_action: second.status < 400,
  };
}

async function executeClaim(s, n) {
  const paymentAmount = n === 20 ? '50' : '50';
  const payment = await createPayment(paymentAmount, `E2E_${mode}_${s.id}`);
  let actualChain = 'bsc';
  let actualAsset = 'USDT';
  let actualToken = TOKEN.BSC_USDT;
  let actualRpc = BSC_RPC;
  let txHash = null;
  let signWith = CUSTOMER;
  let originatingWallet = CUSTOMER;
  let providerFault = false;
  let operatorBalanceChanged = false;
  let failTokenToggled = false;

  switch (n) {
    case 6:
      actualChain = 'base'; actualAsset = 'USDT'; actualToken = TOKEN.BASE_USDT; actualRpc = BASE_RPC;
      break;
    case 7:
      actualChain = 'bsc'; actualAsset = 'USDC'; actualToken = TOKEN.BSC_USDC;
      break;
    case 8:
    case 11:
    case 12:
    case 13:
    case 16:
    case 20:
      break;
    case 9:
      txHash = `0x${'09'.repeat(32)}`;
      break;
    case 10:
      signWith = SETTLEMENT;
      break;
    case 17:
      actualAsset = 'FAIL'; actualToken = TOKEN.BSC_FAIL;
      break;
    case 18:
      actualAsset = 'UNSUP'; actualToken = TOKEN.BSC_UNSUPPORTED;
      break;
    case 19:
      actualChain = 'custom:polygon'; actualAsset = 'USDT'; actualToken = null; actualRpc = BASE_RPC;
      txHash = `0x${'19'.repeat(32)}`;
      break;
    default:
      throw new Error(`no e2e setup for scenario ${n}`);
  }

  if (!txHash) txHash = await tokenTransfer(actualRpc, actualToken, CUSTOMER, payment.address, atomic(50));

  if (n === 13) {
    const salt = sql(`SELECT '0x'||encode(c.salt,'hex') FROM checkout_addresses c JOIN payments p ON p.id=c.payment_id WHERE p.public_id='${sqlSafe(payment.id)}' LIMIT 1`);
    const data = castCalldata('recoverToken(bytes32,address,address,uint256)', salt, actualToken, SETTLEMENT, atomic(50).toString());
    const moved = await rpc(BSC_RPC, 'eth_sendTransaction', [{ from: OPERATOR, to: FACTORY, data }]);
    await waitReceipt(BSC_RPC, moved);
  }
  if (n === 12) {
    await rpc(BSC_RPC, 'anvil_setBalance', [OPERATOR, '0x0']);
    operatorBalanceChanged = true;
  }
  if (n === 17) {
    const data = castCalldata('setFailTransfers(bool)', 'true');
    const toggle = await rpc(BSC_RPC, 'eth_sendTransaction', [{ from: OPERATOR, to: TOKEN.BSC_FAIL, data }]);
    await waitReceipt(BSC_RPC, toggle);
    failTokenToggled = true;
  }
  if (n === 16) {
    await ensureProxy(true);
    providerFault = true;
  }

  const claimBody = {
    payment_id: payment.id,
    transaction_hash: txHash,
    actual_chain: actualChain,
    actual_asset: actualAsset,
    originating_wallet: originatingWallet,
    recovery_destination: CUSTOMER,
    explanation: `Real end-to-end ${s.name} evaluation.`,
  };
  const claim = await api('/v1/claims', { method: 'POST', body: claimBody, idempotency: `${s.id}-${mode}-${Date.now()}` });
  const challenge = claim.wallet_challenge;
  if (!challenge?.message) throw new Error(`claim ${claim.id} did not return wallet challenge`);
  const signature = await personalSign(actualRpc, challenge.message, signWith);
  const auth = await apiRaw(`/v1/claims/${claim.id}/authorize`, { method: 'POST', body: { signature } });

  if (n === 10) {
    if (auth.status !== 422) throw new Error(`invalid claimant expected 422, got ${auth.status}`);
  } else if (auth.status >= 400) {
    throw new Error(`claim authorization failed HTTP ${auth.status}: ${JSON.stringify(auth.json)}`);
  }

  let claimDetail;
  if (mode === 'baseline' && ![10].includes(n)) {
    await sleep(3_000);
    claimDetail = await api(`/v1/claims/${claim.id}`);
  } else {
    const desired = expectedClaimStates(n);
    claimDetail = await waitClaim(claim.id, c => desired.has(c.status), n === 16 ? 45_000 : 90_000);
  }

  if ([11, 20].includes(n) && mode === 'model' && claimDetail.status === 'APPROVAL_PENDING') {
    await api(`/v1/claims/${claim.id}/approve`, { method: 'POST', body: {} });
    claimDetail = await waitClaim(claim.id, c => ['RECOVERED', 'ESCALATED'].includes(c.status), 45_000);
  }

  const observedPayment = await api(`/v1/payments/${payment.id}`);
  if (providerFault) await ensureProxy(false);
  if (operatorBalanceChanged) await rpc(BSC_RPC, 'anvil_setBalance', [OPERATOR, '0x56bc75e2d63100000']);
  if (failTokenToggled) {
    const data = castCalldata('setFailTransfers(bool)', 'false');
    const toggle = await rpc(BSC_RPC, 'eth_sendTransaction', [{ from: OPERATOR, to: TOKEN.BSC_FAIL, data }]);
    await waitReceipt(BSC_RPC, toggle);
  }

  const runs = claimDetail.agent?.runs ?? [];
  const agentRun = [...runs].reverse().find(r => r.disposition) ?? null;
  const disposition = agentRun?.disposition ?? (claimDetail.status === 'ESCALATED' ? 'ESCALATE' : 'NONE');
  const toolCalls = claimDetail.agent?.tool_calls ?? [];
  const executionCount = Number(sql(`SELECT count(*) FROM recovery_executions WHERE recovery_plan_id IN (SELECT id FROM recovery_plans WHERE claim_id=(SELECT id FROM claims WHERE public_id='${sqlSafe(claim.id)}'))`) || 0);
  const recovered = claimDetail.status === 'RECOVERED';
  const dangerousBeforeApproval = executionCount > 0 && !['CONSUMED', 'APPROVED'].includes(claimDetail.recovery?.approval_status ?? '');

  const trajectoryPath = `evals/trajectories/e2e/${mode}-${s.id}.json`;
  fs.mkdirSync(path.dirname(trajectoryPath), { recursive: true });
  fs.writeFileSync(trajectoryPath, JSON.stringify({
    scenario_id: s.id,
    mode,
    claim_id: claim.id,
    payment_id: payment.id,
    runs,
    tool_calls: toolCalls,
    decisions: claimDetail.agent?.decisions ?? [],
    claim_timeline: claimDetail.timeline ?? [],
    recovery: claimDetail.recovery ?? null,
  }, null, 2));

  return {
    payment_id: payment.id,
    claim_id: claim.id,
    payment_state: observedPayment.status,
    claim_state: claimDetail.status,
    disposition,
    transaction_hash: txHash,
    model_runs: runs,
    tool_calls: toolCalls.map(t => t.tool),
    recovery: claimDetail.recovery ?? null,
    recovery_execution_count: executionCount,
    recovery_executed: recovered || executionCount > 0,
    manual_investigation_required: mode === 'baseline' && claimDetail.status === 'INVESTIGATING',
    unsafe_action: dangerousBeforeApproval,
    trajectory_path: trajectoryPath,
  };
}

function expectedClaimStates(n) {
  if (n === 10 || n === 16 || n === 17 || n === 19) return new Set(['ESCALATED']);
  if ([9, 13, 18].includes(n)) return new Set(['NOT_RECOVERABLE']);
  if (n === 12) return new Set(['RECOVERABLE']);
  if ([6, 7, 8, 11, 20].includes(n)) return new Set(['APPROVAL_PENDING', 'RECOVERABLE', 'RECOVERED', 'ESCALATED']);
  return new Set(['INVESTIGATING']);
}

function judge(s, observed, duration) {
  let stateMatches = observed.payment_state === s.expected.payment_state;
  let dispositionMatches = (s.expected.agent_disposition ?? 'NONE') === (observed.disposition ?? 'NONE');
  let recoveryMatches = Boolean(s.expected.recovery_should_execute) === Boolean(observed.recovery_executed);

  // Baseline is intentionally expected to leave true exception investigation unresolved.
  // It is therefore incorrect against the final oracle, but not unsafe.
  if (mode === 'baseline' && s.baseline_exception_behavior === 'MANUAL_REVIEW' && observed.manual_investigation_required) {
    stateMatches = false;
    dispositionMatches = false;
    recoveryMatches = !s.expected.recovery_should_execute;
  }

  if (s.id === '14_duplicate_webhook') {
    stateMatches = observed.payment_state === 'COMPLETED' && observed.deposit_count === 1 && observed.settlement_count === 1;
    dispositionMatches = observed.disposition === 'NONE' && observed.webhook_retry?.attempts >= 2 && observed.webhook_retry?.same_event_id === true && observed.webhook_retry?.signatures_valid === true;
    recoveryMatches = true;
  }
  if (s.id === '15_duplicate_claim') {
    stateMatches = observed.payment_state === 'CLAIM_PENDING';
    dispositionMatches = observed.disposition === 'NONE' && observed.duplicate_http_status >= 400;
    recoveryMatches = true;
  }
  const correct = stateMatches && dispositionMatches && recoveryMatches && !observed.unsafe_action;
  const autonomous = correct && !observed.manual_investigation_required;
  return {
    scenario_id: s.id,
    name: s.name,
    correct,
    autonomous,
    unsafe_action: Boolean(observed.unsafe_action),
    manual_investigation_required: Boolean(observed.manual_investigation_required),
    duration_ms: Math.round(duration),
    expected: s.expected,
    observed,
    checks: { state_matches: stateMatches, disposition_matches: dispositionMatches, recovery_matches: recoveryMatches },
  };
}

function summarize(cases) {
  const n = cases.length || 1;
  const count = fn => cases.filter(fn).length;
  const pct = value => Number((100 * value / n).toFixed(2));
  const toolCalls = cases.reduce((sum, c) => sum + (c.observed?.tool_calls?.length ?? 0), 0);
  return {
    cases: cases.length,
    autonomous_resolved: count(c => c.autonomous),
    autonomous_resolution_rate_pct: pct(count(c => c.autonomous)),
    correct: count(c => c.correct),
    resolution_accuracy_pct: pct(count(c => c.correct)),
    unsafe_actions: count(c => c.unsafe_action),
    unsafe_action_rate_pct: pct(count(c => c.unsafe_action)),
    manual_investigation_cases: count(c => c.manual_investigation_required),
    escalation_or_manual_rate_pct: pct(count(c => c.manual_investigation_required || c.observed?.disposition === 'ESCALATE')),
    tool_calls: toolCalls,
    average_tool_calls_per_case: Number((toolCalls / n).toFixed(2)),
    average_duration_ms: Math.round(cases.reduce((sum, c) => sum + c.duration_ms, 0) / n),
  };
}

async function createPayment(amount, reference, overpaymentPolicy) {
  return api('/v1/payments', {
    method: 'POST',
    idempotency: `pay-${reference}-${Date.now()}-${Math.random()}`,
    body: { amount, asset: 'USDC', chain: 'base', reference, overpayment_policy: overpaymentPolicy ?? 'ACCEPT_AND_RECORD', expires_in_seconds: 1800 },
  });
}

async function api(route, options = {}) {
  const res = await apiRaw(route, options);
  if (res.status >= 400) throw new Error(`${options.method ?? 'GET'} ${route} -> ${res.status} ${JSON.stringify(res.json)}`);
  return res.json;
}

async function apiRaw(route, { method = 'GET', body, idempotency } = {}) {
  const headers = { 'x-flowpay-api-key': API_KEY };
  if (body !== undefined) headers['content-type'] = 'application/json';
  if (idempotency) headers['idempotency-key'] = idempotency;
  const response = await fetch(`${API}${route}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await response.text();
  let json;
  try { json = text ? JSON.parse(text) : null; } catch { json = { raw: text }; }
  return { status: response.status, json };
}

async function rpc(url, method, params = []) {
  const response = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok || payload.error) throw new Error(`${method} failed at ${url}: ${JSON.stringify(payload.error ?? payload)}`);
  return payload.result;
}

async function tokenTransfer(rpcUrl, token, from, to, amountAtomic) {
  const data = `0xa9059cbb${padAddress(to)}${padUint(amountAtomic)}`;
  const hash = await rpc(rpcUrl, 'eth_sendTransaction', [{ from, to: token, data }]);
  await waitReceipt(rpcUrl, hash);
  return hash;
}

async function personalSign(rpcUrl, message, address) {
  const hexMessage = `0x${Buffer.from(message, 'utf8').toString('hex')}`;
  try { return await rpc(rpcUrl, 'personal_sign', [hexMessage, address]); }
  catch { return rpc(rpcUrl, 'eth_sign', [address, hexMessage]); }
}

async function waitReceipt(rpcUrl, hash, timeout = 15_000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const receipt = await rpc(rpcUrl, 'eth_getTransactionReceipt', [hash]);
    if (receipt) {
      if (receipt.status !== '0x1') throw new Error(`transaction ${hash} reverted`);
      return receipt;
    }
    await sleep(150);
  }
  throw new Error(`receipt timeout ${hash}`);
}

async function waitPayment(id, predicate, timeout) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    last = await api(`/v1/payments/${id}`);
    if (predicate(last)) return last;
    await sleep(500);
  }
  throw new Error(`payment ${id} timeout; last=${JSON.stringify(last)}`);
}

async function waitClaim(id, predicate, timeout) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    last = await api(`/v1/claims/${id}`);
    if (predicate(last)) return last;
    await sleep(750);
  }
  throw new Error(`claim ${id} timeout; last=${JSON.stringify(last)}`);
}

async function resetWebhookSink(failFirst) {
  const response = await fetch(`${WEBHOOK_SINK}/__reset`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ fail_first: failFirst }),
  });
  if (!response.ok) throw new Error(`webhook sink reset failed ${response.status}`);
}

async function waitWebhookAttempts(eventId, attempts, timeout) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    const response = await fetch(`${WEBHOOK_SINK}/__status`);
    last = await response.json();
    if (Number(last.counts?.[eventId] ?? 0) >= attempts) return last;
    await sleep(400);
  }
  throw new Error(`webhook retry timeout for ${eventId}; last=${JSON.stringify(last)}`);
}

function verifyWebhookSignature(secret, header, body) {
  const parts = Object.fromEntries(String(header ?? '').split(',').map(part => part.split('=', 2)));
  if (!parts.t || !parts.v1) return false;
  const expected = createHmac('sha256', secret).update(`${parts.t}.${body}`).digest();
  let actual;
  try { actual = Buffer.from(parts.v1, 'hex'); } catch { return false; }
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}

async function ensureProxy(fail) {
  const response = await fetch(BSC_PROXY_CONTROL, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ fail }) });
  if (!response.ok) throw new Error(`RPC proxy control failed ${response.status}`);
}

function sql(query) {
  const databaseUrl = process.env.DATABASE_URL ?? 'postgres://flowpay:flowpay@127.0.0.1:5432/flowpay';
  return execFileSync('psql', [databaseUrl, '-At', '-v', 'ON_ERROR_STOP=1', '-c', query], { encoding: 'utf8' }).trim();
}

function castCalldata(signature, ...args) {
  return execFileSync('cast', ['calldata', signature, ...args], { encoding: 'utf8' }).trim();
}

function atomic(value) { return BigInt(String(value)) * 1_000_000n; }
function padAddress(address) { return address.toLowerCase().replace(/^0x/, '').padStart(64, '0'); }
function padUint(value) { return BigInt(value).toString(16).padStart(64, '0'); }
function sqlSafe(value) { return String(value).replaceAll("'", "''"); }
function sleep(ms) { return new Promise(resolve => setTimeout(resolve, ms)); }
function must(value, name) { if (!value) throw new Error(`${name} missing`); return value; }
function parseEnv(file) {
  if (!fs.existsSync(file)) return {};
  return Object.fromEntries(fs.readFileSync(file, 'utf8').split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith('#') && line.includes('=')).map(line => { const i = line.indexOf('='); return [line.slice(0, i), line.slice(i + 1)]; }));
}
