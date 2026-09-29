import http from 'node:http';

const port = Number(process.env.FLOWPAY_WEBHOOK_SINK_PORT ?? 9555);
let failFirst = 0;
let received = [];

const server = http.createServer(async (req, res) => {
  if (req.method === 'POST' && req.url === '/hook') {
    const body = await readBody(req);
    const eventId = String(req.headers['flowpay-event-id'] ?? '');
    const signature = String(req.headers['flowpay-signature'] ?? '');
    received.push({
      event_id: eventId,
      signature_present: signature.length > 0,
      signature,
      body,
      received_at: new Date().toISOString(),
    });
    if (failFirst > 0) {
      failFirst -= 1;
      return json(res, 503, { ok: false, retry_me: true });
    }
    return json(res, 200, { ok: true });
  }

  if (req.method === 'POST' && req.url === '/__reset') {
    const body = parseJson(await readBody(req));
    failFirst = Math.max(0, Number(body.fail_first ?? 0));
    received = [];
    return json(res, 200, { ok: true, fail_first: failFirst });
  }

  if (req.method === 'GET' && req.url === '/__status') {
    const counts = Object.fromEntries([...new Set(received.map(item => item.event_id))].map(id => [id, received.filter(item => item.event_id === id).length]));
    return json(res, 200, { ok: true, fail_first_remaining: failFirst, received, counts });
  }

  return json(res, 404, { ok: false });
});

server.listen(port, '127.0.0.1', () => {
  console.log(`FlowPay e2e webhook sink listening on http://127.0.0.1:${port}`);
});

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', chunk => chunks.push(chunk));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}
function parseJson(value) { try { return value ? JSON.parse(value) : {}; } catch { return {}; } }
function json(res, status, value) { res.writeHead(status, { 'content-type': 'application/json' }); res.end(JSON.stringify(value)); }
