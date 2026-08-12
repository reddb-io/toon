/**
 * Binds the `toon` CLI to the real process. `bin/toon.mjs` imports this module.
 *
 * The exit code is set rather than forced: `process.exit` would discard whatever
 * stdout still has buffered, truncating a piped result partway through.
 */
export {};
