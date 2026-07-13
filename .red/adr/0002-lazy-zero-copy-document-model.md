# Lazy zero-copy document model, no streaming

The engine reads the whole TOON document into memory (read/mmap) and exposes values as zero-copy views over that buffer; tabular-array rows are only decoded when a query touches them. We rejected both full materialization (a jq-style value tree wastes CPU/allocations on everything the query never touches) and true incremental streaming (only pays off for documents larger than RAM — rare for TOON, which is designed to fit LLM contexts — and its benefits vanish anyway on non-local queries like `sort_by`/`group_by`/`reverse` that must buffer the whole input).

## Consequences

- The primary use case — an LLM/agent invoking `tq` one-shot on context-sized files — gets minimal latency and allocation.
- Documents larger than available RAM are out of scope for now; a streaming mode for local-only queries remains possible later without breaking this model.
- Output serialization (including `-o json`) is independent of the input model.
- Zero-copy applies to the read path only: write operations (assignment, `-i` in-place editing — in scope since v1) materialize the touched values and re-serialize the document.
