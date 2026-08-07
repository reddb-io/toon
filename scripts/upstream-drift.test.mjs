import assert from 'node:assert/strict'
import test from 'node:test'

import { collectLiveSnapshot, evaluateDrift, renderReport } from './upstream-drift.mjs'

const repositories = {
  'toon-format/spec': {
    release: { tag: 'v4.1.1', revision: 'spec-release', prerelease: false },
    head: { revision: 'spec-head' },
    evidence: { revision: 'spec-head', paths: ['tests/fixtures'] },
  },
  'toon-format/toon': {
    release: { tag: 'v4.1.1', revision: 'toon-release', prerelease: false },
    head: { revision: 'toon-head' },
    evidence: { revision: 'toon-head', paths: ['reference implementation'] },
  },
}

const items = {
  'toon-format/spec#48': {
    repository: 'toon-format/spec',
    number: 48,
    kind: 'issue',
    revision: '2026-07-15T09:50:27Z',
    state: { status: 'open' },
  },
  'toon-format/spec#47': {
    repository: 'toon-format/spec',
    number: 47,
    kind: 'pull_request',
    revision: 'old-spec-pr-head',
    state: { status: 'open', draft: true, conflict: true },
  },
  'toon-format/toon#294': {
    repository: 'toon-format/toon',
    number: 294,
    kind: 'pull_request',
    revision: 'old-toon-pr-head',
    state: { status: 'open', draft: false, conflict: false },
  },
  'toon-format/toon#330': {
    repository: 'toon-format/toon',
    number: 330,
    kind: 'pull_request',
    revision: 'toon-330-head',
    state: { status: 'open', draft: false, conflict: false },
  },
}

function checkpoint() {
  return {
    auditedAt: '2026-08-07',
    repositories: structuredClone(repositories),
    watchlist: Object.fromEntries(
      Object.entries(items).map(([key, item]) => [
        key,
        {
          repository: item.repository,
          number: item.number,
          kind: item.kind,
          lastAuditedRevision: item.revision,
          lastAuditedState: structuredClone(item.state),
          disposition: 'maintainer review',
          localImpact: `Review local impact for ${key}`,
        },
      ]),
    ),
  }
}

function snapshot() {
  return {
    repositories: structuredClone(repositories),
    items: structuredClone(items),
  }
}

test('release, HEAD, and conformance checkpoint drift remain separate facts', () => {
  const current = snapshot()
  current.repositories['toon-format/spec'].release = {
    tag: 'v4.2.0',
    revision: 'new-spec-release',
    prerelease: false,
  }
  current.repositories['toon-format/spec'].head.revision = 'new-spec-head'
  current.repositories['toon-format/spec'].evidence.revision = 'new-spec-head'

  const report = evaluateDrift(checkpoint(), current, { date: '2026-08-08' })
  const spec = report.repositories.find(({ repository }) => repository === 'toon-format/spec')

  assert.equal(spec.release.changed, true)
  assert.equal(spec.head.changed, true)
  assert.equal(spec.conformance.rerunRequired, true)
  assert.match(spec.release.detail, /v4\.1\.1.*v4\.2\.0/)
  assert.match(spec.head.detail, /spec-head.*new-spec-head/)
  assert.deepEqual(spec.conformance.paths, ['tests/fixtures'])
})

test('an unchanged checkpoint stays clean when HEAD is ahead of released evidence', async () => {
  const audited = {
    auditedAt: '2026-08-07',
    repositories: {
      'toon-format/toon': {
        branch: 'main',
        release: { tag: 'v4.1.1', revision: 'released-sha', prerelease: false },
        head: { revision: 'newer-head-sha' },
        evidence: { revision: 'released-sha', paths: ['tests/fixtures'] },
      },
    },
    watchlist: {},
  }
  const responses = new Map([
    ['/repos/toon-format/toon/releases/latest', { tag_name: 'v4.1.1', prerelease: false }],
    ['/repos/toon-format/toon/commits/main', { sha: 'newer-head-sha' }],
    ['/repos/toon-format/toon/commits/v4.1.1', { sha: 'released-sha' }],
  ])
  const originalFetch = globalThis.fetch
  globalThis.fetch = async (url) => {
    const path = new URL(url).pathname
    assert.ok(responses.has(path), `unexpected GitHub request: ${path}`)
    return { ok: true, json: async () => responses.get(path) }
  }

  try {
    const current = await collectLiveSnapshot(audited)
    const report = evaluateDrift(audited, current, { date: '2026-08-08' })

    assert.equal(report.hasDrift, false)
    assert.equal(report.repositories[0].release.changed, false)
    assert.equal(report.repositories[0].head.changed, false)
    assert.equal(report.repositories[0].conformance.rerunRequired, false)
  } finally {
    globalThis.fetch = originalFetch
  }
})

test('watchlist decisions detect close, merge, force-push, draft, and conflict changes', () => {
  const current = snapshot()
  current.items['toon-format/spec#48'].state.status = 'closed'
  current.items['toon-format/spec#48'].revision = '2026-08-08T10:00:00Z'
  Object.assign(current.items['toon-format/spec#47'], {
    revision: 'merged-spec-pr-head',
    state: { status: 'merged', draft: false, conflict: false },
  })
  Object.assign(current.items['toon-format/toon#294'], {
    revision: 'force-pushed-head',
    forcePushed: true,
  })
  current.items['toon-format/toon#330'].state = {
    status: 'open',
    draft: true,
    conflict: true,
  }

  const report = evaluateDrift(checkpoint(), current, { date: '2026-08-08' })
  const kinds = report.watchlist.flatMap(({ changes }) => changes.map(({ kind }) => kind))

  assert.ok(kinds.includes('closed'))
  assert.ok(kinds.includes('merged'))
  assert.ok(kinds.includes('updated'))
  assert.ok(kinds.includes('head-changed'))
  assert.ok(kinds.includes('force-pushed'))
  assert.ok(kinds.includes('draft-changed'))
  assert.ok(kinds.includes('conflict-changed'))
  assert.ok(report.watchlist.every(({ action }) => action.includes('Review local impact')))
  assert.equal(checkpoint().watchlist['toon-format/spec#48'].lastAuditedState.status, 'open')
})

test('release-state changes and a clean snapshot render a deterministic dated report', () => {
  const changed = snapshot()
  changed.repositories['toon-format/toon'].release.prerelease = true
  const changedReport = evaluateDrift(checkpoint(), changed, { date: '2026-08-08' })
  assert.equal(
    changedReport.repositories.find(({ repository }) => repository === 'toon-format/toon')
      .release.changed,
    true,
  )

  const cleanReport = evaluateDrift(checkpoint(), snapshot(), { date: '2026-08-08' })
  assert.equal(cleanReport.hasDrift, false)
  assert.equal(
    renderReport(cleanReport),
    [
      '# Upstream drift report — 2026-08-08',
      '',
      'No upstream drift detected against the 2026-08-07 audit checkpoint.',
    ].join('\n'),
  )
})

test('the experimental reviver frontier flags both PR merge and released-successor drift', () => {
  const audited = checkpoint()
  const current = snapshot()
  current.items['toon-format/toon#294'].state.status = 'merged'
  current.repositories['toon-format/toon'].release = {
    tag: 'v4.2.0',
    revision: 'reviver-release',
    prerelease: false,
  }
  current.repositories['toon-format/toon'].evidence.revision = 'reviver-release'

  const report = evaluateDrift(audited, current, { date: '2026-08-08' })
  const pull = report.watchlist.find(({ key }) => key === 'toon-format/toon#294')
  const repository = report.repositories.find(
    ({ repository: name }) => name === 'toon-format/toon',
  )

  assert.ok(pull.changes.some(({ kind }) => kind === 'merged'))
  assert.equal(repository.release.changed, true)
  assert.equal(repository.conformance.rerunRequired, true)
  assert.equal(report.hasDrift, true)
})
