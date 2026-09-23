#!/usr/bin/env node
// Line coverage per RPC component: each Rust crate (cargo llvm-cov) and each
// TypeScript package (Node's built-in coverage over its compiled dist/).
// Components with a floor in rpc-coverage-floors.json fail below it; the
// others are reported so a floor can be set from a measured value.
//
//   node scripts/rpc-coverage.mjs            both languages
//   node scripts/rpc-coverage.mjs --ts-only  TypeScript only (no cargo)

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const floors = JSON.parse(readFileSync(join(root, 'scripts/rpc-coverage-floors.json'), 'utf8'));
const RUST_CRATES = [
  'reddb-io-toon-rpc',
  'reddb-io-toon-rpc-stdio',
  'reddb-io-toon-rpc-tcp',
  'reddb-io-toon-rpc-http',
  'reddb-io-toon-rpc-ws',
  'reddb-io-toon-rpc-sse',
  'reddb-io-toon-rpc-mcp',
  'reddb-io-toon-rpc-acp',
  'reddb-io-toon-rpc-codegen',
  'reddb-io-toon-rpc-cli',
];
const TS_PACKAGES = ['toon-rpc', 'multi-rpc', 'toon-rpc-mcp', 'toon-rpc-acp'];
const rows = [];

function add(component, covered, total) {
  rows.push({ component, covered, total, percent: total === 0 ? 100 : (100 * covered) / total });
}

if (!process.argv.includes('--ts-only')) {
  const args = ['llvm-cov', '--json', '--summary-only'];
  for (const crate of RUST_CRATES) args.push('-p', crate);
  const report = JSON.parse(
    execFileSync('cargo', args, { cwd: root, maxBuffer: 1 << 30, stdio: ['ignore', 'pipe', 'inherit'] })
  );
  for (const crate of RUST_CRATES) {
    const files = report.data[0].files.filter((file) =>
      relative(root, file.filename).startsWith(`crates/${crate}/src/`)
    );
    const sum = (key) => files.reduce((total, file) => total + file.summary.lines[key], 0);
    add(crate, sum('covered'), sum('count'));
  }
}

for (const name of TS_PACKAGES) {
  const directory = join(root, 'packages', name);
  const scratch = mkdtempSync(join(tmpdir(), 'rpc-coverage-'));
  const lcov = join(scratch, 'lcov.info');
  try {
    const tests = readdirSync(join(directory, 'test'))
      .filter((file) => file.endsWith('.test.mjs'))
      .map((file) => join('test', file));
    execFileSync(
      process.execPath,
      ['--test', '--experimental-test-coverage', '--test-reporter=lcov', `--test-reporter-destination=${lcov}`, ...tests],
      { cwd: directory, stdio: ['ignore', 'ignore', 'inherit'] }
    );
    let covered = 0;
    let total = 0;
    let inDist = false;
    for (const line of readFileSync(lcov, 'utf8').split('\n')) {
      // Paths are relative to the package, where the tests ran.
      if (line.startsWith('SF:')) inDist = relative(directory, resolve(directory, line.slice(3))).startsWith('dist/');
      else if (inDist && line.startsWith('LF:')) total += Number(line.slice(3));
      else if (inDist && line.startsWith('LH:')) covered += Number(line.slice(3));
    }
    add(`@reddb-io/${name}`, covered, total);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

let failed = false;
console.log('| Component | Lines | Coverage | Floor |\n| --- | ---: | ---: | ---: |');
for (const { component, covered, total, percent } of rows) {
  const floor = floors[component];
  const below = floor !== undefined && percent < floor;
  failed ||= below;
  console.log(
    `| ${component} | ${covered}/${total} | ${percent.toFixed(1)}% | ${floor === undefined ? 'report only' : `${floor}%`}${below ? ' **below**' : ''} |`
  );
}
process.exit(failed ? 1 : 0);
