import { strict as assert } from 'node:assert'
import {
  cpSync,
  readdirSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { after, test } from 'node:test'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const temporaryDirectories = []

after(() => {
  for (const directory of temporaryDirectories) {
    rmSync(directory, { force: true, recursive: true })
  }
})

function temporaryDirectory(prefix) {
  const directory = mkdtempSync(join(tmpdir(), prefix))
  temporaryDirectories.push(directory)
  return directory
}

function copyFixture(paths) {
  const directory = temporaryDirectory('toon-version-')
  for (const path of paths) {
    const destination = join(directory, path)
    mkdirSync(dirname(destination), { recursive: true })
    cpSync(join(root, path), destination, { recursive: true })
  }
  return directory
}

function run(command, args, cwd, options = {}) {
  return spawnSync(command, args, {
    cwd,
    encoding: 'utf8',
    ...options,
  })
}

const versionFixturePaths = [
  'Cargo.lock',
  'Cargo.toml',
  'crates/toon/Cargo.toml',
  'crates/tq/Cargo.toml',
  'package.json',
  'packages/toon/dist/version.js',
  'packages/toon/package.json',
  'packages/toon/src/version.ts',
  'packages/vscode-toon/package.json',
  'scripts/check-versions.sh',
  'scripts/sync-version.sh',
  'scripts/workspace-members.sh',
]

function syncedFixture(version = '9.8.7') {
  const directory = copyFixture(versionFixturePaths)
  const result = run('bash', ['scripts/sync-version.sh', version], directory)
  assert.equal(result.status, 0, result.stderr || result.stdout)
  return directory
}

function text(directory, path) {
  return readFileSync(join(directory, path), 'utf8')
}

function unknownWorkflowDependencies(source) {
  const jobs = new Set()
  const dependencies = []
  let currentJob = ''
  let inJobs = false

  for (const line of source.split('\n')) {
    if (line === 'jobs:') {
      inJobs = true
      continue
    }
    if (!inJobs) continue
    if (/^\S/.test(line)) break

    const job = line.match(/^  ([a-zA-Z0-9_-]+):\s*$/)
    if (job) {
      currentJob = job[1]
      jobs.add(currentJob)
      continue
    }

    const needs = line.match(/^    needs:\s*\[([^\]]+)\]\s*$/)
    if (needs) {
      for (const dependency of needs[1].split(',').map((name) => name.trim())) {
        dependencies.push({ job: currentJob, dependency })
      }
    }
  }

  return dependencies.filter(({ dependency }) => !jobs.has(dependency))
}

test('GitHub Actions jobs depend only on jobs declared by their workflow', () => {
  const workflowDirectory = join(root, '.github/workflows')
  for (const filename of readdirSync(workflowDirectory).filter((name) => name.endsWith('.yml'))) {
    assert.deepEqual(
      unknownWorkflowDependencies(text(workflowDirectory, filename)),
      [],
      `${filename} contains an unknown job dependency`,
    )
  }
})

test('workspace versioning keeps source, generated output, crates, and manifests in lockstep', () => {
  const directory = syncedFixture()

  assert.match(text(directory, 'Cargo.toml'), /^version = "9\.8\.7"$/m)
  assert.match(
    text(directory, 'crates/tq/Cargo.toml'),
    /reddb-io-toon = \{ path = "\.\.\/toon", version = "9\.8\.7"/,
  )
  assert.match(text(directory, 'Cargo.lock'), /name = "reddb-io-toon"\nversion = "9\.8\.7"/)
  assert.match(text(directory, 'Cargo.lock'), /name = "reddb-io-tq"\nversion = "9\.8\.7"/)
  assert.equal(JSON.parse(text(directory, 'package.json')).version, '9.8.7')
  assert.equal(JSON.parse(text(directory, 'packages/toon/package.json')).version, '9.8.7')
  assert.equal(JSON.parse(text(directory, 'packages/vscode-toon/package.json')).version, '9.8.7')
  assert.match(text(directory, 'packages/toon/src/version.ts'), /VERSION = '9\.8\.7'/)
  assert.match(text(directory, 'packages/toon/dist/version.js'), /VERSION = '9\.8\.7'/)
})

for (const drift of [
  { name: 'TypeScript source', path: 'packages/toon/src/version.ts' },
  { name: 'generated JavaScript', path: 'packages/toon/dist/version.js' },
  { name: 'package manifest', path: 'packages/toon/package.json' },
  { name: 'workspace dependency', path: 'crates/tq/Cargo.toml' },
]) {
  test(`version check rejects drift in ${drift.name}`, () => {
    const directory = syncedFixture()
    const path = join(directory, drift.path)
    writeFileSync(path, readFileSync(path, 'utf8').replace('9.8.7', '9.8.6'))

    const result = run('bash', ['scripts/check-versions.sh'], directory)
    assert.notEqual(result.status, 0, 'drift unexpectedly passed the version check')
    assert.match(result.stderr, /version drift:/)
  })
}

test('automatic release planning selects the intended next version without changing tracked files', () => {
  const directory = temporaryDirectory('toon-release-plan-')
  mkdirSync(join(directory, 'scripts'))
  cpSync(join(root, 'scripts/plan-auto-release.sh'), join(directory, 'scripts/plan-auto-release.sh'))

  for (const args of [
    ['init', '-q'],
    ['config', 'user.name', 'Release Test'],
    ['config', 'user.email', 'release-test@example.invalid'],
  ]) {
    const result = run('git', args, directory)
    assert.equal(result.status, 0, result.stderr)
  }
  writeFileSync(join(directory, 'README.md'), 'baseline\n')
  assert.equal(run('git', ['add', 'README.md'], directory).status, 0)
  assert.equal(run('git', ['commit', '-qm', 'chore: release 0.13.2'], directory).status, 0)
  assert.equal(run('git', ['tag', 'v0.13.2'], directory).status, 0)
  writeFileSync(join(directory, 'README.md'), 'baseline\nv4.1\n')
  assert.equal(run('git', ['commit', '-qam', 'feat: implement TOON v4.1'], directory).status, 0)
  assert.equal(run('git', ['add', 'scripts/plan-auto-release.sh'], directory).status, 0)
  assert.equal(run('git', ['commit', '-qm', 'test: install release planner'], directory).status, 0)

  const before = run('git', ['status', '--short', '--untracked-files=no'], directory)
  const result = run('bash', ['scripts/plan-auto-release.sh'], directory)
  const after = run('git', ['status', '--short', '--untracked-files=no'], directory)

  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stdout, /^bump=minor$/m)
  assert.match(result.stdout, /^version=0\.14\.0$/m)
  assert.equal(after.stdout, before.stdout)
})

test('automatic release planning recovers an untagged synced release without another version commit', () => {
  const directory = temporaryDirectory('toon-release-recovery-')
  mkdirSync(join(directory, 'scripts'))
  cpSync(join(root, 'scripts/plan-auto-release.sh'), join(directory, 'scripts/plan-auto-release.sh'))

  for (const args of [
    ['init', '-q'],
    ['config', 'user.name', 'Release Test'],
    ['config', 'user.email', 'release-test@example.invalid'],
  ]) {
    const result = run('git', args, directory)
    assert.equal(result.status, 0, result.stderr)
  }
  writeFileSync(join(directory, 'README.md'), 'baseline\n')
  assert.equal(run('git', ['add', 'README.md', 'scripts/plan-auto-release.sh'], directory).status, 0)
  assert.equal(run('git', ['commit', '-qm', 'chore: release 0.20.0'], directory).status, 0)
  assert.equal(run('git', ['tag', 'v0.20.0'], directory).status, 0)
  writeFileSync(join(directory, 'README.md'), 'baseline\nv4.1\n')
  assert.equal(run('git', ['commit', '-qam', 'feat: implement TOON v4.1'], directory).status, 0)
  writeFileSync(join(directory, 'README.md'), 'baseline\nv4.1\nsynced\n')
  assert.equal(run('git', ['commit', '-qam', 'chore: release 0.21.0'], directory).status, 0)
  const releaseSha = run('git', ['rev-parse', 'HEAD'], directory).stdout.trim()
  writeFileSync(join(directory, 'README.md'), 'baseline\nv4.1\nsynced\nrecovery\n')
  assert.equal(run('git', ['commit', '-qam', 'fix: recover release automation'], directory).status, 0)

  const result = run('bash', ['scripts/plan-auto-release.sh'], directory)

  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stdout, /^bump=none$/m)
  assert.match(result.stdout, /^version=0\.21\.0$/m)
  assert.match(result.stdout, new RegExp(`^release_sha=${releaseSha}$`, 'm'))
  assert.match(result.stdout, /^needs_sync=false$/m)
})

test('CI, automatic release, and manual release use the TypeScript-aware version tools', () => {
  const ci = text(root, '.github/workflows/ci.yml')
  const automatic = text(root, '.github/workflows/auto-release.yml')
  const manual = text(root, '.github/workflows/release.yml')
  const packageManifest = text(root, 'package.json')
  const releaseSurface = `${ci}\n${automatic}\n${manual}\n${packageManifest}`

  assert.match(ci, /run: pnpm check:versions/)
  assert.match(automatic, /bash scripts\/sync-version\.sh "\$VERSION"/)
  assert.match(manual, /run: bash scripts\/check-versions\.sh/)
  assert.match(manual, /run: bash scripts\/sync-version\.sh/)
  assert.match(automatic, /packages\/toon\/src\/version\.ts/)
  assert.match(automatic, /packages\/toon\/dist\/version\.js/)
  assert.match(packageManifest, /packages\/toon\/src\/version\.ts/)
  assert.match(packageManifest, /packages\/toon\/dist\/version\.js/)
  assert.doesNotMatch(releaseSurface, /packages\/toon\/src\/version\.js/)
})

test('stable releases document v4.1 and close only after public clean-room verification', () => {
  const automatic = text(root, '.github/workflows/auto-release.yml')
  const manual = text(root, '.github/workflows/release.yml')

  for (const heading of [
    'TOON v4.1 checkpoint',
    'API cutovers',
    'Extension policy',
    'Migration',
    'Experimental upstream frontiers',
  ]) {
    assert.match(manual, new RegExp(`## ${heading}`))
  }

  assert.match(manual, /name: Capture release-time upstream drift/)
  assert.match(manual, /name: Verify exact-commit CI/)
  assert.match(manual, /name: Verify public artifacts from clean consumers/)
  assert.match(manual, /npm install --ignore-scripts "@reddb-io\/toon@\$\{VERSION\}"/)
  assert.match(manual, /cargo install --version "\$\{VERSION\}" reddb-io-tq/)
  assert.match(manual, /releases\/download\/\$\{TAG\}\/reddb-toon\.vsix/)
  assert.match(manual, /gh issue close "\$CLOSURE_ISSUE"/)
  assert.match(manual, /gh issue close "\$CLOSURE_SPEC"/)
  assert.match(automatic, /closure_issue=247/)
  assert.match(automatic, /closure_spec=203/)
})

test('automatic release dispatch recovers the exact untagged v4.1 release commit', () => {
  const automatic = text(root, '.github/workflows/auto-release.yml')

  assert.match(automatic, /if: steps\.plan\.outputs\.needs_sync == 'true'/)
  assert.match(automatic, /if: steps\.plan\.outputs\.version != ''/)
  assert.match(automatic, /RELEASE_SHA="\$\{\{ steps\.plan\.outputs\.release_sha \}\}"/)
  assert.match(automatic, /-f release_sha="\$RELEASE_SHA"/)
  assert.match(automatic, /if \[\[ "\$VERSION" == "0\.21\.0" \]\]/)
  assert.match(automatic, /closure_issue=247/)
  assert.match(automatic, /closure_spec=203/)
})
