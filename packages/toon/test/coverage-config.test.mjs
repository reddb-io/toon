import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..')
const read = (path) => readFileSync(resolve(root, path), 'utf8')

test('the TypeScript package enforces 95% line coverage', () => {
  const packageJson = JSON.parse(read('packages/toon/package.json'))

  assert.equal(packageJson.scripts.test, 'pnpm test:coverage')
  assert.match(packageJson.scripts['test:coverage'], /^c8 /)
  assert.match(packageJson.scripts['test:coverage'], /--check-coverage/)
  assert.match(packageJson.scripts['test:coverage'], /--lines 95(?:\s|$)/)
  assert.match(packageJson.scripts['test:coverage'], /coverage-config\.test\.mjs/)
  assert.ok(packageJson.devDependencies.c8)
})

test('CI and AFK validate TypeScript coverage beside Rust coverage', () => {
  const ci = read('.github/workflows/ci.yml')
  assert.match(ci, /run: bash scripts\/check-rust-coverage\.sh/)
  assert.match(
    ci,
    /- name: TypeScript coverage\s+run: pnpm --filter @reddb-io\/toon test:coverage/,
  )

  const validation = read('.red/config.yaml')
  const postDone = validation.match(/post_done:[\s\S]*?landing:/)?.[0]
  assert.ok(postDone, 'AFK validation must declare a post_done moment')
  assert.match(postDone, /bash scripts\/check-rust-coverage\.sh/)
  assert.match(postDone, /pnpm --filter @reddb-io\/toon test:coverage/)
})
