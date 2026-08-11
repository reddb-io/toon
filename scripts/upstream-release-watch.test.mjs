import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import {
  evaluateReleaseWatch,
  renderTrackingIssue,
  upsertTrackingIssue,
} from './upstream-release-watch.mjs'

const local = {
  toon: {
    pin: 'toon-pin',
    packages: {
      '@toon-format/toon': '2.0.0',
      '@toon-format/cli': '2.0.0',
    },
  },
  spec: { pin: 'spec-pin' },
}

function cleanSnapshot() {
  return {
    toon: {
      tags: [{ name: 'v2.0.0', revision: 'toon-pin', aheadOfPin: false }],
    },
    npm: {
      '@toon-format/toon': { latest: '2.0.0' },
      '@toon-format/cli': { latest: '2.0.0' },
    },
    spec: { head: 'spec-pin', commits: [] },
  }
}

function driftSnapshot() {
  const snapshot = cleanSnapshot()
  snapshot.toon.tags.unshift({
    name: 'v2.1.0',
    revision: 'toon-tag-next',
    aheadOfPin: true,
  })
  snapshot.npm['@toon-format/toon'] = { latest: '2.1.0', next: '2.2.0-beta.1' }
  snapshot.npm['@toon-format/cli'] = { latest: '2.1.0' }
  snapshot.spec = {
    head: 'spec-head-next',
    commits: [
      { revision: 'spec-change-1', subject: 'Clarify delimiter selection' },
      { revision: 'spec-head-next', subject: 'Add conformance example' },
    ],
  }
  return snapshot
}

test('a snapshot aligned with the vendor pins is clean', () => {
  const report = evaluateReleaseWatch(local, cleanSnapshot(), { date: '2026-08-11' })

  assert.equal(report.hasDrift, false)
  assert.deepEqual(report.tags, [])
  assert.deepEqual(report.npm, [])
  assert.deepEqual(report.spec.commits, [])
})

test('simulated drift renders tag, npm dist-tag, and spec commit deltas', () => {
  const report = evaluateReleaseWatch(local, driftSnapshot(), { date: '2026-08-11' })
  const issue = renderTrackingIssue(report)

  assert.equal(report.hasDrift, true)
  assert.match(issue.body, /v2\.1\.0.*toon-tag-next/)
  assert.match(issue.body, /@toon-format\/toon.*latest.*2\.0\.0.*2\.1\.0/)
  assert.match(issue.body, /@toon-format\/toon.*next.*2\.2\.0-beta\.1/)
  assert.match(issue.body, /@toon-format\/cli.*latest.*2\.0\.0.*2\.1\.0/)
  assert.match(issue.body, /spec-change-1.*Clarify delimiter selection/)
  assert.match(issue.body, /spec-head-next.*Add conformance example/)
  assert.match(issue.body, /No dependency pins were changed automatically/)
})

test('duplicate drift runs update the single tracking issue', async () => {
  const calls = []
  const github = {
    rest: {
      issues: {
        listForRepo: async () => ({
          data: [
            {
              number: 73,
              title: '[upstream-watch] Upstream release drift',
              body: '<!-- upstream-release-watch -->\nold report',
              state: 'open',
            },
          ],
        }),
        create: async (input) => calls.push(['create', input]),
        update: async (input) => calls.push(['update', input]),
      },
    },
  }
  const issue = renderTrackingIssue(
    evaluateReleaseWatch(local, driftSnapshot(), { date: '2026-08-11' }),
  )

  const result = await upsertTrackingIssue(github, { owner: 'reddb-io', repo: 'toon' }, issue)

  assert.deepEqual(result, { action: 'updated', number: 73 })
  assert.equal(calls.length, 1)
  assert.equal(calls[0][0], 'update')
  assert.equal(calls[0][1].issue_number, 73)
  assert.match(calls[0][1].body, /v2\.1\.0/)
})

test('the standalone workflow runs weekly and can upsert the tracking issue', async () => {
  const workflow = await readFile('.github/workflows/upstream-release-watch.yml', 'utf8')

  assert.match(workflow, /^name: Upstream release watch$/m)
  assert.match(workflow, /workflow_dispatch:/)
  assert.match(workflow, /cron: ['"]17 9 \* \* 1['"]/)
  assert.match(workflow, /issues: write/)
  assert.match(workflow, /submodules: true/)
  assert.match(workflow, /node scripts\/upstream-release-watch\.mjs --write-issue/)
})
