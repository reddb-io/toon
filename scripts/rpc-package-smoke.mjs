#!/usr/bin/env node
// Packs every RPC package (and the codec they depend on) the way npm would
// publish them, installs the tarballs into an empty project, imports every
// export, and runs one real exchange per package. A missing file in `files`,
// a broken `exports` entry or a workspace-only import fails here, before a
// release can publish it.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const PACKAGES = ['toon', 'toon-rpc', 'multi-rpc', 'toon-rpc-mcp', 'toon-rpc-acp'];

const work = mkdtempSync(join(tmpdir(), 'rpc-package-smoke-'));
const packs = join(work, 'packs');
const project = join(work, 'project');
try {
  for (const name of PACKAGES) {
    execFileSync('pnpm', ['pack', '--pack-destination', packs], {
      cwd: join(root, 'packages', name),
      stdio: ['ignore', 'ignore', 'inherit'],
    });
  }
  execFileSync('mkdir', ['-p', project]);
  writeFileSync(join(project, 'package.json'), JSON.stringify({ name: 'rpc-smoke', private: true, type: 'module' }));
  const tarballs = readdirSync(packs).map((file) => join(packs, file));
  execFileSync('npm', ['install', '--no-audit', '--no-fund', '--omit=peer', ...tarballs], {
    cwd: project,
    stdio: ['ignore', 'ignore', 'inherit'],
  });

  const imports = [];
  for (const name of PACKAGES) {
    const manifest = JSON.parse(readFileSync(join(root, 'packages', name, 'package.json'), 'utf8'));
    for (const subpath of Object.keys(manifest.exports ?? { '.': null })) {
      if (subpath.includes('*')) continue;
      imports.push(`${manifest.name}${subpath === '.' ? '' : subpath.slice(1)}`);
    }
  }
  const smoke = `
    import assert from 'node:assert/strict';
    for (const specifier of ${JSON.stringify(imports)}) await import(specifier);

    const { Client, Server } = await import('@reddb-io/toon-rpc');
    const { serveTcp } = await import('@reddb-io/toon-rpc/serve');
    const { TcpTransport } = await import('@reddb-io/toon-rpc/tcp');
    const server = new Server();
    server.register('add', async ([a, b]) => a + b);
    const handle = await serveTcp(server);
    const client = new Client(new TcpTransport({ host: '127.0.0.1', port: handle.address.port }));
    assert.equal(await client.call('add', [2, 3]), 5);
    await client.close();
    await handle.close();

    const { MultiRpc } = await import('@reddb-io/multi-rpc');
    const json = await new MultiRpc(server).handle('{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1}');
    assert.equal(JSON.parse(new TextDecoder().decode(json)).result, 3);

    const { McpServer, MCP_PROTOCOL_VERSION } = await import('@reddb-io/toon-rpc-mcp');
    const mcp = new McpServer({ serverInfo: { name: 'smoke', version: '1.0.0' } });
    const initialize = JSON.parse(await mcp.handleLine(JSON.stringify({
      jsonrpc: '2.0', id: 1, method: 'initialize',
      params: { protocolVersion: MCP_PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: 'c', version: '1' } },
    })));
    assert.equal(initialize.result.protocolVersion, MCP_PROTOCOL_VERSION);

    const { ACP_API_VERSION } = await import('@reddb-io/toon-rpc-acp');
    assert.equal(typeof ACP_API_VERSION, 'string');
    console.log(${JSON.stringify(`${imports.length} exports import; one exchange per package works`)});
  `;
  writeFileSync(join(project, 'smoke.mjs'), smoke);
  execFileSync(process.execPath, ['smoke.mjs'], { cwd: project, stdio: 'inherit' });
} finally {
  rmSync(work, { recursive: true, force: true });
}
