/**
 * Copies the built `@reddb-io/toon` codec into `dist/codec/` so the packaged
 * extension carries it: vsce does not follow pnpm workspace links. The codec
 * is ESM, so the copy gets its own `package.json` marking it as a module.
 */

import { cpSync, rmSync, writeFileSync } from 'node:fs'

const source = new URL('../../toon/dist/', import.meta.url)
const target = new URL('../dist/codec/', import.meta.url)

rmSync(target, { recursive: true, force: true })
cpSync(source, target, { recursive: true, filter: (path) => !path.endsWith('.d.ts') })
writeFileSync(new URL('package.json', target), `${JSON.stringify({ type: 'module' })}\n`)
