import fs from "node:fs";

const env = { ...parseEnv(".env"), ...process.env };
const isLocal = String(env.FLOWPAY_ENV ?? "local").toLowerCase() === "local";
const networks = [
  ...(isLocal ? [["base", "BASE"], ["bsc", "BSC"]] : []),
  ["base_sepolia", "BASE_SEPOLIA"],
  ["ethereum_sepolia", "ETHEREUM_SEPOLIA"],
  ["arbitrum_sepolia", "ARBITRUM_SEPOLIA"],
  ["bsc_testnet", "BSC_TESTNET"],
  ["optimism_sepolia", "OPTIMISM_SEPOLIA"],
  ["polygon_amoy", "POLYGON_AMOY"],
].flatMap(([name, prefix]) => {
  const rpc = env[`${prefix}_RPC_URL`];
  if (!rpc) return [];
  const factory = env[`${prefix}_FACTORY_ADDRESS`] ?? env.FLOWPAY_FACTORY_ADDRESS;
  if (!/^0x[0-9a-fA-F]{40}$/.test(factory ?? "")) {
    throw new Error(`${prefix}_FACTORY_ADDRESS is missing or invalid`);
  }
  return [{ name, rpc, factory: factory.toLowerCase() }];
});

if (networks.length === 0) throw new Error("no EVM networks are configured");

const expectedFactory = networks[0].factory;
const probeCalldata = `0x0a47c3a1${"00".repeat(32)}`;
let expectedCode = null;
let expectedCheckout = null;

for (const network of networks) {
  if (network.factory !== expectedFactory) {
    throw new Error(`${network.name}: factory ${network.factory} differs from ${expectedFactory}`);
  }
  const [chainId, code, checkoutRaw] = await Promise.all([
    rpc(network.rpc, "eth_chainId", []),
    rpc(network.rpc, "eth_getCode", [network.factory, "latest"]),
    rpc(network.rpc, "eth_call", [{ to: network.factory, data: probeCalldata }, "latest"]),
  ]);
  if (code === "0x") throw new Error(`${network.name}: factory has no deployed bytecode`);
  if (expectedCode !== null && code.toLowerCase() !== expectedCode) {
    throw new Error(`${network.name}: factory runtime bytecode differs`);
  }
  expectedCode ??= code.toLowerCase();
  const checkout = `0x${checkoutRaw.slice(-40)}`.toLowerCase();
  if (expectedCheckout !== null && checkout !== expectedCheckout) {
    throw new Error(`${network.name}: CREATE3 checkout prediction differs`);
  }
  expectedCheckout ??= checkout;
  console.log(`${network.name} chain=${BigInt(chainId)} factory=${network.factory} probe=${checkout}`);
}

console.log(`CREATE3 verified across ${networks.length} configured EVM networks`);

async function rpc(url, method, params) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: attempt, method, params }),
        signal: AbortSignal.timeout(15_000),
      });
      const body = await response.json();
      if (!response.ok || body.error || body.result == null) {
        throw new Error(body.error?.message ?? `HTTP ${response.status}`);
      }
      return body.result;
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(`${method} failed for ${url}: ${lastError}`);
}

function parseEnv(file) {
  if (!fs.existsSync(file)) return {};
  return Object.fromEntries(
    fs.readFileSync(file, "utf8")
      .split(/\r?\n/)
      .map(line => line.trim())
      .filter(line => line && !line.startsWith("#") && line.includes("="))
      .map(line => {
        const index = line.indexOf("=");
        return [line.slice(0, index), line.slice(index + 1).replace(/^['"]|['"]$/g, "")];
      }),
  );
}
