import http from 'node:http';

const listenPort = Number(process.env.FLOWPAY_E2E_RPC_PROXY_PORT ?? 9546);
const target = new URL(process.env.FLOWPAY_E2E_RPC_TARGET ?? 'http://127.0.0.1:9545');
let fail = false;
let failureCount = 0;

const server = http.createServer(async (req, res) => {
  if (req.url === '/__control' && req.method === 'POST') {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}');
    fail = Boolean(body.fail);
    failureCount = 0;
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ ok: true, fail }));
    return;
  }
  if (req.url === '/__status') {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ ok: true, fail, failureCount, target: target.href }));
    return;
  }
  if (fail) {
    failureCount += 1;
    res.writeHead(503, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: 'injected_provider_failure', failureCount }));
    return;
  }
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = Buffer.concat(chunks);
  const upstream = http.request({
    hostname: target.hostname,
    port: target.port,
    path: '/',
    method: req.method,
    headers: { ...req.headers, host: target.host, 'content-length': String(body.length) },
  }, upstreamRes => {
    res.writeHead(upstreamRes.statusCode ?? 502, upstreamRes.headers);
    upstreamRes.pipe(res);
  });
  upstream.on('error', error => {
    res.writeHead(502, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: 'proxy_upstream_error', message: error.message }));
  });
  upstream.end(body);
});

server.listen(listenPort, '127.0.0.1', () => {
  console.log(`FlowPay eval RPC proxy http://127.0.0.1:${listenPort} -> ${target.href}`);
});
