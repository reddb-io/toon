#!/usr/bin/env node

import { execFile } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { promisify } from 'node:util'
import { fileURLToPath } from 'node:url'

const execFileAsync = promisify(execFile)
const issueMarker = '<!-- upstream-release-watch -->'
const issueTitle = '[upstream-watch] Upstream release drift'
const packagePaths = {
  '@toon-format/toon': 'vendor/toon/packages/toon/package.json',
  '@toon-format/cli': 'vendor/toon/packages/cli/package.json',
}

async function git(arguments_, cwd = '.') {
  const { stdout } = await execFileAsync('git', arguments_, { cwd, encoding: 'utf8' })
  return stdout.trim()
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'))
}

async function fetchJson(url, options = {}) {
  const response = await fetch(url, options)
  if (!response.ok) throw new Error(`${url} returned HTTP ${response.status}`)
  if (response.status === 204) return null
  return response.json()
}

async function collectLocalState() {
  const packagePairs = await Promise.all(
    Object.entries(packagePaths).map(async ([name, path]) => {
      const manifest = await readJson(path)
      if (manifest.name !== name) throw new Error(`${path} does not describe ${name}`)
      return [name, manifest.version]
    }),
  )
  return {
    toon: {
      pin: await git(['rev-parse', 'HEAD:vendor/toon']),
      packages: Object.fromEntries(packagePairs),
    },
    spec: { pin: await git(['rev-parse', 'HEAD:vendor/toon-spec']) },
  }
}

async function collectNewTags(pin) {
  await git(
    ['fetch', '--quiet', '--tags', 'origin', '+refs/heads/main:refs/remotes/origin/main'],
    'vendor/toon',
  )
  const names = await git(
    ['tag', '--merged', 'origin/main', '--no-merged', pin, '--sort=-version:refname'],
    'vendor/toon',
  )
  if (!names) return []
  return Promise.all(
    names.split('\n').map(async (name) => ({
      name,
      revision: await git(['rev-list', '-n', '1', name], 'vendor/toon'),
      aheadOfPin: true,
    })),
  )
}

async function collectNpmDistTags() {
  const pairs = await Promise.all(
    Object.keys(packagePaths).map(async (name) => {
      const encoded = encodeURIComponent(name)
      return [name, await fetchJson(`https://registry.npmjs.org/-/package/${encoded}/dist-tags`)]
    }),
  )
  return Object.fromEntries(pairs)
}

async function collectSpec(pin) {
  await git(
    ['fetch', '--quiet', 'origin', '+refs/heads/main:refs/remotes/origin/main'],
    'vendor/toon-spec',
  )
  const head = await git(['rev-parse', 'origin/main'], 'vendor/toon-spec')
  const log = await git(['log', '-z', '--format=%H%x00%s', `${pin}..origin/main`], 'vendor/toon-spec')
  const fields = log.split('\0').map((field) => field.trim()).filter(Boolean)
  const commits = []
  for (let index = 0; index < fields.length; index += 2) {
    commits.push({ revision: fields[index], subject: fields[index + 1] })
  }
  return { head, commits }
}

export async function collectReleaseWatchSnapshot(local) {
  const [tags, npm, spec] = await Promise.all([
    collectNewTags(local.toon.pin),
    collectNpmDistTags(),
    collectSpec(local.spec.pin),
  ])
  return { toon: { tags }, npm, spec }
}

export function evaluateReleaseWatch(local, snapshot, options = {}) {
  const tags = snapshot.toon.tags
    .filter(({ aheadOfPin }) => aheadOfPin)
    .map(({ name, revision }) => ({ name, revision }))
  const npm = []
  for (const name of Object.keys(packagePaths).sort()) {
    const pinned = local.toon.packages[name]
    const distTags = snapshot.npm[name] ?? {}
    for (const [distTag, version] of Object.entries(distTags).sort(([left], [right]) =>
      left.localeCompare(right),
    )) {
      if (version !== pinned) npm.push({ name, distTag, pinned, version })
    }
  }
  const spec = {
    pin: local.spec.pin,
    head: snapshot.spec.head,
    commits: snapshot.spec.commits,
  }
  return {
    date: options.date ?? new Date().toISOString().slice(0, 10),
    local,
    tags,
    npm,
    spec,
    hasDrift: tags.length > 0 || npm.length > 0 || spec.head !== spec.pin,
  }
}

function short(revision) {
  return revision.length === 40 ? revision.slice(0, 12) : revision
}

export function renderTrackingIssue(report) {
  const lines = [
    issueMarker,
    '# Upstream release drift',
    '',
    `Observed on ${report.date} against the checked-in vendor pins.`,
    '',
    `- \`vendor/toon\`: \`${short(report.local.toon.pin)}\``,
    `- \`vendor/toon-spec\`: \`${short(report.local.spec.pin)}\``,
    '',
    `## Delta log — ${report.date}`,
    '',
    '### New upstream git tags',
    '',
  ]
  if (report.tags.length === 0) lines.push('- None')
  for (const tag of report.tags) lines.push(`- \`${tag.name}\` at \`${short(tag.revision)}\``)

  lines.push('', '### npm dist-tags ahead of or different from the vendor package versions', '')
  if (report.npm.length === 0) lines.push('- None')
  for (const delta of report.npm) {
    lines.push(
      `- \`${delta.name}\` \`${delta.distTag}\`: \`${delta.pinned}\` -> \`${delta.version}\``,
    )
  }

  lines.push('', '### Spec main commits after the vendor pin', '')
  if (report.spec.commits.length === 0) lines.push('- None')
  for (const commit of report.spec.commits) {
    lines.push(`- \`${short(commit.revision)}\` ${commit.subject}`)
  }
  lines.push(
    '',
    'No dependency pins were changed automatically. Maintainers decide whether and when to move them.',
  )
  return { title: issueTitle, body: lines.join('\n') }
}

export async function upsertTrackingIssue(github, repository, issue) {
  const response = await github.rest.issues.listForRepo({
    ...repository,
    state: 'all',
    per_page: 100,
    sort: 'updated',
    direction: 'desc',
  })
  const existing = response.data.find(
    ({ title, body }) => title === issueTitle || body?.includes(issueMarker),
  )
  if (existing) {
    await github.rest.issues.update({
      ...repository,
      issue_number: existing.number,
      title: issue.title,
      body: issue.body,
      state: 'open',
    })
    return { action: 'updated', number: existing.number }
  }
  const created = await github.rest.issues.create({ ...repository, ...issue })
  return { action: 'created', number: created.data.number }
}

function githubClient(token) {
  const request = async (method, path, body) => {
    const headers = {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
      'User-Agent': 'reddb-io-toon-upstream-release-watch',
      'X-GitHub-Api-Version': '2022-11-28',
    }
    const data = await fetchJson(`https://api.github.com${path}`, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
    })
    return { data }
  }
  return {
    rest: {
      issues: {
        listForRepo: ({ owner, repo, state, per_page: perPage, sort, direction }) =>
          request(
            'GET',
            `/repos/${owner}/${repo}/issues?state=${state}&per_page=${perPage}&sort=${sort}&direction=${direction}`,
          ),
        create: ({ owner, repo, ...body }) => request('POST', `/repos/${owner}/${repo}/issues`, body),
        update: ({ owner, repo, issue_number: number, ...body }) =>
          request('PATCH', `/repos/${owner}/${repo}/issues/${number}`, body),
      },
    },
  }
}

function parseArguments(arguments_) {
  const options = { date: undefined, writeIssue: false, repository: process.env.GITHUB_REPOSITORY }
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index]
    if (argument === '--date') options.date = arguments_[++index]
    else if (argument === '--write-issue') options.writeIssue = true
    else if (argument === '--repository') options.repository = arguments_[++index]
    else throw new Error(`unknown argument: ${argument}`)
  }
  return options
}

async function main() {
  const options = parseArguments(process.argv.slice(2))
  const local = await collectLocalState()
  const snapshot = await collectReleaseWatchSnapshot(local)
  const report = evaluateReleaseWatch(local, snapshot, { date: options.date })
  if (!report.hasDrift) {
    process.stdout.write('No upstream release drift detected against the checked-in vendor pins.\n')
    return
  }
  const issue = renderTrackingIssue(report)
  process.stdout.write(`${issue.body}\n`)
  if (!options.writeIssue) {
    process.exitCode = 1
    return
  }
  if (!options.repository?.includes('/')) throw new Error('GITHUB_REPOSITORY must be owner/repo')
  if (!process.env.GITHUB_TOKEN) throw new Error('GITHUB_TOKEN is required with --write-issue')
  const [owner, repo] = options.repository.split('/')
  const result = await upsertTrackingIssue(githubClient(process.env.GITHUB_TOKEN), { owner, repo }, issue)
  process.stdout.write(`Tracking issue #${result.number} ${result.action}.\n`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().catch((error) => {
    process.stderr.write(`upstream release watch failed: ${error.message}\n`)
    process.exitCode = 2
  })
}
