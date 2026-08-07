#!/usr/bin/env node

import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const defaultCheckpoint = new URL('../.github/upstream-watch.json', import.meta.url)

function changed(before, after, fields) {
  return fields.some((field) => before[field] !== after[field])
}

function releaseDetail(before, after) {
  const oldState = before.prerelease ? 'prerelease' : 'stable'
  const newState = after.prerelease ? 'prerelease' : 'stable'
  return `${before.tag}@${before.revision} (${oldState}) -> ${after.tag}@${after.revision} (${newState})`
}

function itemChanges(before, after) {
  const changes = []
  if (before.state.status !== after.state.status) {
    let kind = 'state-changed'
    if (after.state.status === 'merged') kind = 'merged'
    if (after.state.status === 'closed') kind = 'closed'
    changes.push({ kind, detail: `${before.state.status} -> ${after.state.status}` })
  }
  if (before.revision !== after.revision) {
    let kind = 'updated'
    if (before.kind === 'pull_request') kind = after.forcePushed ? 'force-pushed' : 'head-changed'
    changes.push({ kind, detail: `${before.revision} -> ${after.revision}` })
  }
  if (before.state.draft !== after.state.draft) {
    changes.push({
      kind: 'draft-changed',
      detail: `${String(before.state.draft)} -> ${String(after.state.draft)}`,
    })
  }
  if (before.state.conflict !== after.state.conflict) {
    changes.push({
      kind: 'conflict-changed',
      detail: `${String(before.state.conflict)} -> ${String(after.state.conflict)}`,
    })
  }
  return changes
}

function requireEntry(collection, key, label) {
  const entry = collection[key]
  if (!entry) throw new Error(`snapshot is missing ${label} ${key}`)
  return entry
}

export function evaluateDrift(checkpoint, snapshot, options = {}) {
  const date = options.date ?? new Date().toISOString().slice(0, 10)
  const repositories = Object.entries(checkpoint.repositories).map(([repository, baseline]) => {
    const current = requireEntry(snapshot.repositories, repository, 'repository')
    const releaseChanged = changed(baseline.release, current.release, [
      'tag',
      'revision',
      'prerelease',
    ])
    const headChanged = baseline.head.revision !== current.head.revision
    const evidenceRevision = current.evidence?.revision ?? current.head.revision
    const rerunRequired = baseline.evidence.revision !== evidenceRevision
    return {
      repository,
      release: {
        changed: releaseChanged,
        detail: releaseDetail(baseline.release, current.release),
      },
      head: {
        changed: headChanged,
        detail: `${baseline.head.revision} -> ${current.head.revision}`,
      },
      conformance: {
        rerunRequired,
        detail: `${baseline.evidence.revision} -> ${evidenceRevision}`,
        paths: baseline.evidence.paths,
      },
    }
  })

  const watchlist = Object.entries(checkpoint.watchlist).map(([key, baseline]) => {
    const current = requireEntry(snapshot.items, key, 'watchlist item')
    const audited = {
      ...baseline,
      revision: baseline.lastAuditedRevision,
      state: baseline.lastAuditedState,
    }
    return {
      key,
      repository: baseline.repository,
      number: baseline.number,
      changes: itemChanges(audited, current),
      disposition: baseline.disposition,
      localImpact: baseline.localImpact,
      action: `${baseline.localImpact}; disposition: ${baseline.disposition}`,
    }
  })

  const repositoryDrift = repositories.some(
    ({ release, head, conformance }) =>
      release.changed || head.changed || conformance.rerunRequired,
  )
  return {
    date,
    auditedAt: checkpoint.auditedAt,
    repositories,
    watchlist,
    hasDrift: repositoryDrift || watchlist.some(({ changes }) => changes.length > 0),
  }
}

function marker(value) {
  return value ? 'DRIFT' : 'unchanged'
}

export function renderReport(report) {
  const lines = [`# Upstream drift report — ${report.date}`, '']
  if (!report.hasDrift) {
    lines.push(`No upstream drift detected against the ${report.auditedAt} audit checkpoint.`)
    return lines.join('\n')
  }

  lines.push(`Compared with the explicit audit checkpoint dated ${report.auditedAt}.`, '')
  for (const repository of report.repositories) {
    lines.push(`## ${repository.repository}`, '')
    lines.push(`- Released version: **${marker(repository.release.changed)}** — ${repository.release.detail}`)
    lines.push(`- Repository HEAD: **${marker(repository.head.changed)}** — ${repository.head.detail}`)
    const evidence = repository.conformance.rerunRequired ? 'RERUN REQUIRED' : 'current'
    lines.push(`- Local conformance evidence: **${evidence}** — ${repository.conformance.detail}`)
    lines.push(`  Evidence scope: ${repository.conformance.paths.join(', ')}`, '')
  }

  const changedItems = report.watchlist.filter(({ changes }) => changes.length > 0)
  if (changedItems.length > 0) {
    lines.push('## Watchlist actions', '')
    for (const item of changedItems) {
      const url = `https://github.com/${item.repository}/issues/${item.number}`
      const details = item.changes.map(({ kind, detail }) => `${kind}: ${detail}`).join('; ')
      lines.push(`- [${item.key}](${url}) — ${details}`)
      lines.push(`  Action: ${item.action}`)
    }
  }
  return lines.join('\n').trimEnd()
}

async function githubJson(path) {
  const headers = {
    Accept: 'application/vnd.github+json',
    'User-Agent': 'reddb-io-toon-upstream-drift-check',
    'X-GitHub-Api-Version': '2022-11-28',
  }
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`
  const response = await fetch(`https://api.github.com${path}`, { headers })
  if (!response.ok) throw new Error(`GitHub API ${path} returned HTTP ${response.status}`)
  return response.json()
}

async function collectRepository(repository, baseline) {
  const branch = baseline.branch ?? 'main'
  const [release, head] = await Promise.all([
    githubJson(`/repos/${repository}/releases/latest`),
    githubJson(`/repos/${repository}/commits/${encodeURIComponent(branch)}`),
  ])
  const releaseCommit = await githubJson(
    `/repos/${repository}/commits/${encodeURIComponent(release.tag_name)}`,
  )
  return {
    release: {
      tag: release.tag_name,
      revision: releaseCommit.sha,
      prerelease: release.prerelease,
    },
    head: { revision: head.sha },
    evidence: { revision: releaseCommit.sha },
  }
}

function conflictState(pull) {
  if (pull.mergeable === false || pull.mergeable_state === 'dirty') return true
  if (pull.mergeable === true) return false
  return null
}

async function collectItem(key, baseline) {
  const issue = await githubJson(`/repos/${baseline.repository}/issues/${baseline.number}`)
  if (!issue.pull_request) {
    return [
      key,
      {
        repository: baseline.repository,
        number: baseline.number,
        kind: 'issue',
        revision: issue.updated_at,
        state: { status: issue.state },
      },
    ]
  }
  const pull = await githubJson(`/repos/${baseline.repository}/pulls/${baseline.number}`)
  const status = pull.merged_at ? 'merged' : pull.state
  let forcePushed = false
  if (baseline.lastAuditedRevision !== pull.head.sha) {
    const comparison = await githubJson(
      `/repos/${baseline.repository}/compare/${baseline.lastAuditedRevision}...${pull.head.sha}`,
    )
    forcePushed = comparison.status === 'behind' || comparison.status === 'diverged'
  }
  return [
    key,
    {
      repository: baseline.repository,
      number: baseline.number,
      kind: 'pull_request',
      revision: pull.head.sha,
      forcePushed,
      state: { status, draft: pull.draft, conflict: conflictState(pull) },
    },
  ]
}

export async function collectLiveSnapshot(checkpoint) {
  const repositoryPairs = await Promise.all(
    Object.entries(checkpoint.repositories).map(async ([repository, baseline]) => [
      repository,
      await collectRepository(repository, baseline),
    ]),
  )
  const itemPairs = await Promise.all(
    Object.entries(checkpoint.watchlist).map(([key, baseline]) => collectItem(key, baseline)),
  )
  return { repositories: Object.fromEntries(repositoryPairs), items: Object.fromEntries(itemPairs) }
}

function parseArguments(arguments_) {
  const options = { checkpoint: defaultCheckpoint, snapshot: null, format: 'markdown', date: null }
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index]
    if (argument === '--checkpoint') options.checkpoint = arguments_[++index]
    else if (argument === '--snapshot') options.snapshot = arguments_[++index]
    else if (argument === '--format') options.format = arguments_[++index]
    else if (argument === '--date') options.date = arguments_[++index]
    else throw new Error(`unknown argument: ${argument}`)
  }
  if (!['markdown', 'json'].includes(options.format)) {
    throw new Error('--format must be markdown or json')
  }
  return options
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'))
}

async function main() {
  const options = parseArguments(process.argv.slice(2))
  const checkpoint = await readJson(options.checkpoint)
  const snapshot = options.snapshot ? await readJson(options.snapshot) : await collectLiveSnapshot(checkpoint)
  const report = evaluateDrift(checkpoint, snapshot, { date: options.date ?? undefined })
  process.stdout.write(`${options.format === 'json' ? JSON.stringify(report, null, 2) : renderReport(report)}\n`)
  process.exitCode = report.hasDrift ? 1 : 0
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().catch((error) => {
    process.stderr.write(`upstream drift check failed: ${error.message}\n`)
    process.exitCode = 2
  })
}
