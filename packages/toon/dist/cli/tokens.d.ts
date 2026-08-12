/**
 * Token estimates compatible with tokenx 1.3.0, the estimator the pinned
 * upstream TOON CLI uses for `--stats`. Kept in lockstep with the Rust port in
 * `crates/tq/src/cli/token_stats.rs` so both front-ends report the same numbers.
 */
/** Estimates the token count of `text` the way tokenx 1.3.0 does. */
export declare function estimateTokenCount(text: string): number;
/** Formats the `--stats` report body, matching upstream wording and rounding. */
export declare function formatStatistics(json: string, toon: string): {
    estimates: string;
    saved: string;
};
