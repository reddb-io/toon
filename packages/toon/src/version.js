/**
 * The package version, as a source constant.
 *
 * Deliberately not read from package.json at runtime: the package ships as
 * dependency-free ESM that can be bundled or vendored away from its manifest,
 * and a runtime manifest read would need filesystem access the browser build
 * does not have. scripts/sync-version.sh rewrites this line on every release
 * so it stays in lockstep with the crates and the manifest (ADR 0003);
 * test/version.test.mjs is the guard that they never drift apart.
 */
export const VERSION = '0.12.0'
