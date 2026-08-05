# Rebase on TOON spec v4.1 with event-based decoders in both languages

Upstream (toon-format) released spec v4.0/v4.1 while our implementations tracked
v3.3: key folding and path expansion were removed, comment lines, keyed tabular
form (RFC #57) and nested field groups (RFC #46) were added, and the strict-mode
and number-grammar rules were substantially hardened. We decided to adopt v4.1
as the new baseline and rebuild the decoders (Rust and JS) as event-based
streaming decoders targeting the v4.1 rules directly, rather than patching
conformance onto the current tree-walking decoders and rewriting for streaming
afterwards — the double rewrite would be almost pure rework. The JS package is
rewritten in TypeScript in the same pass (types from source, generated `.d.ts`).

Extension policy: where v4.1 absorbed a mechanism our dialect had (nested
tabular headers, keyed collapse territory), the official syntax/semantics win —
the proposals under `docs/proposals/` become design-history documentation only,
with no alias syntax kept alive on decode. Path expansion is removed from the
default decode path, matching its removal from the spec. Extensions the spec
did not absorb (TOONL, cyclic discriminated arrays, truncation detection,
child tables beyond RFC #49) are re-expressed on top of the v4.1 base.

The `vendor/toon` / `vendor/toon-spec` submodule pins are the checkpoint: they
are bumped to the v4.1.1 commits (`a9e6d97` / `62f16b3`) at program start as the
declared target, and conformance is proven by CI against the renormalized v4
fixture corpus, not by the pin itself.
