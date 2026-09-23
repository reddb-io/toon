import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import test from 'node:test'

import * as codec from '../../toon/dist/index.js'
import { estimateTokenCount } from '../../toon/dist/cli/tokens.js'

const require = createRequire(import.meta.url)
const core = require('../lib/core.cjs')
const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))

test('valid documents raise no diagnostic', () => {
  assert.equal(core.validate(codec, 'toon', 'users[2]{id,name}:\n  1,Ada\n  2,Linus\n'), null)
  assert.equal(core.validate(codec, 'toonl', '[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n'), null)
})

test('a decode error becomes a range on its line, at its column when known', () => {
  const indentation = core.validate(codec, 'toon', 'a:\n   b: 1\n')
  assert.deepEqual(indentation, {
    line: 1,
    start: 3,
    end: 7,
    message: 'invalid indentation',
    code: 'indentation',
  })

  const mismatch = core.validate(codec, 'toon', 'name: Ada\n  \ntags[3]: a,b\n')
  assert.equal(mismatch.line, 2)
  assert.equal(mismatch.start, 0)
  assert.equal(mismatch.code, 'length-mismatch')

  assert.equal(core.validate(codec, 'toonl', '[]{id,name}:\n1,Ada,extra\n').line, 1)
})

test('formatting re-encodes canonically and refuses to drop comments', () => {
  assert.deepEqual(core.formatToon(codec, 'a:   1\nlist[2]:   x,y\n', { indentSize: 2 }), { text: 'a: 1\nlist[2]: x,y\n' })
  assert.deepEqual(core.formatToon(codec, 'a: 1', { indentSize: 2 }), { text: 'a: 1' })
  assert.match(core.formatToon(codec, '# note\na: 1\n').skipped, /comment/)
  assert.throws(() => core.formatToon(codec, 'a:\n   b: 1'))
})

test('conversions go both ways, TOONL streams included', () => {
  assert.equal(core.jsonToToon(codec, '{"a":[1,2]}'), 'a[2]: 1,2')
  assert.equal(core.jsonToToon(codec, '{"a":[1,2]}', { delimiter: '|' }), 'a[2|]: 1|2')
  assert.equal(core.toonToJson(codec, 'a[2]: 1,2'), '{\n  "a": [\n    1,\n    2\n  ]\n}\n')
  assert.deepEqual(JSON.parse(core.toonToJson(codec, '[]{id}:\n1\n2\n', 'toonl')), [{ id: 1 }, { id: 2 }])
})

test('the status label shows size and tokens, or the TOON saving for JSON', () => {
  assert.equal(core.statusText(codec, estimateTokenCount, 'toon', 'a: 1'), '4 B · ~3 tok')
  const rows = JSON.stringify({ users: Array.from({ length: 20 }, (_, id) => ({ id, name: `user${id}` })) })
  assert.match(core.statusText(codec, estimateTokenCount, 'json', rows), /^TOON -\d+% tokens$/)
  assert.equal(core.statusText(codec, estimateTokenCount, 'json', '{broken'), undefined)
})

test('the manifest wires the entry point, activation, commands and settings', () => {
  assert.equal(manifest.main, './extension.cjs')
  assert.ok(existsSync(new URL('../extension.cjs', import.meta.url)))
  assert.deepEqual(manifest.activationEvents, ['onLanguage:toon', 'onLanguage:toonl', 'onLanguage:json'])
  assert.deepEqual(
    manifest.contributes.commands.map((command) => command.command),
    ['reddbToon.convertJsonToToon', 'reddbToon.convertToonToJson'],
  )
  assert.deepEqual(Object.keys(manifest.contributes.configuration.properties), ['reddbToon.validate', 'reddbToon.delimiter'])
  assert.equal(manifest.scripts.build, 'node scripts/vendor-codec.mjs')
})
