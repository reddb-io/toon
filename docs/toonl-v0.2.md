# TOONL v0.2

This document defines TOONL v0.2, a line-oriented extension format for appendable
streams of flat records that can be closed into TOON v3.3 documents. It is a
strict superset of [TOONL v0.1](toonl-v0.1.md): every valid v0.1 stream is a
valid v0.2 stream with identical meaning, and this document changes no v0.1
semantics.

v0.2 does not add new record-carrying wire syntax for the common case. Instead it
promotes properties that were already true of v0.1 streams — that a stream can be
suffixed, concatenated, and re-headed without loss — to **first-class,
normatively guaranteed data-model properties**, and it builds four operational
capabilities on top of them: resumable readers (R1), header-preserving trim (R2),
tagged-row multiplexing (R3), and a blessed retry/re-queue pattern (R4).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in RFC 2119.

This document is a **specification of requirements**. It does not itself
mandate that any particular encoder or decoder implement v0.2.

**Implementation status:** the Rust crate (`reddb-io-toon`), the JS package
(`@reddb-io/toon`), and the `tq` CLI implement TOONL v0.2 in full — resumable
readers (R1), `tq trim --keep-last N` (R2), tagged-row multiplexing with both
close-transform variants (R3), and the side-journal pattern (R4) — since
registry version **0.2.6**. The shared executable corpus under
`tests/toonl/fixtures/` pins both implementations to identical behavior. See
[Versioning And Compatibility](#versioning-and-compatibility).

## Terminology

The following terms are used throughout and refine the v0.1 vocabulary.

- **Row**: a single non-blank, non-header, non-trailer line that carries one
  object under the active header. Blank lines, header lines, and trailer lines are
  **not rows**.
- **Row boundary**: a byte offset that sits immediately after an LF terminating a
  complete line and immediately before the first byte of the next line (or EOF).
  Every complete line ends on a row boundary. A byte offset in the middle of a
  line is not a row boundary.
- **Active header line**: the most recent header line that is in effect at a given
  point in the stream — the header whose schema binds the rows that follow it. The
  *verbatim* active header line is that line's exact bytes, including its leading
  `[`, its delimiter symbol if any, its field list, its trailing `:`, and its
  terminating LF.
- **Segment**: as in v0.1 — a header, zero or more rows, and an optional trailer.
- **Stream**: a sequence of segments and blank lines, as in v0.1.
- **Writer on open**: a process that appends rows to a stream it has just opened
  (e.g. a log producer that reopens a file for append).

## Identity

TOONL v0.2 files SHOULD use the `.toonl` extension. Tools that need a media hint
SHOULD use `application/toonl`. There is no separate media type or extension for
v0.2; the version is a property of the content, signaled as described in
[Versioning And Compatibility](#versioning-and-compatibility).

As in v0.1, TOONL is not open-phase TOON v3.3. A TOONL decoder MUST parse TOONL
with the grammar in this document, and a TOON decoder is expected to reject TOONL
open-phase syntax. Compatibility is provided by the close-transform, not by making
every open stream a valid TOON document.

## Closure Properties

v0.2 elevates three closure properties to first-class data-model guarantees. Every
capability in this document rests on them, and a conforming v0.2 implementation
MUST preserve them.

### Suffix-closure

Let `S` be a valid TOONL stream, let `b` be any row boundary in `S`, and let `H`
be the verbatim active header line in effect at `b`. Then the stream formed by
concatenating `H` with the byte suffix of `S` starting at `b` is a valid TOONL
stream, and it decodes to exactly the sequence of rows that `S` produces from `b`
onward, under the schema `H` declares.

Informally: **any row-boundary suffix, re-prefixed with the header that was active
there, is a valid stream equivalent to the tail it was cut from.** This is what
makes trimming (R2) and resuming (R1) safe.

Two constraints make suffix-closure well-defined:

1. The cut MUST be at a row boundary. A suffix that begins in the middle of a line
   is not covered by this guarantee.
2. The prefixed header MUST be the *verbatim* active header line — the exact bytes,
   including the active delimiter symbol. Re-deriving or re-serializing the header
   is not permitted, because a re-serialization could change the delimiter or field
   quoting and thereby change how the suffix rows parse.

A trailer (`[=N]`) that appeared before the cut is not carried into the suffix; the
suffix is an open segment until it is closed by a new header, a trailer, or EOF.
See [Trailers Under Trimming](#trailers-under-trimming) for how synthesized
suffixes handle trailer counts.

### Concatenation closure

Let `A` and `B` be valid TOONL streams. Then the byte concatenation `A || B` is a
valid TOONL stream.

The concatenation decodes to the rows of `A` followed by the rows of `B`. If `B`
begins with a header line (as a self-contained stream normally does), that header
opens a fresh segment — this is an ordinary schema rotation at the seam, identical
to any mid-stream header. If `A` did not end on a row boundary, the concatenation
is still byte-valid only when `A` is itself a valid stream (which requires `A` to
end on a row boundary); this is why every valid stream ends on a row boundary.

Concatenation closure is what makes `>>` append safe, makes multi-file
concatenation (`cat a.toonl b.toonl`) a valid stream, and underpins the
side-journal pattern (R4).

### Header-on-open discipline

To make suffix-closure and concatenation closure hold for **appending writers**,
v0.2 imposes a discipline on writers:

> A writer that opens a stream for append MUST emit its schema header before its
> first row on that open.

This is the **header-on-open** rule. Its consequence is that a stream never
contains a run of rows that is not immediately preceded, within the same open, by
the header that governs them. A reader that starts at any point produced by a fresh
open therefore always finds a header before the rows.

Header-on-open is an **idempotent rotation**: emitting the active header again when
it is byte-identical to the one already in effect is a rotation that opens a new
segment with the same schema, and it is *semantically transparent* — the rows that
follow decode identically whether or not the redundant header was emitted. Because
it is idempotent, a writer MAY emit the header on every open without tracking
whether the previous open left the same schema active.

The close-transform and `tq` MAY **coalesce adjacent identical headers**: when a
header line is byte-identical to the header of the immediately preceding segment
and no rows intervene, the transform MAY drop the redundant header and merge the
two segments into one. Coalescing is OPTIONAL and MUST NOT change the row sequence
or the per-segment row counts of the closed output beyond removing an empty
segment. A trailer between two identical headers with no rows counts as an empty
segment (`[=0]`) and MAY likewise be coalesced away.

#### Worked example — header-on-open

A producer writes two rows, is restarted, reopens the file for append, and writes
one more row. Under header-on-open it re-emits the header on the second open:

```toonl
[]{ts,level,msg}:
2026-07-14T03:00:00Z,info,boot
2026-07-14T03:00:02Z,error,"disk full"
[]{ts,level,msg}:
2026-07-14T03:05:00Z,info,resume
```

The second `[]{ts,level,msg}:` is an idempotent rotation. A reader starting at any
row boundary — including the one just before the second header — always has a
header in front of it. The close-transform MAY coalesce the two identical headers
into a single three-row segment, or MAY keep them as a two-row and a one-row
segment; both are conforming and carry the same rows.

## R1 — Resumable Readers

R1 lets a reader stop at a row boundary, remember where it was, and later resume
without rescanning the whole stream, while remaining safe against truncation and
rewrite.

### Cursor convention

A **resume cursor** is the triple:

```
{ byteOffset, activeHeaderLine, rowsSinceHeader }
```

- `byteOffset` — a **row boundary** in the stream: the byte offset at which the
  next unread row begins. MUST be a row boundary.
- `activeHeaderLine` — the **verbatim** active header line in effect at
  `byteOffset` (exact bytes, including the terminating LF), as defined in
  [Terminology](#terminology). Stored verbatim precisely so that a resume can
  reconstruct a suffix-closed stream without re-serializing the header.
- `rowsSinceHeader` — the number of rows that have been consumed since
  `activeHeaderLine` became active, up to `byteOffset`. Used for diagnostics and to
  re-derive a trailer count if the reader also needs to close the resumed suffix.

A reader MAY persist this cursor by any means (a sidecar file, a database column, a
message-queue offset). The convention is the *shape* of the cursor and its
guarantee, not a storage format.

### Resume guarantee

> Decoding the stream starting at `byteOffset`, treating `activeHeaderLine` as the
> active header, yields exactly the row sequence that a sequential scan from the
> beginning of the stream would produce starting at that row boundary.

This is a direct corollary of [suffix-closure](#suffix-closure): the cursor names
a row boundary and the verbatim active header, which is precisely what
suffix-closure requires. A resuming reader constructs the logical stream
`activeHeaderLine || suffix(byteOffset)` and decodes it.

### Invalidation conditions

A cursor MUST be treated as **invalid** — and the reader MUST fall back to a full
rescan (or an error, per the reader's policy) — if any of the following hold when
the reader attempts to resume:

1. **Truncation**: the current file size is less than `byteOffset`. The bytes the
   cursor pointed past no longer exist, so the stream has been truncated or
   replaced.
2. **Anchor mismatch**: the bytes at a remembered anchor no longer match. A reader
   that remembered anchor bytes (for example, the `activeHeaderLine` at its recorded
   offset, or a byte fingerprint immediately before `byteOffset`) MUST re-read those
   bytes and compare; if they differ, the underlying stream was rewritten rather than
   only appended, and the cursor is invalid.

A reader SHOULD store at least one anchor sufficient to detect rewrite-in-place
(the two conditions above catch shrink and mutate respectively). A cursor is valid
only for a stream that has been **append-only** since the cursor was taken; any
in-place rewrite invalidates it. This is consistent with R4 declaring in-place
splice a non-goal.

### OPTIONAL continuation header

For long-lived, single-segment streams — a `tail -f`-style log that may run for
days under one schema — a reader that arrives late has to scan backward an
unbounded distance to find the one header at the top. v0.2 defines an OPTIONAL
**continuation header** to bound that scan.

Syntax:

```
[~]{field1,field2}:
```

A continuation header uses the `~` sentinel in the bracket where an anonymous
header has nothing and a tagged declaration (R3) has a tag. Its rules:

- A continuation header MUST be byte-equal to the currently active header in every
  respect except the `~` sentinel — same delimiter symbol, same field list, same
  quoting. A decoder MUST reject a continuation header whose fields or delimiter
  differ from the active header.
- A continuation header **is not a rotation**. It does not open a new segment, does
  not reset `rowsSinceHeader` for trailer purposes, and does not change the active
  schema. It is a re-assertion of the active header for the benefit of late readers.
- The close-transform MUST **discard** continuation headers entirely; they never
  appear in closed TOON output and never affect row counts.
- Encoders MAY emit a continuation header periodically — every K rows or every K
  bytes — to bound how far back a late reader scans. Encoders that do not need
  bounded late-join MUST NOT be required to emit them.
- A resuming reader that lands just after a continuation header MAY use it as the
  `activeHeaderLine` for its cursor, because it is byte-equal (modulo `~`) to the
  active header; when doing so the reader MUST normalize the `~` sentinel back to
  the active header's sentinel (anonymous `[]` or the tag) before applying
  suffix-closure, since the reconstructed suffix must be prefixed with a real
  header, not a continuation marker.

#### Worked example — resume with continuation header

A long-running producer emits a continuation header every three rows:

```toonl
[]{ts,seq}:
2026-07-14T03:00:00Z,1
2026-07-14T03:00:01Z,2
2026-07-14T03:00:02Z,3
[~]{ts,seq}:
2026-07-14T03:00:03Z,4
2026-07-14T03:00:04Z,5
```

A reader that opens the file and seeks near the end finds `[~]{ts,seq}:` within a
bounded window, learns the active schema without scanning to byte 0, and takes a
cursor `{ byteOffset = <offset of the `2026-07-14T03:00:03Z,4` line>,
activeHeaderLine = "[]{ts,seq}:\n", rowsSinceHeader = 0 }`. Closing the resumed
suffix drops the `[~]` line and yields:

```toon
[2]{ts,seq}:
  2026-07-14T03:00:03Z,4
  2026-07-14T03:00:04Z,5
```

## R2 — Header-Preserving Trim

R2 defines **keep-last-N** on top of suffix-closure: bound a stream to its most
recent N rows while keeping it a valid, self-describing stream.

### keep-last-N algorithm

Given a valid TOONL stream `S` and a cap `N` (rows), produce a trimmed stream `S'`:

1. **Count in rows.** Scan `S` and identify row boundaries. The cap `N` counts
   **rows only**; headers, trailers, and blank lines are not counted.
2. **Choose the cut.** If `S` has `M` rows and `M <= N`, `S' = S` (no trim needed).
   Otherwise let the cut be the row boundary immediately before the `(M − N + 1)`-th
   row — i.e. the boundary such that exactly `N` rows follow it.
3. **Cut at a row boundary.** The cut MUST be a row boundary (never mid-line), so
   suffix-closure applies.
4. **Determine the active header at the cut.** Find the verbatim active header line
   in effect at the cut boundary.
5. **Emit the synthesized stream.** Write the verbatim active header line, then the
   retained byte suffix from the cut boundary to EOF. Per suffix-closure this is a
   valid stream carrying exactly the last `N` rows.
6. **Apply the trailer rule** (below) to the retained suffix.

The synthesized header is a **verbatim** copy of the active header line — the same
bytes, same delimiter symbol — not a re-serialization. This preserves the exact
delimiter and quoting the retained rows were written against.

If the retained suffix crosses one or more schema rotations (the last `N` rows span
more than one segment), the retained bytes already contain those interior headers;
step 5 only prepends the header active at the cut. The result is a multi-segment
stream whose first segment is headed by the synthesized header and whose later
segments are the rotations that were already present.

### Trailers under trimming

A trailer states the row count of *its* segment. After a trim, the count of the
first retained segment usually changes, so a stale trailer would be wrong. The rule
is **drop-or-recount**:

- If the **original segment that the cut falls within had no trailer**, the
  synthesized first segment MUST NOT gain one. The trim only re-heads a suffix; it
  does not invent verification the producer never provided.
- If the **original segment that the cut falls within had a trailer** (i.e. the
  producer closed that segment with `[=N_orig]`), then the trailer for the
  synthesized first segment MUST be **recounted** to the number of retained rows in
  that first segment, or dropped. An implementation MUST NOT copy the original
  trailer's count unchanged onto the trimmed segment.

Interior segments fully contained in the retained suffix keep their original
trailers unchanged, because their row counts did not change.

### Atomic write

A trim that replaces a file in place MUST be **atomic**: write the trimmed stream
to a temporary file in the same directory, then `rename` it over the original. A
reader observing the file MUST see either the pre-trim stream or the post-trim
stream, never a partial write. (A reader holding a cursor into the pre-trim stream
will observe its [invalidation conditions](#invalidation-conditions) after the
rename — file size shrinks and/or the anchor bytes move — and fall back to rescan,
which is correct.)

### `tq trim --keep-last N` verb contract

v0.2 specifies the contract of a `tq trim` verb; implementation is a follow-up
Spec, not this document.

```
tq trim --keep-last N [--in-place] [FILE]
```

- `--keep-last N` — REQUIRED. `N` is a non-negative integer row cap. `N = 0`
  produces a stream with the active header and zero rows (a valid empty segment).
- Input is `FILE` (a `.toonl` stream) or stdin.
- Default output is stdout: the trimmed stream is written to stdout and the input
  is not modified.
- `--in-place` — OPTIONAL. Rewrites `FILE` atomically (tmp+rename in the same
  directory, per [Atomic write](#atomic-write)). MUST error if no `FILE` is given.
- The verb MUST count rows per the [keep-last-N algorithm](#keep-last-n-algorithm),
  cut at a row boundary, emit the verbatim active header, and apply the
  drop-or-recount trailer rule.
- If the input has `M <= N` rows the verb MUST output the input unchanged
  (byte-for-byte when `--in-place` is not combined with a normalizing option).
- Exit status is `0` on success and non-zero on a malformed input stream.

#### Worked example — trim

Given `app.toonl`:

```toonl
[]{ts,level,msg}:
2026-07-14T03:00:00Z,info,boot
2026-07-14T03:00:01Z,info,ready
2026-07-14T03:00:02Z,error,"disk full"
2026-07-14T03:00:03Z,info,recovered
[=4]
```

`tq trim --keep-last 2 app.toonl` emits (the original segment had a trailer, so the
first retained segment's trailer is recounted to `2`):

```toonl
[]{ts,level,msg}:
2026-07-14T03:00:02Z,error,"disk full"
2026-07-14T03:00:03Z,info,recovered
[=2]
```

If the original had **no** `[=4]` trailer, the output would be the same two lines
under the header with **no** trailer.

## R3 — Tagged-Row Multiplexing

R3 lets a single stream carry more than one record shape interleaved, without
paying any cost when the stream has a single shape.

### Named schema declarations and tagged rows

A **named schema declaration** binds a short tag to a schema:

```
[]<tag>{field1,field2}:
```

The `<tag>` sits in the bracket, between `[]` and `{...}`. A **tagged row**
references a declared tag:

```
<tag>:cell1,cell2
```

A tagged row's cells bind positionally to the fields of the schema declared for
`<tag>`, exactly as untagged rows bind to the anonymous schema.

Grammar for the tag token:

```abnf
tag        = 1*( ALPHA / DIGIT / "_" / "-" ) ; MUST NOT contain the active delimiter, ":", "{", "}", "[", "]", or "~"
tag-decl   = "[]" "<" tag ">" "{" field *( delim field ) "}" ":" LF
tagged-row = tag ":" cell *( delim cell ) LF
```

A tag MUST match the same character class in a declaration and in the rows that
reference it. A `<tag>:` prefix on a row is distinguished from an anonymous row by
the presence of a bare tag token followed by `:` before the first cell; because
anonymous v0.1 rows are TOON cells and a leading `tag:` would be TOON-quoted if it
were data, an unquoted `tag:` prefix is unambiguous. A decoder MUST reject a tagged
row whose tag has not been declared by an in-scope `[]<tag>{...}:` declaration.

### Bounded live-schema table

A decoder maintains a **live-schema table** mapping in-scope tags to their schemas,
plus the single anonymous schema. The table is **bounded**: an implementation MUST
support at least **8** simultaneously-live tagged lanes, and MUST reject a stream
that declares a 9th distinct live tag beyond the supported bound rather than
growing without limit. The anonymous schema does not count against the tagged-lane
bound.

The bound is a robustness limit (a multiplexed stream with hundreds of live lanes
is almost certainly a producer bug or an attack), not a target; producers SHOULD
keep the number of live lanes small.

### Redefinition is the rotation

Re-declaring an already-live tag with a new `[]<tag>{...}:` **rotates that lane's
schema**: rows tagged with that tag after the redeclaration bind to the new schema.
This is the tagged analogue of anonymous schema rotation. Redefinition does not
free a lane; it replaces the schema in the existing lane slot, so it does not push
the table over the bound.

### Untagged rows keep v0.1 semantics

An **untagged row** binds to the sole **anonymous** schema — the schema most
recently declared by an anonymous `[]{...}:` header — exactly as in v0.1. A stream
that never uses a tagged declaration or a tagged row is a v0.1 stream and behaves
identically. **Single-shape streams pay nothing**: no tag byte, no lane, no table
growth.

The anonymous schema and tagged lanes coexist: a stream MAY interleave untagged
rows (bound to the anonymous schema) with tagged rows (bound to their lanes).

### Canonical per-shape field order

To keep multiplexed output deterministic and diffable, **encoders MUST emit a
canonical field order per shape.** For each distinct shape an encoder produces, the
field order in the declaration MUST be either **sorted** (e.g. lexicographic) or
**first-seen** (the order fields first appeared for that shape), chosen once and
applied consistently for that shape across the stream. An encoder MUST NOT reorder
a shape's fields arbitrarily between declarations of the same shape. Decoders do
not require canonical order to parse (fields bind positionally to whatever the
declaration lists), but the requirement makes encoder output stable.

### Close-transform for tagged streams

The close-transform for a multiplexed stream has two defined forms.

**Per-lane form (default).** Produce **one TOON document per lane**, in the order
each lane's schema was first declared in the stream. The anonymous schema, if any
rows bound to it, is one such lane. Within a lane, rows appear in stream order. Each
lane closes exactly as a v0.1 segment closes: header rewritten to `[count]{...}:`
with the materialized count, rows indented two spaces. A lane that rotated schema
mid-stream closes to multiple TOON documents for that lane, one per schema epoch,
in stream order — identical to how anonymous rotation closes in v0.1.

**Interleave-preserving form (variant).** Produce a single sequence that preserves
the original interleaving of rows across lanes. This form is for consumers that
need to reconstruct the exact temporal order of a multiplexed stream. It is defined
as: for each maximal run of consecutive rows that share a lane, emit one TOON
document for that run (header from the run's schema, count = run length, rows
indented). Runs appear in stream order. A tool MUST document which form it emits;
the per-lane form is the default when unspecified.

#### Worked example — tagged multiplexing

A stream multiplexes request logs and metric samples:

```toonl
[]<req>{method,path,status}:
[]<metric>{name,value}:
req:GET,/health,200
metric:cpu,0.42
req:POST,/login,401
metric:cpu,0.55
```

**Per-lane close** (two documents, `req` declared first, then `metric`):

```toon
[2]{method,path,status}:
  GET,/health,200
  POST,/login,401
```

```toon
[2]{name,value}:
  cpu,0.42
  cpu,0.55
```

**Interleave-preserving close** (four single-row documents in stream order):

```toon
[1]{method,path,status}:
  GET,/health,200
```

```toon
[1]{name,value}:
  cpu,0.42
```

```toon
[1]{method,path,status}:
  POST,/login,401
```

```toon
[1]{name,value}:
  cpu,0.55
```

#### Worked example — untagged compatibility

Untagged rows bind to the anonymous schema and may interleave with tagged rows:

```toonl
[]{event}:
[]<audit>{actor,action}:
started
audit:alice,login
finished
```

`started` and `finished` bind to the anonymous `[]{event}:` schema; `audit:...`
binds to the `audit` lane. Per-lane close yields a `[2]{event}:` document
(`started`, `finished`) and a `[1]{actor,action}:` document (`alice,login`).

### Interaction with the reserved `- ` prefix

The v0.1 reservation stands: lines beginning with `- ` remain **reserved for a
future nested-frame syntax**. A v0.2 decoder MUST reject any non-blank line whose
first two bytes are `- `. Tags MUST NOT begin with `- `, and the tag character
class above excludes the space that would make a `- ` prefix, so tagged rows never
collide with the reservation.

## R4 — In-Place Splice Non-Goal And The Side-Journal Pattern

### Splice non-goal

**In-place row splice — mutating, replacing, or deleting an individual row within a
stream's existing bytes — is an explicit non-goal of TOONL v0.2 and is NOT
specified.**

Rationale:

- In-place mutation breaks [suffix-closure](#suffix-closure) and the R1 resume
  guarantee: a cursor's validity rests on the stream being append-only since the
  cursor was taken, and every anchor/truncation check assumes rewrite invalidates.
  Making splice a first-class operation would force every reader to defend against
  arbitrary interior mutation, which defeats cheap resumption.
- It breaks the self-checking property: trailers count rows in a segment; splicing a
  row either invalidates the trailer or forces a rewrite of the whole downstream,
  which is no longer a splice.
- Append-only streams are the whole point of the format (TOONL is to TOON what JSONL
  is to JSON). Interior edit is the property JSONL-style logs deliberately give up in
  exchange for cheap append, tail, and concatenation.

Consumers that need to "change" a row MUST express it as a new appended row (a
correction record) plus application-level last-writer-wins semantics, or MUST
rebuild the stream from a source of truth — not by editing bytes in place.

### The side-journal pattern (blessed retry / re-queue)

The blessed pattern for retry and re-queue is a **side journal**: a separate file
that carries the retries, drained ahead of the main stream.

- Retries are written to a separate `.retry` stream (e.g. `app.toonl.retry`), **with
  its own header**, using the same header-on-open discipline as any writer. The
  `.retry` file is a self-contained, valid TOONL stream in its own right.
- A consumer **drains the `.retry` stream ahead of the main stream**: it processes
  the side journal first, then the main stream, so retried work is re-attempted
  before new work.
- Logically, processing is `retry-journal || main-stream` — which is a valid stream
  by [concatenation closure](#concatenation-closure). The consumer never needs to
  splice a retried row back into the main stream's bytes; concatenation gives the
  combined view for free.
- Because each `.retry` writer follows [header-on-open](#header-on-open-discipline),
  the side journal is safe to append to from a reopened producer, safe to
  concatenate, and safe to trim (R2) and resume (R1) exactly like a main stream.

This pattern rests entirely on **concatenation closure + header-on-open**: no new
wire syntax is required, and no interior mutation ever occurs.

#### Worked example — side journal

Main stream `jobs.toonl`:

```toonl
[]{id,payload}:
1,alpha
2,beta
```

A consumer fails job `2` and re-queues it into `jobs.toonl.retry`, which is its own
valid stream with its own header:

```toonl
[]{id,payload}:
2,beta
```

The consumer's logical input is `jobs.toonl.retry || jobs.toonl`, draining the
retry first:

```toon
[3]{id,payload}:
  2,beta
  1,alpha
  2,beta
```

(De-duplication of the re-tried `2` is an application concern — last-writer-wins or
an idempotency key — not a stream-format concern.)

## Versioning And Compatibility

TOONL v0.2 is a strict superset of v0.1. The compatibility contract is:

1. **v0.2 readers MUST accept v0.1 streams unchanged.** A stream that uses only
   v0.1 constructs decodes identically under a v0.2 reader; v0.2 changes no v0.1
   semantics.
2. **v0.1 decoders MUST cleanly reject v0.2-only constructs.** The v0.2-only
   constructs are the continuation header `[~]{...}:`, named schema declarations
   `[]<tag>{...}:`, and tagged rows `<tag>:...`. A v0.1 decoder MUST reject each of
   these rather than silently mis-parsing them:
   - `[~]{...}:` — the `~` is not a valid v0.1 delimiter symbol (v0.1 allows only an
     empty symbol, `|`, or HTAB inside `[]`), so a v0.1 header parser rejects it.
   - `[]<tag>{...}:` — the `<tag>` between `]` and `{` is not part of the v0.1 header
     grammar, so a v0.1 header parser rejects it.
   - `<tag>:...` — a v0.1 decoder parses this as an anonymous row; the leading
     `tag:` is TOON cell data. Because a v0.1 reader has no lane for it and the
     arity will not match the anonymous schema in the general case, it is rejected
     as a malformed row. Producers MUST NOT rely on a v0.1 reader interpreting a
     tagged row as anything meaningful; v0.2-only streams are for v0.2 readers.

### How a stream signals v0.2

A TOONL stream signals that it requires a v0.2 reader **structurally, by using a
v0.2-only construct** — a continuation header, a named schema declaration, or a
tagged row. There is no version banner line and none is required: a stream that
uses no v0.2-only construct is, by definition, a v0.1 stream and is readable by
v0.1 and v0.2 readers alike.

Producers that want an explicit, human-visible signal MAY document the v0.2
requirement out of band (file naming convention, catalog metadata, or the media
type parameter `application/toonl; version=0.2`). Such out-of-band signaling is
OPTIONAL and MUST NOT be required for correctness; the structural rule above is
authoritative. A decoder MUST NOT depend on an out-of-band signal to decide whether
to accept v0.2 constructs — it accepts them iff it is a v0.2 decoder.

## Grammar Additions

The following ABNF extends the v0.1 grammar. It is normative except where it
delegates cell parsing to TOON v3.3. Constructs not shown here are unchanged from
v0.1.

```abnf
stream        = *( segment / decl / cont-header / tagged-row / blank-line )

; v0.1 anonymous header and rows are unchanged.
header        = "[" [ delim-sym ] "]" "{" field *( delim field ) "}" ":" LF

; v0.2 continuation header (R1) — MUST match the active header modulo the "~".
cont-header   = "[~]" "{" field *( delim field ) "}" ":" LF

; v0.2 named schema declaration (R3).
decl          = "[]" "<" tag ">" "{" field *( delim field ) "}" ":" LF

; v0.2 tagged row (R3).
tagged-row    = tag ":" cell *( delim cell ) LF

tag           = 1*( ALPHA / DIGIT / "_" / "-" )
                ; MUST NOT contain the active delimiter, ":", "{", "}", "[", "]", or "~"

delim-sym     = "|" / HTAB   ; unchanged from v0.1; note "~" is NOT a delim-sym
delim         = "," / "|" / HTAB
field         = 1*( %x21-7E ) ; interpreted with TOON key quoting rules
cell          = <TOON primitive token using the active delimiter>
```

Note that `delim-sym` deliberately does **not** include `~`; the `[~]` continuation
header is a distinct production, and the exclusion is what makes a v0.1 decoder
reject `[~]{...}:`.

## Traceability

This section maps the red-skills requirements R1–R4
([reddb-io/red-skills#1770](https://github.com/reddb-io/red-skills/issues/1770))
to the sections that formally close them.

| Requirement | Summary | Section(s) |
| --- | --- | --- |
| Closure — suffix | Suffix + verbatim active header is a valid equivalent stream | [Suffix-closure](#suffix-closure) |
| Closure — concatenation | Concatenation of two valid streams is a valid stream | [Concatenation closure](#concatenation-closure) |
| Closure — header-on-open | Writer emits header before first row on open; idempotent rotation; coalescing | [Header-on-open discipline](#header-on-open-discipline) |
| **R1** | Resumable readers: cursor convention, resume guarantee, invalidation, optional `[~]` continuation header | [R1 — Resumable Readers](#r1--resumable-readers) |
| **R2** | Header-preserving trim: keep-last-N on suffix-closure, trailer drop-or-recount, atomic write, `tq trim --keep-last N` contract | [R2 — Header-Preserving Trim](#r2--header-preserving-trim) |
| **R3** | Tagged-row multiplexing: named declarations, tagged rows, bounded 8-lane table, redefinition rotation, untagged v0.1 compatibility, canonical field order, close-transform (per-lane + interleave-preserving) | [R3 — Tagged-Row Multiplexing](#r3--tagged-row-multiplexing) |
| **R4** | In-place splice non-goal + side-journal retry/re-queue pattern | [R4 — In-Place Splice Non-Goal And The Side-Journal Pattern](#r4--in-place-splice-non-goal-and-the-side-journal-pattern) |
| Versioning | v0.1↔v0.2 accept/reject rules and version signaling | [Versioning And Compatibility](#versioning-and-compatibility) |

## Relationship To v0.1 And To TOON v3.3

- This document does not change any v0.1 semantics. Where v0.2 is silent, v0.1
  governs.
- The v0.1 reserved `- ` line prefix stays reserved for a future nested-frame
  syntax under v0.2.
- v0.2 does not change the TOON v3.3 document specification. The close-transform
  continues to map closed TOONL segments to valid TOON v3.3 documents.
- v0.2 is a specification of requirements, implemented in full by the Rust
  crate, the JS package, and `tq` since registry version 0.2.6. Nothing in this
  document required an existing v0.1 conformance fixture to change; v0.2
  behavior is pinned by its own fixture corpus alongside the v0.1 one.
