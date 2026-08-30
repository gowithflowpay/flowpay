const rpcUrl = process.env.RPC_URL;
const account = process.env.ACCOUNT?.toLowerCase();
const windowSize = Number(process.env.FROM_BLOCK_WINDOW ?? 120000);
const chunkSize = Number(process.env.LOG_CHUNK_SIZE ?? 5000);
if (!rpcUrl || !/^0x[0-9a-f]{40}$/.test(account ?? "")) {
  throw new Error("RPC_URL and a valid ACCOUNT are required");
}

let requestId = 0;
async function rpc(method, params) {
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: {"content-type": "application/json"},
    body: JSON.stringify({jsonrpc: "2.0", id: ++requestId, method, params}),
  });
  const body = await response.json();
  if (body.error) throw new Error(`${method}: ${body.error.message}`);
  return body.result;
}

const transferTopic = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const accountTopic = `0x${account.slice(2).padStart(64, "0")}`;
const latest = Number.parseInt(await rpc("eth_blockNumber", []), 16);
const first = Math.max(0, latest - windowSize);
const contracts = new Set();

for (let from = first; from <= latest; from += chunkSize) {
  const to = Math.min(latest, from + chunkSize - 1);
  const logs = await rpc("eth_getLogs", [{
    fromBlock: `0x${from.toString(16)}`,
    toBlock: `0x${to.toString(16)}`,
    topics: [transferTopic, null, accountTopic],
  }]);
  for (const log of logs) contracts.add(log.address.toLowerCase());
}

const balanceSelector = "70a08231";
const symbolSelector = "95d89b41";
const decimalsSelector = "313ce567";
const balanceData = `0x${balanceSelector}${account.slice(2).padStart(64, "0")}`;

for (const contract of contracts) {
  const [balance, symbolRaw, decimalsRaw] = await Promise.all([
    rpc("eth_call", [{to: contract, data: balanceData}, "latest"]),
    rpc("eth_call", [{to: contract, data: `0x${symbolSelector}`}, "latest"]).catch(() => "0x"),
    rpc("eth_call", [{to: contract, data: `0x${decimalsSelector}`}, "latest"]).catch(() => "0x"),
  ]);
  let symbol = "unknown";
  try {
    const bytes = Buffer.from(symbolRaw.slice(2), "hex");
    const offset = Number(BigInt(`0x${bytes.subarray(0, 32).toString("hex")}`));
    const length = Number(BigInt(`0x${bytes.subarray(offset, offset + 32).toString("hex")}`));
    symbol = bytes.subarray(offset + 32, offset + 32 + length).toString("utf8");
  } catch {}
  const decimals = decimalsRaw === "0x" ? null : Number(BigInt(decimalsRaw));
  console.log(JSON.stringify({contract, symbol, decimals, balance: BigInt(balance).toString()}));
}
