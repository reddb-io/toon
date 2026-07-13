# reddb-style release pipeline with lockstep crate publishing

The release pipeline is adapted from `reddb`'s `release.yml`: a `plan` job resolving stable (tag `v*.*.*`) vs `next` (push to main → prerelease) channels, a multi-platform build matrix (including fully-static musl assets), a GitHub Release with checksums/attestations, and a `publish-cargo` job. Both workspace crates — `reddb-io-toon` (the parser/serializer/lazy-document-model library) and `reddb-io-tq` (the crate producing the `tq` binary) — publish to crates.io in lockstep from the first `v0.1.0` tag, superseding the earlier intent to hold the library back until its API stabilized: crates.io rejects path-only dependencies, so publishing the `tq` bin crate (required for `cargo install tq`) forces the library to be published too. The `0.x` semver range carries the API-instability warning instead.

The org-prefixed crate names follow the house pattern (`reddb-io-*` crates, short-brand binary — as `reddb-io` ships the `red` binary) and are also forced by the registry: on crates.io, `tq` is a dead squat (game framework stub, 2020) and `toon` is an unrelated active TOON implementation by a third party.

## Consequences

- `cargo install tq` works from the first stable tag.
- The `toon` library's public API is published while still evolving; breaking changes ride `0.x` minor bumps per Cargo semver convention.
- reddb-specific pipeline parts (protoc, Docker images, npm packages, contract-matrix gates) are dropped from the adaptation; the workflow lands together with the Cargo workspace skeleton, since it references files that must exist first.
