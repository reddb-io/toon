// Drives toon-diff (https://github.com/antrixy/toon-diff, MIT) against the
// reddb-io engines and the upstream TypeScript reference. It reuses toon-diff's
// corpus, mutation generator and lossless oracle, so it runs from inside a
// toon-diff checkout: the weekly workflow copies it there (see
// docs/upstream-monitoring.md).
//
//   REDDB_ROOT=<repo> REDDB_TOON_BIN=<toon binary> \
//     node --experimental-strip-types reddb-matrix.ts [--per 200] [--seed 1]
//
// Exit 0 when every ordered engine pair round-trips every case; the one
// tolerated class is the documented numeric domain (JSON integers beyond
// i64/u64 read as f64). Exit 1 on any other finding.
import { spawnSync } from "node:child_process";
import { encode as upEncode, decode as upDecode } from "@toon-format/toon";
import { loadCorpus } from "./probe/corpus.ts";
import { ingest, equal } from "./oracle/ingest.ts";
import { generateCase } from "./gen/generate.ts";

const ROOT = process.env.REDDB_ROOT!;
const RUST_BIN = process.env.REDDB_TOON_BIN!;
const reddb = await import(`${ROOT}/packages/toon/dist/index.js`);

interface Engine {
  name: string;
  /** f64 engines round integers beyond 2^53 on ingestion (JSON.parse). */
  f64: boolean;
  encode(jsonText: string): string;
  decode(toonText: string): string;
}

function rust(args: string[], input: string): string {
  // A 500x500 grid prints more than Node's default 1 MiB buffer.
  const run = spawnSync(RUST_BIN, args, { input, encoding: "utf8", maxBuffer: 512 * 1024 * 1024 });
  if (run.status !== 0) throw new Error(`exit ${run.status} signal ${run.signal}: ${run.stderr.trim().split("\n")[0]}`);
  return run.stdout.replace(/\n$/, "");
}

const engines: Engine[] = [
  { name: "upstream-ts", f64: true, encode: (j) => upEncode(JSON.parse(j)), decode: (t) => JSON.stringify(upDecode(t)) },
  { name: "reddb-ts", f64: true, encode: (j) => reddb.encode(JSON.parse(j)), decode: (t) => JSON.stringify(reddb.decode(t)) },
  { name: "reddb-rust", f64: false, encode: (j) => rust(["-e"], j), decode: (t) => rust(["-d"], t) },
];

const args = process.argv.slice(2);
const opt = (name: string, def: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : def;
};
const per = Number(opt("per", "200"));
const baseSeed = Number(opt("seed", "1"));
const maxOps = Number(opt("maxops", "3"));

const corpus = loadCorpus();
const cases: { label: string; text: string }[] = corpus.cases
  .filter((c) => c.bucket !== "spec")
  .map((c) => ({ label: c.key, text: c.text }));
corpus.byBucket.seeds.forEach((seed, si) => {
  for (let i = 0; i < per; i++) {
    const rngSeed = (baseSeed * 1_000_003 + si * 9973 + i) >>> 0;
    const g = generateCase(seed.text, rngSeed, { seedName: seed.key, maxOps });
    cases.push({ label: `${seed.key} rngSeed=${rngSeed}`, text: g.text });
  }
});

const NUMERIC_DOMAIN = "value changed (numbers beyond i64/u64 read as f64)";
const findings = new Map<string, { count: number; example: string }>();
let checks = 0;
for (const c of cases) {
  const exact = ingest(c.text);
  const viaF64 = ingest(JSON.stringify(JSON.parse(c.text)));
  for (const X of engines) {
    let wire: string;
    try {
      wire = X.encode(c.text);
    } catch (e) {
      record(`${X.name} encode threw: ${(e as Error).message.split("\n")[0]}`, c, "");
      continue;
    }
    for (const Y of engines) {
      checks++;
      // Exact lexemes when both sides keep them, the f64 reading otherwise.
      const expected = X.f64 || Y.f64 ? viaF64 : exact;
      try {
        const back = ingest(Y.decode(wire));
        if (!equal(back, expected)) {
          record(`${X.name} -> ${Y.name}: ${equal(back, viaF64) ? NUMERIC_DOMAIN : "value changed"}`, c, wire);
        }
      } catch (e) {
        record(`${X.name} -> ${Y.name}: decode threw: ${(e as Error).message.split("\n")[0]}`, c, wire);
      }
    }
  }
}

function record(kind: string, c: { label: string; text: string }, wire: string) {
  const found = findings.get(kind);
  if (found) found.count++;
  else findings.set(kind, { count: 1, example: `${c.label}\n    json: ${c.text.slice(0, 240)}\n    wire: ${JSON.stringify(wire.slice(0, 240))}` });
}

const unexpected = [...findings].filter(([kind]) => !kind.endsWith(NUMERIC_DOMAIN));
console.log(`## toon-diff matrix\n`);
console.log(`cases=${cases.length} engines=${engines.map((e) => e.name).join(",")} pairChecks=${checks}`);
console.log(`findings: ${findings.size} classes, ${unexpected.length} unexpected\n`);
for (const [kind, { count, example }] of [...findings].sort((a, b) => b[1].count - a[1].count)) {
  console.log(`- [${count}] ${kind}${kind.endsWith(NUMERIC_DOMAIN) ? " (tolerated: documented numeric domain)" : ""}\n  ${example}`);
}
process.exit(unexpected.length === 0 ? 0 : 1);
