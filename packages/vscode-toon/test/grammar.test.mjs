import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const pkgRoot = path.join(path.dirname(fileURLToPath(import.meta.url)), '..')
const readJson = (rel) => JSON.parse(readFileSync(path.join(pkgRoot, rel), 'utf8'))

const manifest = readJson('package.json')
const toonGrammar = readJson('syntaxes/toon.tmLanguage.json')
const toonlGrammar = readJson('syntaxes/toonl.tmLanguage.json')
const injectionGrammar = readJson('syntaxes/toon-markdown-injection.tmLanguage.json')
const grammars = { toonGrammar, toonlGrammar, injectionGrammar }

// TextMate grammars run on Oniguruma. The two language grammars stick to the
// JS-compatible subset so `new RegExp` doubles as a syntax smoke test; the
// markdown injection grammar uses two Oniguruma-only constructs that this
// strips before compiling (the \G anchor and `(?i:` modifier groups).
const jsCompilable = (source) => source.replaceAll('\\G', '').replaceAll('(?i:', '(?:')

function* walkRules(node, trail = '$') {
  if (Array.isArray(node)) {
    for (const [i, item] of node.entries()) yield* walkRules(item, `${trail}[${i}]`)
    return
  }
  if (node === null || typeof node !== 'object') return
  yield { rule: node, trail }
  for (const key of ['patterns', 'repository', 'captures', 'beginCaptures', 'endCaptures']) {
    if (key in node) yield* walkRules(node[key], `${trail}.${key}`)
  }
  if (node.repository === undefined && node.patterns === undefined) {
    for (const value of Object.values(node)) {
      if (value && typeof value === 'object') yield* walkRules(value, trail)
    }
  }
}

const rx = (grammar, key, field = 'match') => {
  const rule = grammar.repository[key]
  assert.ok(rule, `repository entry ${key} exists`)
  assert.ok(rule[field], `repository entry ${key} has a ${field} regex`)
  return new RegExp(jsCompilable(rule[field]))
}

test('every grammar regex compiles as a JS RegExp after Oniguruma stripping', () => {
  for (const [name, grammar] of Object.entries(grammars)) {
    for (const { rule, trail } of walkRules(grammar)) {
      for (const field of ['match', 'begin', 'end', 'while']) {
        if (typeof rule[field] !== 'string') continue
        assert.doesNotThrow(
          () => new RegExp(jsCompilable(rule[field])),
          `${name} ${trail}.${field}: ${rule[field]}`
        )
      }
    }
  }
})

test('every #include resolves to a repository entry or a known scope', () => {
  const externalScopes = new Set(['source.toon', 'source.toonl'])
  for (const [name, grammar] of Object.entries(grammars)) {
    const repositoryKeys = new Set(Object.keys(grammar.repository ?? {}))
    for (const { rule, trail } of walkRules(grammar)) {
      if (typeof rule.include !== 'string') continue
      if (rule.include.startsWith('#')) {
        assert.ok(
          repositoryKeys.has(rule.include.slice(1)),
          `${name} ${trail}: unresolved include ${rule.include}`
        )
      } else {
        assert.ok(externalScopes.has(rule.include), `${name} ${trail}: unknown scope ${rule.include}`)
      }
    }
  }
})

test('manifest wires languages, grammars, and configurations consistently', () => {
  const languages = manifest.contributes.languages
  assert.deepEqual(languages.map((l) => l.id).sort(), ['toon', 'toonl'])
  for (const language of languages) {
    assert.ok(existsSync(path.join(pkgRoot, language.configuration)), language.configuration)
    const configuration = readJson(language.configuration)
    // TOON and TOONL have no comment syntax; a lineComment here would be a spec bug.
    assert.equal(configuration.comments, undefined, `${language.id} must not declare comments`)
  }
  const grammarEntries = manifest.contributes.grammars
  for (const entry of grammarEntries) {
    assert.ok(existsSync(path.join(pkgRoot, entry.path)), entry.path)
    const grammar = readJson(entry.path)
    assert.equal(grammar.scopeName, entry.scopeName, entry.path)
  }
  const byLanguage = Object.fromEntries(grammarEntries.filter((g) => g.language).map((g) => [g.language, g.scopeName]))
  assert.deepEqual(byLanguage, { toon: 'source.toon', toonl: 'source.toonl' })
  const injection = grammarEntries.find((g) => g.injectTo)
  assert.deepEqual(injection.injectTo, ['text.html.markdown'])
  const languageIds = new Set(languages.map((l) => l.id))
  for (const id of Object.values(injection.embeddedLanguages)) {
    assert.ok(languageIds.has(id), `embedded language ${id} is declared`)
  }
})

test('toon: array headers match every header shape from the specs', () => {
  const header = rx(toonGrammar, 'array-header')
  const matching = [
    'users[2]{id,name,active}:',
    'items[3]: apple,banana,cherry',
    '[2]{id,name}:',
    '[2]:',
    '"my-key"[3]: a,b,c',
    'piped[2|]{name|status}:',
    'data[2\t]{id\tvalue}:',
    '  - [2]: 1,2',
    '- items[2]{id,qty}:',
    'orders[2]{id,customer{name,country},total}:',
    'shipment[2]{id,tags[;],quantity}:',
    'grid[2|]{values[3|]}:',
    'fulfilment[2|]{id|customer|items{sku|quantity}}:',
    'common[6|]{tenant|seq}:',
    'legacy[0]:'
  ]
  for (const line of matching) assert.ok(header.test(line), `header matches: ${line}`)
  const nonMatching = ['key: value', '1,Alice,true', 'people{first,last}:', '[]']
  for (const line of nonMatching) assert.ok(!header.test(line), `header rejects: ${line}`)
})

test('toon: keyed-map collapse headers (Extension 2) match', () => {
  const keyedMap = rx(toonGrammar, 'keyed-map-header')
  for (const line of ['people{first,last}:', 'map{|id|name}:', '"odd key"{a,b}:']) {
    assert.ok(keyedMap.test(line), `keyed-map matches: ${line}`)
  }
  for (const line of ['users[2]{id,name}:', 'key: value', 'joe: Joe,Schmoe']) {
    assert.ok(!keyedMap.test(line), `keyed-map rejects: ${line}`)
  }
})

test('toon: key-value lines match, headers and rows do not', () => {
  const keyValue = rx(toonGrammar, 'key-value', 'begin')
  for (const line of ['id: 123', 'server.host: localhost', 'config:', '"my key": 1', 'joe: Joe,Schmoe']) {
    assert.ok(keyValue.test(line), `key-value matches: ${line}`)
  }
  for (const line of ['users[2]:', '1,Alice,true', '95,87,92']) {
    assert.ok(!keyValue.test(line), `key-value rejects: ${line}`)
  }
})

test('toon: numbers follow the canonical form, leading-zero tokens stay strings', () => {
  const number = rx(toonGrammar, 'number')
  for (const token of ['0', '123', '-1.5', '1000000', '0.000001', '1e-3', '-1E+03', '98.5']) {
    assert.ok(number.test(token), `number matches: ${token}`)
  }
  for (const token of ['05', '0001', '-05', '1.2.3', '2026-07-14', 'v1.2', 'a3']) {
    assert.ok(!number.test(token), `number rejects: ${token}`)
  }
})

test('toon: constants are whole-token true/false/null', () => {
  const constant = rx(toonGrammar, 'constant')
  for (const token of ['true', 'false', 'null', '1,Alice,true']) {
    assert.ok(constant.test(token), `constant matches in: ${token}`)
  }
  for (const token of ['truely', 'nullable', 'x-true', '"true"']) {
    assert.ok(!constant.test(token), `constant rejects: ${token}`)
  }
})

test('toon: cycle expressions (Extension 5) match', () => {
  const cycle = rx(toonGrammar, 'cycle')
  assert.ok(cycle.test('order: cycle(login,purchase,logout)*2'))
  assert.ok(cycle.test('cycle(a,b)*3'))
  assert.ok(!cycle.test('cycle stuff'))
  assert.ok(!cycle.test('recycle(a)*2'))
})

test('toonl: the four structural line forms match exactly', () => {
  const segmentHeader = rx(toonlGrammar, 'segment-header')
  for (const line of ['[]{ts,level,msg}:', '[|]{name|value}:', '[\t]{a\tb\tc}:', '[2]{id,msg}:']) {
    assert.ok(segmentHeader.test(line), `segment header matches: ${line}`)
  }
  for (const line of ['{ts,level,msg}:', '[~]{ts}:', '[]<audit>{a}:', '[=2]', 'key: value']) {
    assert.ok(!segmentHeader.test(line), `segment header rejects: ${line}`)
  }

  const declaration = rx(toonlGrammar, 'schema-declaration')
  assert.ok(declaration.test('[]<audit>{actor,action}:'))
  assert.ok(declaration.test('[]<metric-2>{name,value}:'))
  assert.ok(!declaration.test('[]<au dit>{a}:'))
  assert.ok(!declaration.test('[]{a,b}:'))

  const continuation = rx(toonlGrammar, 'continuation-header')
  assert.ok(continuation.test('[~]{ts,level,msg}:'))
  assert.ok(!continuation.test('[]{ts,level,msg}:'))

  const trailer = rx(toonlGrammar, 'trailer')
  assert.ok(trailer.test('[=2]'))
  assert.ok(trailer.test('[=0]'))
  for (const line of ['[=]', '[= 2]', '[2]', '[=2]{a}:']) {
    assert.ok(!trailer.test(line), `trailer rejects: ${line}`)
  }
})

test('toonl: reserved `- ` lines and tagged rows are recognized', () => {
  const reserved = rx(toonlGrammar, 'reserved-line')
  assert.ok(reserved.test('- reserved for future use'))
  assert.ok(!reserved.test('-5,x'))

  const taggedRow = rx(toonlGrammar, 'tagged-row')
  const match = 'audit:u1,delete'.match(taggedRow)
  assert.ok(match)
  assert.equal(match[1], 'audit')
  assert.ok(!taggedRow.test('"quoted",start'))
  assert.ok(!taggedRow.test('[=2]'))
})

test('fixtures parse against the structural patterns they showcase', () => {
  const toonSample = readFileSync(path.join(pkgRoot, 'examples/sample.toon'), 'utf8').split('\n')
  const header = rx(toonGrammar, 'array-header')
  assert.ok(toonSample.filter((line) => header.test(line)).length >= 10, 'sample.toon exercises headers')
  const keyedMap = rx(toonGrammar, 'keyed-map-header')
  assert.ok(toonSample.some((line) => keyedMap.test(line)), 'sample.toon exercises keyed-map collapse')
  const cycle = rx(toonGrammar, 'cycle')
  assert.ok(toonSample.some((line) => cycle.test(line)), 'sample.toon exercises cycle expressions')

  const toonlSample = readFileSync(path.join(pkgRoot, 'examples/sample.toonl'), 'utf8').split('\n')
  for (const key of ['segment-header', 'schema-declaration', 'continuation-header', 'trailer', 'tagged-row']) {
    const pattern = rx(toonlGrammar, key)
    assert.ok(toonlSample.some((line) => pattern.test(line)), `sample.toonl exercises ${key}`)
  }
})
