import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {mkdtemp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StdioClientTransport} from '@modelcontextprotocol/sdk/client/stdio.js';

test('MCP uses CLI email login to install and verify without exposing credentials', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'flowpay-mcp-login-'));
  const configDirectory = path.join(directory, 'credentials');
  await mkdir(configDirectory);
  const requests = [];
  const server = createServer(async (request, response) => {
    let text = '';
    for await (const chunk of request) text += chunk;
    requests.push({url: request.url, headers: request.headers});
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify(request.url === '/v1/api-keys' ? {api_key: 'fp_test.integration-secret'} : {data: []}));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  const client = new Client({name: 'flowpay-test', version: '1.0.0'});
  const entry = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../dist/index.js');
  const transport = new StdioClientTransport({command: process.execPath, args: [entry], env: {...process.env, FLOWPAY_PROJECT_ROOT: directory, FLOWPAY_CONFIG_DIR: configDirectory, FLOWPAY_API_URL: base, FLOWPAY_API_KEY: '', FLOWPAY_MERCHANT_TOKEN: ''}});
  try {
    await writeFile(path.join(directory, 'package.json'), JSON.stringify({dependencies: {next: '15.5.24'}}));
    await writeFile(path.join(configDirectory, 'config.json'), JSON.stringify({baseUrl: base, sessionToken: 'session-secret', sessionExpiresAt: Date.now() + 60000}));
    await client.connect(transport);
    const installed = await client.callTool({name: 'flowpay_install_nextjs', arguments: {}});
    assert.notEqual(installed.isError, true, JSON.stringify(installed));
    assert.doesNotMatch(JSON.stringify(installed), /session-secret|integration-secret/);
    assert.equal(requests[0].headers.authorization, 'Bearer session-secret');
    const env = await readFile(path.join(directory, '.env.local'), 'utf8');
    assert.match(env, /FLOWPAY_API_KEY=fp_test.integration-secret/);
    const verified = await client.callTool({name: 'flowpay_verify_integration', arguments: {}});
    assert.notEqual(verified.isError, true);
    assert.equal(JSON.parse(verified.content[0].text).ok, true);
    assert.doesNotMatch(JSON.stringify(verified), /session-secret|integration-secret/);
    const component = await readFile(path.join(directory, 'components/FlowPayButton.tsx'), 'utf8');
    assert.doesNotMatch(component, /session-secret|integration-secret/);
  } finally {
    await client.close().catch(() => {});
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await rm(directory, {recursive: true, force: true});
  }
});
