// The TS ↔ Rust interop matrix: for every transport, a TypeScript client
// against the Rust server and a Rust client against the TypeScript server,
// each running the shared call sequence. RUST_PEER names the built
// `interop_peer` binary (cargo build -p reddb-io-toon-rpc-examples --bin interop_peer).

import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

const TRANSPORTS = ['http', 'ws', 'tcp', 'sse', 'stdio'];
const CELL_TIMEOUT_MS = 30_000;

const rustPeer = process.env.RUST_PEER;
if (!rustPeer) {
  console.error('set RUST_PEER to the interop_peer binary');
  process.exit(2);
}
const peers = {
  rust: [rustPeer],
  ts: [process.execPath, fileURLToPath(new URL('./peer.mjs', import.meta.url))],
};

function run([program, ...args], { stdout = 'inherit' } = {}) {
  return spawn(program, args, { stdio: ['ignore', stdout, 'inherit'] });
}

function exited(child) {
  return new Promise((resolve) => child.on('exit', (code, signal) => resolve(code ?? signal)));
}

function withTimeout(promise, label) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out`)), CELL_TIMEOUT_MS);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

async function cell(transport, serverSide, clientSide) {
  if (transport === 'stdio') {
    const server = JSON.stringify([...peers[serverSide], 'serve', 'stdio']);
    return exited(run([...peers[clientSide], 'call', 'stdio', server]));
  }
  const server = run([...peers[serverSide], 'serve', transport], { stdout: 'pipe' });
  try {
    const lines = createInterface({ input: server.stdout });
    const [ready] = await Promise.race([
      new Promise((resolve) => lines.once('line', (line) => resolve([line]))),
      exited(server).then((code) => [`server exited ${code}`]),
    ]);
    if (!ready.startsWith('ready ')) throw new Error(ready);
    return await exited(run([...peers[clientSide], 'call', transport, ready.slice(6)]));
  } finally {
    server.kill();
  }
}

const results = [];
for (const transport of TRANSPORTS) {
  for (const [serverSide, clientSide] of [
    ['rust', 'ts'],
    ['ts', 'rust'],
  ]) {
    const label = `${clientSide} client -> ${serverSide} server over ${transport}`;
    let outcome;
    try {
      const code = await withTimeout(cell(transport, serverSide, clientSide), label);
      outcome = code === 0 ? 'pass' : `fail (${code})`;
    } catch (error) {
      outcome = `fail (${error.message})`;
    }
    results.push({ label, outcome });
    console.log(`${outcome === 'pass' ? 'ok  ' : 'FAIL'} ${label}${outcome === 'pass' ? '' : `: ${outcome}`}`);
  }
}
const failed = results.filter(({ outcome }) => outcome !== 'pass');
console.log(`\n${results.length - failed.length}/${results.length} interop cells pass`);
process.exit(failed.length === 0 ? 0 : 1);
