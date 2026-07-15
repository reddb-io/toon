# TOONL — Token-Oriented Object Notation, Lines

**TL;DR:** TOONL is a line-oriented streaming format for appending flat records to logs, one per line, with a header that defines the schema. Segments can be closed deterministically into TOON v3.3 documents, and v0.2 adds resumable readers, header-preserving trimming, multi-schema multiplexing, and append-safe patterns. TOONL is to TOON what JSONL is to JSON.

## Table of Contents

- [Acknowledgment](#acknowledgment)
- [Introduction](#introduction)
- [Identity](#identity)
- [Terminology](#terminology)
- [Data Model](#data-model)
- [Encoding](#encoding)
- [Grammar](#grammar)
- [Headers And Rotation](#headers-and-rotation)
- [Trailers](#trailers)
- [Close-Transform](#close-transform)
- [Closure Properties (v0.2)](#closure-properties-v02)
  - [Suffix-closure](#suffix-closure)
  - [Concatenation closure](#concatenation-closure)
  - [Header-on-open discipline](#header-on-open-discipline)
- [R1 — Resumable Readers (v0.2)](#r1--resumable-readers-v02)
  - [Cursor convention](#cursor-convention)
  - [Resume guarantee](#resume-guarantee)
  - [Invalidation conditions](#invalidation-conditions)
  - [OPTIONAL continuation header](#optional-continuation-header)
- [R2 — Header-Preserving Trim (v0.2)](#r2--header-preserving-trim-v02)
  - [keep-last-N algorithm](#keep-last-n-algorithm)
  - [Trailers under trimming](#trailers-under-trimming)
  - [Atomic write](#atomic-write)
  - [`tq trim --keep-last N` verb contract](#tq-trim---keep-last-n-verb-contract)
- [R3 — Tagged-Row Multiplexing (v0.2)](#r3--tagged-row-multiplexing-v02)
  - [Named schema declarations and tagged rows](#named-schema-declarations-and-tagged-rows)
  - [Bounded live-schema table](#bounded-live-schema-table)
  - [Redefinition is the rotation](#redefinition-is-the-rotation)
  - [Untagged rows keep v0.1 semantics](#untagged-rows-keep-v01-semantics)
  - [Canonical per-shape field order](#canonical-per-shape-field-order)
  - [Close-transform for tagged streams](#close-transform-for-tagged-streams)
  - [Interaction with the reserved `- ` prefix](#interaction-with-the-reserved----prefix)
- [R4 — In-Place Splice Non-Goal And The Side-Journal Pattern (v0.2)](#r4--in-place-splice-non-goal-and-the-side-journal-pattern-v02)
  - [Splice non-goal](#splice-non-goal)
  - [The side-journal pattern (blessed retry / re-queue)](#the-side-journal-pattern-blessed-retry--re-queue)
- [Versioning And Compatibility](#versioning-and-compatibility)
  - [Structural version signaling](#structural-version-signaling)
- [Relationship To TOON v3.3](#relationship-to-toon-v33)
- [Conformance](#conformance)

## Acknowledgment

TOONL stands on the shoulders of **TOON**, the Token-Oriented Object Notation
created and stewarded by the [toon-format](https://github.com/toon-format/spec)
team and its author, Johann Schopplich. TOON gave the world a deterministic,
minimally-quoted, token-efficient encoding of the JSON data model; TOONL is our
grateful, optimistic extension of that idea into *streams* — appendable logs of
flat records that close back into valid TOON v3.3 documents. Everything TOONL
does rests on TOON's design, and we thank the toon-format team for setting a
standard clean enough to build a streaming layer on top of. TOON v3.3 is
released under the MIT License; TOONL is a reddb-io extension with its own
versioning and does not modify the TOON v3.3 specification.

## Introduction

This document is the single normative specification of **TOONL**, a
line-oriented extension format for appendable streams of flat records that can
be closed into TOON v3.3 documents. **TOONL is to TOON what JSONL is to JSON**:
one record per line, header once, append forever — a log you can `>>` into and
`tail -f` out of.

This document unifies the previously separate **v0.1** and **v0.2** drafts into
one normative text. v0.2 is a *strict superset* of v0.1: every valid v0.1 stream
is a valid v0.2 stream with identical meaning, and v0.2 changes no v0.1
semantics. Because the superset relationship is strict, a single document
suffices. Throughout, features introduced by v0.2 are marked **(v0.2)**; a
stream that uses no v0.2-only construct is, by definition, a v0.1 stream and is
readable by v0.1 and v0.2 readers alike (see [Versioning And
Compatibility](#versioning-and-compatibility)).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in RFC 2119.

This document is a **specification of requirements**. It does not itself mandate
that any particular encoder or decoder implement every capability. Where an
implementation status is relevant, it is stated inline.

**Implementation status.** The Rust crate (`reddb-io-toon`), the JS package
(`@reddb-io/toon`), and the `tq` CLI implement TOONL in full — the v0.1 base
plus the four v0.2 capabilities: resumable readers (R1), header-preserving trim
(R2, `tq trim --keep-last N`), tagged-row multiplexing with both close-transform
variants (R3), and the side-journal retry pattern (R4) — since registry version
**0.2.6**. The shared executable corpus under `tests/toonl/fixtures/` pins both
implementations to identical behavior.

## Identity

TOONL files SHOULD use the `.toonl` extension. Tools that need a media hint
SHOULD use `application/toonl`. There is no separate media type or extension per
version; the version is a property of the content, signaled as described in
[Versioning And Compatibility](#versioning-and-compatibility).

TOONL is **not** open-phase TOON v3.3. A TOONL decoder MUST parse TOONL with the
grammar in this document, and a TOON decoder is expected to reject TOONL
open-phase syntax. Compatibility is provided by the [close-transform](#close-transform),
not by making every open stream a valid TOON document.

## Terminology

The following terms are used throughout.

- **Row**: a single non-blank, non-header, non-trailer line that carries one
  object under the active header. Blank lines, header lines, and trailer lines
  are **not rows**.
- **Segment**: a header, zero or more rows, and an optional trailer.
- **Stream**: a sequence of segments and blank lines.
- **Open vs closed phase**: a segment is **open** while it may still receive
  rows — it has a header and, so far, no trailer. It is **closed** when a trailer
  `[=N]`, a new header, or EOF ends it. A whole stream is *closed* when its final
  segment is closed with a verifiable trailer; it is *open* when it may still be
  appended to. Continuous writing (`>>`, `tail -f`) operates on open streams;
  the close-transform operates on closed segments.
- **Row boundary** (v0.2): a byte offset that sits immediately after an LF
  terminating a complete line and immediately before the first byte of the next
  line (or EOF). Every complete line ends on a row boundary; a byte offset in the
  middle of a line is not a row boundary.
- **Active header line**: the most recent header line in effect at a given point
  — the header whose schema binds the rows that follow it. The **verbatim**
  active header line is that line's exact bytes, including its leading `[`, its
  delimiter symbol if any, its field list, its trailing `:`, and its terminating
  LF.
- **Writer on open** (v0.2): a process that appends rows to a stream it has just
  opened (e.g. a log producer that reopens a file for append).

## Data Model

A TOONL stream is a sequence of segments. Each segment has a header, zero or more
rows, and an optional trailer.

Each row in a segment represents one object. The segment header defines the
object fields. Row cells are positional: cell 1 maps to field 1, cell 2 maps to
field 2, and so on. A row MUST contain exactly one cell per field.

**Concrete example:**

```toonl
[]{id,name,age}:
1,alice,30
2,bob,null
3,"charlie smith",25
```

Here the header declares three fields: `id`, `name`, `age`. Each row has exactly 3 cells:
- Row 1: `id=1`, `name=alice`, `age=30`
- Row 2: `id=2`, `name=bob`, `age=null` (explicit `null`)
- Row 3: `id=3`, `name="charlie smith"` (quoted field), `age=25`

Null is explicit. Encoders MUST write `null` for a null field value. Encoders
MUST NOT omit a cell to imply null.

**Invalid example (MUST be rejected):**

```toonl
[]{id,name,age}:
1,alice
3,"charlie smith",25
```

> Row 1 has 2 cells but the header declares 3 fields. This is a malformed row and MUST be rejected. Cell omission cannot imply `null`.

Nested values MAY be carried in one cell by encoding that cell as a TOON string
containing JSON text. This is an escape hatch for values that are not flat
records; it does not change the row arity rule.

**Example with nested JSON:**

```toonl
[]{id,metadata}:
1,"{\"tags\":[\"a\",\"b\"],\"color\":\"red\"}"
2,"{\"tags\":[],\"color\":\"blue\"}"
```

The `metadata` field contains JSON text encoded as a TOON string. Each row still has exactly 2 cells.

## Encoding

TOONL streams MUST be UTF-8 text. Encoders MUST emit LF line endings.

Rows are flush-left. Encoders MUST NOT prefix row lines with TOON's two-space
tabular indentation while the stream is open. (Indentation is applied by the
[close-transform](#close-transform) when a segment is materialized into a TOON
document.)

**Valid TOONL encoding (flush-left rows):**

```toonl
[]{id,msg}:
1,hello
2,world
```

**Invalid TOONL encoding (indented rows in open stream):**

```toonl
[]{id,msg}:
  1,hello
  2,world
```

> Rows MUST NOT be indented while the stream is open. Indentation is only applied by the close-transform.

Blank lines MAY appear between segment constructs. Decoders MUST ignore blank
lines. Encoders MUST NOT emit blank lines.

**Valid TOONL with blank lines (decoders ignore them):**

```toonl
[]{id,msg}:
1,hello

2,world

[=2]
```

The blank lines are ignored; the segment contains 2 rows.

Comments do not exist in TOONL. A `#` character has no comment meaning and is
cell data unless TOON cell quoting gives it another meaning.

**Example where `#` is cell data:**

```toonl
[]{id,tag}:
1,"#important"
2,normal
```

The `#important` cell is quoted TOON string data, not a comment.

Lines beginning with `- ` are **reserved** for a future nested-frame syntax. A
decoder MUST reject any non-blank line whose first two bytes are `- `.

**Invalid example (reserved prefix):**

```toonl
[]{id,msg}:
- reserved for future use
```

> Any line starting with `- ` MUST be rejected as reserved.

## Grammar

The following ABNF is normative except where it delegates cell parsing to TOON
v3.3. The v0.1 base grammar is given first; the v0.2 additions follow. Constructs
not shown in the additions are unchanged from the base.

```abnf
; --- v0.1 base ---
stream     = *( segment / blank-line )
segment    = header *( row / blank-line ) [ trailer ]
header     = "[" [ delim-sym ] "]" "{" field *( delim field ) "}" ":" LF
row        = cell *( delim cell ) LF
trailer    = "[=" 1*DIGIT "]" LF
blank-line = LF

delim-sym  = "|" / HTAB          ; note: "~" is NOT a delim-sym
delim      = "," / "|" / HTAB
field      = 1*( %x21-7E )        ; interpreted with TOON key quoting rules
cell       = <TOON primitive token using the active delimiter>

; --- v0.2 additions ---
stream     =/ *( decl / cont-header / tagged-row )
cont-header = "[~]" "{" field *( delim field ) "}" ":" LF   ; R1 continuation header
decl        = "[]" "<" tag ">" "{" field *( delim field ) "}" ":" LF  ; R3 named schema
tagged-row  = tag ":" cell *( delim cell ) LF                ; R3 tagged row
tag         = 1*( ALPHA / DIGIT / "_" / "-" )
              ; MUST NOT contain the active delimiter, ":", "{", "}", "[", "]", or "~"
```

An absent `delim-sym` selects comma. `|` selects pipe. HTAB selects tab. The
active delimiter applies to the field list and every row in the segment.

**Delimiter examples:**

```toonl
[]{a,b,c}:
1,2,3

[|]{a|b|c}:
1|2|3

[	]{a	b	c}:
1	2	3
```

Field names and cell tokens MUST follow TOON v3.3 key, scalar, and cell quoting
rules for the active delimiter. A cell containing the active delimiter MUST be
quoted as TOON requires.

**Example where cell requires quoting (contains the active delimiter):**

```toonl
[]{id,value}:
1,"contains,comma"
2,"normal"
```

The first row has a cell `"contains,comma"` which must be quoted because it contains the comma delimiter.

Note that `delim-sym` deliberately does **not** include `~`; the `[~]`
continuation header is a distinct production, and that exclusion is what makes a
v0.1 decoder reject `[~]{...}:`.

## Headers And Rotation

The canonical header form is:

```toonl
[]{field1,field2}:
```

The empty `[]` is intentional. Decoders MUST NOT accept `{field1,field2}:` as a
TOONL header, because that shape can be silently misparsed by TOON decoders as an
ordinary object key.

**Valid header examples:**

```toonl
[]{ts,level,msg}:
[|]{name|value}:
[]	{a	b	c}:
```

**Invalid header examples (MUST be rejected):**

```toonl
{ts,level,msg}:
[~]{ts,level,msg}:
[]{ts level msg}:
```

> The first lacks the `[]` bracket syntax. The second uses `~` which is only valid as v0.2 continuation header syntax. The third lacks proper field delimiters.

A new header starts a new segment (**schema rotation**). If a new header appears
while a previous segment is open, the previous segment is closed without trailer
verification, and the new header defines the next segment's schema.

**Example of schema rotation:**

```toonl
[]{ts,event}:
2026-07-14T03:00:00Z,start
2026-07-14T03:00:01Z,ready
[]{ts,event,duration_ms}:
2026-07-14T03:00:02Z,complete,2000
```

The first segment has 2 rows with fields `ts` and `event`. The second header opens a new segment with 3 fields, rotating the schema. This is valid TOONL.

## Trailers

A trailer has the form:

```
[=N]
```

`N` is the number of rows since the current segment header. If a trailer is
present, a decoder MUST verify that `N` equals the segment row count and MUST
reject the stream if it does not.

**Valid trailer examples:**

```toonl
[]{ts,event}:
2026-07-14T03:00:00Z,start
2026-07-14T03:00:01Z,ready
[=2]
```

The trailer `[=2]` verifies that exactly 2 rows follow the header.

**Invalid trailer example (MUST be rejected):**

```toonl
[]{ts,event}:
2026-07-14T03:00:00Z,start
2026-07-14T03:00:01Z,ready
[=3]
```

> The trailer claims 3 rows but only 2 follow the header. A compliant decoder rejects this.

Clean-EOF emitters SHOULD write a trailer for the final segment. Interrupted,
still-open append streams MAY end without a final trailer; consumers MUST treat
that final segment as unverified.

**Example of unverified open stream:**

```toonl
[]{ts,event}:
2026-07-14T03:00:00Z,start
2026-07-14T03:00:01Z,ready
```

This stream has no trailer. It is valid TOONL but unverified; a reader knows the segment may still receive more rows.

## Close-Transform

The close-transform maps closed TOONL segments to TOON v3.3 documents. It is
deterministic, order-preserving, and requires one O(n) pass over the input.

For each segment, the close-transform MUST:

1. Count the segment rows.
2. If a trailer is present, verify its count.
3. Rewrite the TOONL header from `[]...` to a TOON header with the materialized
   count `[N]...`.
4. Indent every row with two spaces.
5. Discard the trailer.

A single-segment stream closes to exactly one TOON document. A multi-segment
stream closes to a sequence of TOON documents, one document per segment, in
stream order. (The tagged-multiplexing variants of the close-transform are
defined in [R3](#r3--tagged-row-multiplexing).)

Example:

```toonl
[]{ts,level,msg}:
"2026-07-14T03:00:00Z",info,boot
"2026-07-14T03:00:02Z",error,"disk full"
[=2]
[]{ts,level,msg,request_id}:
"2026-07-14T03:01:00Z",info,retry,null
[=1]
```

closes to the document sequence:

```toon
[2]{ts,level,msg}:
  "2026-07-14T03:00:00Z",info,boot
  "2026-07-14T03:00:02Z",error,"disk full"
```

```toon
[1]{ts,level,msg,request_id}:
  "2026-07-14T03:01:00Z",info,retry,null
```

## Closure Properties (v0.2)

v0.2 elevates three closure properties to first-class data-model guarantees.
Every v0.2 capability rests on them, and a conforming v0.2 implementation MUST
preserve them. They were already true of v0.1 streams; v0.2 makes them
normative.

### Suffix-closure

Let `S` be a valid TOONL stream, let `b` be any row boundary in `S`, and let `H`
be the verbatim active header line in effect at `b`. Then the stream formed by
concatenating `H` with the byte suffix of `S` starting at `b` is a valid TOONL
stream, and it decodes to exactly the sequence of rows that `S` produces from `b`
onward, under the schema `H` declares.

Informally: **any row-boundary suffix, re-prefixed with the header that was
active there, is a valid stream equivalent to the tail it was cut from.** This is
what makes trimming (R2) and resuming (R1) safe.

**Example of suffix-closure:**

Given this stream:

```toonl
[]{ts,level}:
2026-07-14T03:00:00Z,info
2026-07-14T03:00:01Z,error
2026-07-14T03:00:02Z,info
```

If we cut at the row boundary before `2026-07-14T03:00:02Z,info` and re-prefix with the active header `[]{ts,level}:\n`, we get:

```toonl
[]{ts,level}:
2026-07-14T03:00:02Z,info
```

This is a valid TOONL stream that decodes to exactly what the original stream produces from that boundary onward.

Two constraints make suffix-closure well-defined:

1. The cut MUST be at a row boundary. A suffix that begins in the middle of a
   line is not covered by this guarantee.
2. The prefixed header MUST be the *verbatim* active header line — the exact
   bytes, including the active delimiter symbol. Re-deriving or re-serializing the
   header is not permitted, because a re-serialization could change the delimiter
   or field quoting and thereby change how the suffix rows parse.

A trailer (`[=N]`) that appeared before the cut is not carried into the suffix;
the suffix is an open segment until it is closed. See [Trailers Under
Trimming](#trailers-under-trimming).

### Concatenation closure

Let `A` and `B` be valid TOONL streams. Then the byte concatenation `A || B` is a
valid TOONL stream. It decodes to the rows of `A` followed by the rows of `B`. If
`B` begins with a header line (as a self-contained stream normally does), that
header opens a fresh segment — an ordinary schema rotation at the seam.

Concatenation closure is what makes `>>` append safe, makes multi-file
concatenation (`cat a.toonl b.toonl`) a valid stream, and underpins the
side-journal pattern (R4). Because every valid stream ends on a row boundary,
concatenation is always byte-valid.

**Example of concatenation closure:**

Stream A:

```toonl
[]{id,name}:
1,alice
2,bob
```

Stream B:

```toonl
[]{id,name}:
3,charlie
```

Concatenation `A || B` produces:

```toonl
[]{id,name}:
1,alice
2,bob
[]{id,name}:
3,charlie
```

This is a valid 2-segment TOONL stream with a schema rotation at the seam. The rows decode in order: `1,alice`, `2,bob`, `3,charlie`.

### Header-on-open discipline

To make suffix-closure and concatenation closure hold for **appending writers**,
v0.2 imposes a discipline:

> A writer that opens a stream for append MUST emit its schema header before its
> first row on that open.

This is the **header-on-open** rule. Its consequence is that a stream never
contains a run of rows that is not immediately preceded, within the same open, by
the header that governs them. A reader that starts at any point produced by a
fresh open therefore always finds a header before the rows.

Header-on-open is an **idempotent rotation**: emitting the active header again
when it is byte-identical to the one already in effect opens a new segment with
the same schema and is *semantically transparent* — the rows that follow decode
identically whether or not the redundant header was emitted. Because it is
idempotent, a writer MAY emit the header on every open without tracking whether
the previous open left the same schema active.

The close-transform and `tq` MAY **coalesce adjacent identical headers**: when a
header line is byte-identical to the header of the immediately preceding segment
and no rows intervene, the transform MAY drop the redundant header and merge the
two segments. Coalescing is OPTIONAL and MUST NOT change the row sequence or the
per-segment row counts of the closed output beyond removing an empty segment. A
trailer between two identical headers with no rows counts as an empty segment
(`[=0]`) and MAY likewise be coalesced away.

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

The second `[]{ts,level,msg}:` is an idempotent rotation. A reader starting at
any row boundary — including the one just before the second header — always has a
header in front of it. The close-transform MAY coalesce the two identical headers
into a single three-row segment, or MAY keep them as a two-row and a one-row
segment; both are conforming and carry the same rows.

## R1 — Resumable Readers (v0.2)

R1 lets a reader stop at a row boundary, remember where it was, and later resume
without rescanning the whole stream, while remaining safe against truncation and
rewrite.

### Cursor convention

A **resume cursor** is the triple:

```
{ byteOffset, activeHeaderLine, rowsSinceHeader }
```

- `byteOffset` — a **row boundary**: the byte offset at which the next unread row
  begins. MUST be a row boundary.
- `activeHeaderLine` — the **verbatim** active header line in effect at
  `byteOffset` (exact bytes, including the terminating LF). Stored verbatim
  precisely so that a resume can reconstruct a suffix-closed stream without
  re-serializing the header.
- `rowsSinceHeader` — the number of rows consumed since `activeHeaderLine` became
  active, up to `byteOffset`. Used for diagnostics and to re-derive a trailer
  count if the reader also needs to close the resumed suffix.

**Example cursor:**

```json
{
  "byteOffset": 47,
  "activeHeaderLine": "[]{ts,level}:\n",
  "rowsSinceHeader": 2
}
```

This cursor records that at byte offset 47 (a row boundary), the active header is `[]{ts,level}:\n`, and 2 rows have been consumed since that header became active.

A reader MAY persist this cursor by any means (a sidecar file, a database column,
a message-queue offset). The convention is the *shape* of the cursor and its
guarantee, not a storage format.

### Resume guarantee

> Decoding the stream starting at `byteOffset`, treating `activeHeaderLine` as
> the active header, yields exactly the row sequence that a sequential scan from
> the beginning of the stream would produce starting at that row boundary.

This is a direct corollary of [suffix-closure](#suffix-closure): the cursor names
a row boundary and the verbatim active header, which is precisely what
suffix-closure requires. A resuming reader constructs the logical stream
`activeHeaderLine || suffix(byteOffset)` and decodes it.

### Invalidation conditions

A cursor MUST be treated as **invalid** — and the reader MUST fall back to a full
rescan (or an error, per the reader's policy) — if any of the following hold when
the reader attempts to resume:

1. **Truncation**: the current file size is less than `byteOffset`. The bytes the
   cursor pointed past no longer exist.
2. **Anchor mismatch**: the bytes at a remembered anchor no longer match. A
   reader that remembered anchor bytes (for example, the `activeHeaderLine` at its
   recorded offset, or a byte fingerprint immediately before `byteOffset`) MUST
   re-read those bytes and compare; if they differ, the underlying stream was
   rewritten rather than only appended, and the cursor is invalid.

**Examples of invalidation:**

Cursor points to byte 47, but the file is now 30 bytes → **Truncation**, cursor invalid.

Cursor records `activeHeaderLine = "[]{ts,level}:\n"` at offset 0, but re-reading the file shows `[]{ts,level,request_id}:\n` → **Anchor mismatch**, cursor invalid.

A reader SHOULD store at least one anchor sufficient to detect rewrite-in-place
(the two conditions catch shrink and mutate respectively). A cursor is valid only
for a stream that has been **append-only** since the cursor was taken; any
in-place rewrite invalidates it. This is consistent with R4 declaring in-place
splice a non-goal.

> **Caveat:** Appending rows to a file is always safe. Modifying or deleting existing rows invalidates all cursors into that stream.

### OPTIONAL continuation header

For long-lived, single-segment streams — a `tail -f`-style log that may run for
days under one schema — a reader that arrives late has to scan backward an
unbounded distance to find the one header at the top. TOONL defines an OPTIONAL
**continuation header** to bound that scan.

Syntax:

```
[~]{field1,field2}:
```

A continuation header uses the `~` sentinel in the bracket where an anonymous
header has nothing and a tagged declaration (R3) has a tag. Its rules:

- A continuation header MUST be byte-equal to the currently active header in
  every respect except the `~` sentinel — same delimiter symbol, same field list,
  same quoting. A decoder MUST reject a continuation header whose fields or
  delimiter differ from the active header.
- A continuation header **is not a rotation**. It does not open a new segment,
  does not reset `rowsSinceHeader` for trailer purposes, and does not change the
  active schema. It is a re-assertion of the active header for late readers.
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
cursor `{ byteOffset = <offset of the 2026-07-14T03:00:03Z,4 line>,
activeHeaderLine = "[]{ts,seq}:\n", rowsSinceHeader = 0 }`. Closing the resumed
suffix drops the `[~]` line and yields:

```toon
[2]{ts,seq}:
  "2026-07-14T03:00:03Z",4
  "2026-07-14T03:00:04Z",5
```

## R2 — Header-Preserving Trim (v0.2)

R2 defines **keep-last-N** on top of suffix-closure: bound a stream to its most
recent N rows while keeping it a valid, self-describing stream.

### keep-last-N algorithm

Given a valid TOONL stream `S` and a cap `N` (rows), produce a trimmed stream
`S'`:

1. **Count in rows.** Scan `S` and identify row boundaries. The cap `N` counts
   **rows only**; headers, trailers, and blank lines are not counted.
2. **Choose the cut.** If `S` has `M` rows and `M <= N`, `S' = S` (no trim
   needed). Otherwise let the cut be the row boundary immediately before the
   `(M − N + 1)`-th row — i.e. the boundary such that exactly `N` rows follow it.
3. **Cut at a row boundary.** The cut MUST be a row boundary (never mid-line), so
   suffix-closure applies.
4. **Determine the active header at the cut.** Find the verbatim active header
   line in effect at the cut boundary.
5. **Emit the synthesized stream.** Write the verbatim active header line, then
   the retained byte suffix from the cut boundary to EOF. Per suffix-closure this
   is a valid stream carrying exactly the last `N` rows.
6. **Apply the trailer rule** (below) to the retained suffix.

**Example of keep-last-N:**

Original stream with 5 rows, trim to keep-last-3:

```toonl
[]{ts,event}:
1,a
2,b
3,c
4,d
5,e
```

Step 2: M=5, N=3, so cut before the 3rd row (before `3,c`). The cut is at the row boundary after `2,b`.

Step 4: The active header at that boundary is `[]{ts,event}:\n`.

Step 5: Emit `[]{ts,event}:\n` + suffix from after `2,b`:

```toonl
[]{ts,event}:
3,c
4,d
5,e
```

The synthesized header is a **verbatim** copy of the active header line — the
same bytes, same delimiter symbol — not a re-serialization. This preserves the
exact delimiter and quoting the retained rows were written against.

If the retained suffix crosses one or more schema rotations, the retained bytes
already contain those interior headers; step 5 only prepends the header active at
the cut. The result is a multi-segment stream whose first segment is headed by
the synthesized header and whose later segments are the rotations already
present.

### Trailers under trimming

A trailer states the row count of *its* segment. After a trim, the count of the
first retained segment usually changes, so a stale trailer would be wrong. The
rule is **drop-or-recount**:

- If the **original segment that the cut falls within had no trailer**, the
  synthesized first segment MUST NOT gain one. The trim only re-heads a suffix; it
  does not invent verification the producer never provided.
- If the **original segment that the cut falls within had a trailer**, then the
  trailer for the synthesized first segment MUST be **recounted** to the number of
  retained rows in that first segment, or dropped. An implementation MUST NOT copy
  the original trailer's count unchanged onto the trimmed segment.

Interior segments fully contained in the retained suffix keep their original
trailers unchanged, because their row counts did not change.

### Atomic write

A trim that replaces a file in place MUST be **atomic**: write the trimmed stream
to a temporary file in the same directory, then `rename` it over the original. A
reader observing the file MUST see either the pre-trim stream or the post-trim
stream, never a partial write. (A reader holding a cursor into the pre-trim
stream will observe its [invalidation conditions](#invalidation-conditions) after
the rename — file size shrinks and/or anchor bytes move — and fall back to
rescan, which is correct.)

### `tq trim --keep-last N` verb contract

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

`tq trim --keep-last 2 app.toonl` emits (the original segment had a trailer, so
the first retained segment's trailer is recounted to `2`):

```toonl
[]{ts,level,msg}:
2026-07-14T03:00:02Z,error,"disk full"
2026-07-14T03:00:03Z,info,recovered
[=2]
```

If the original had **no** `[=4]` trailer, the output would be the same two lines
under the header with **no** trailer.

## R3 — Tagged-Row Multiplexing (v0.2)

R3 lets a single stream carry more than one record shape interleaved, without
paying any cost when the stream has a single shape.

### Named schema declarations and tagged rows

A **named schema declaration** binds a short tag to a schema:

```toonl
[]<tag>{field1,field2}:
```

The `<tag>` sits in the bracket, between `[]` and `{...}`. A **tagged row**
references a declared tag:

```
<tag>:cell1,cell2
```

A tagged row's cells bind positionally to the fields of the schema declared for
`<tag>`, exactly as untagged rows bind to the anonymous schema.

**Example of named schemas and tagged rows:**

```toonl
[]<user>{id,name}:
[]<product>{sku,price}:
user:1,alice
product:ABC,50
user:2,bob
```

The stream declares two named schemas: `user` and `product`. Rows prefixed with `user:` bind to the `user` schema (2 fields); rows prefixed with `product:` bind to the `product` schema (2 fields). This is valid TOONL with two interleaved record shapes.

A tag MUST match the same character class (see [Grammar](#grammar)) in a
declaration and in the rows that reference it. A `<tag>:` prefix on a row is
distinguished from an anonymous row by the presence of a bare tag token followed
by `:` before the first cell; because anonymous rows are TOON cells and a leading
`tag:` would be TOON-quoted if it were data, an unquoted `tag:` prefix is
unambiguous. A decoder MUST reject a tagged row whose tag has not been declared by
an in-scope `[]<tag>{...}:` declaration.

**Invalid example (tag used before declaration):**

```toonl
[]<user>{id,name}:
product:ABC,50
```

> The row `product:ABC,50` references tag `product` which was never declared. This MUST be rejected.

### Bounded live-schema table

A decoder maintains a **live-schema table** mapping in-scope tags to their
schemas, plus the single anonymous schema. The table is **bounded**: an
implementation MUST support at least **8** simultaneously-live tagged lanes, and
MUST reject a stream that declares a 9th distinct live tag beyond the supported
bound rather than growing without limit. The anonymous schema does not count
against the tagged-lane bound.

**Example of hitting the bound:**

A stream declares 9 distinct tags:

```toonl
[]<t1>{a}:
[]<t2>{b}:
[]<t3>{c}:
[]<t4>{d}:
[]<t5>{e}:
[]<t6>{f}:
[]<t7>{g}:
[]<t8>{h}:
[]<t9>{i}:
```

An implementation supporting the minimum bound of 8 live lanes MUST reject the 9th declaration `[]<t9>{i}:` as exceeding the limit.

The bound is a robustness limit (a multiplexed stream with hundreds of live lanes
is almost certainly a producer bug or an attack), not a target; producers SHOULD
keep the number of live lanes small.

> **Caveat:** Do not design systems that rely on redefining a lane to evict an old tag. Keep the number of distinct tags small.

### Redefinition is the rotation

Re-declaring an already-live tag with a new `[]<tag>{...}:` **rotates that lane's
schema**: rows tagged with that tag after the redeclaration bind to the new
schema. This is the tagged analogue of anonymous schema rotation. Redefinition
does not free a lane; it replaces the schema in the existing lane slot, so it does
not push the table over the bound.

**Example of schema rotation in a lane:**

```toonl
[]<log>{ts,level}:
log:2026-07-14T03:00:00Z,info
log:2026-07-14T03:00:01Z,error
[]<log>{ts,level,request_id}:
log:2026-07-14T03:00:02Z,info,null
```

The tag `log` is redeclared with 3 fields instead of 2. Rows after the redeclaration bind to the new schema. This lane rotates its schema but remains a single lane in the table.

### Untagged rows keep v0.1 semantics

An **untagged row** binds to the sole **anonymous** schema — the schema most
recently declared by an anonymous `[]{...}:` header — exactly as in v0.1. A
stream that never uses a tagged declaration or a tagged row is a v0.1 stream and
behaves identically. **Single-shape streams pay nothing**: no tag byte, no lane,
no table growth. The anonymous schema and tagged lanes coexist: a stream MAY
interleave untagged rows with tagged rows.

**Example: v0.1 stream (no tags, backward compatible):**

```toonl
[]{id,name}:
1,alice
2,bob
```

This is a valid v0.1 stream. v0.2 readers accept it identically.

**Example: mixed untagged and tagged rows:**

```toonl
[]{event}:
[]<error>{code,message}:
started
error:500,"server error"
finished
```

The stream has an anonymous schema `[]{event}:` and a named schema `error`. Untagged rows `started` and `finished` bind to the anonymous schema. The tagged row `error:500,"server error"` binds to the `error` schema.

### Canonical per-shape field order

To keep multiplexed output deterministic and diffable, **encoders MUST emit a
canonical field order per shape.** For each distinct shape an encoder produces,
the field order in the declaration MUST be either **sorted** (e.g. lexicographic)
or **first-seen** (the order fields first appeared for that shape), chosen once
and applied consistently for that shape across the stream. An encoder MUST NOT
reorder a shape's fields arbitrarily between declarations of the same shape.
Decoders do not require canonical order to parse (fields bind positionally), but
the requirement makes encoder output stable.

**Valid example (consistent sorted order):**

```toonl
[]<user>{age,id,name}:
[]<product>{price,sku}:
user:30,1,alice
product:50,ABC
user:25,2,bob
[]<user>{age,id,name}:
user:29,3,charlie
```

Each time `user` is redeclared, the fields are in alphabetical order: `age,id,name`. Each time `product` is redeclared, the fields are in sorted order: `price,sku`.

**Invalid example (inconsistent order):**

```toonl
[]<user>{id,name}:
[]<user>{name,id}:
```

> The `user` shape is redeclared with fields reordered. This violates the canonical order requirement and MUST be rejected or treated as a redefinition (schema rotation) per the implementation.

### Close-transform for tagged streams

The close-transform for a multiplexed stream has two defined forms. A tool MUST document which form it emits; the per-lane form is the default when unspecified.

**Per-lane form (default).** Produce **one TOON document per lane**, in the order
each lane's schema was first declared in the stream. The anonymous schema, if any
rows bound to it, is one such lane. Within a lane, rows appear in stream order.
Each lane closes exactly as a v0.1 segment closes. A lane that rotated schema
mid-stream closes to multiple TOON documents for that lane, one per schema epoch,
in stream order.

**Example — per-lane close:**

Given this stream:

```toonl
[]<req>{method,path}:
[]<metric>{name}:
req:GET,/health
metric:cpu
req:POST,/login
metric:mem
```

Per-lane close produces two TOON documents (one per lane, in declaration order):

```toon
[2]{method,path}:
  GET,/health
  POST,/login
```

```toon
[2]{name}:
  cpu
  mem
```

**Interleave-preserving form (variant).** Produce a single sequence that
preserves the original interleaving of rows across lanes, for consumers that need
to reconstruct the exact temporal order. It is defined as: for each maximal run of
consecutive rows that share a lane, emit one TOON document for that run (header
from the run's schema, count = run length, rows indented). Runs appear in stream
order.

**Example — interleave-preserving close:**

Using the same stream, interleave-preserving close produces four TOON documents (one per maximal run):

```toon
[1]{method,path}:
  GET,/health
```

```toon
[1]{name}:
  cpu
```

```toon
[1]{method,path}:
  POST,/login
```

```toon
[1]{name}:
  mem
```

> **Caveat:** Document which close-form your tool uses. Consumers expect one of the two forms; an undocumented mix will cause data misinterpretation.

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

The reservation stands: lines beginning with `- ` remain **reserved for a future
nested-frame syntax**. A decoder MUST reject any non-blank line whose first two
bytes are `- `. Tags MUST NOT begin with `- `, and the tag character class
excludes the space that would make a `- ` prefix, so tagged rows never collide
with the reservation.

**Example of reserved prefix collision (MUST be rejected):**

```toonl
[]{id,msg}:
1,hello
- nested frame (reserved, not allowed)
2,world
```

> The line `- nested frame (reserved, not allowed)` starts with `- ` and MUST be rejected.

**Valid tagged row examples (no collision with reservation):**

```toonl
[]<req>{method}:
req:GET
req-pending:POST
req_retry:DELETE
```

Tags like `req`, `req-pending`, and `req_retry` do not start with `- ` and are valid.

## R4 — In-Place Splice Non-Goal And The Side-Journal Pattern (v0.2)

### Splice non-goal

**In-place row splice — mutating, replacing, or deleting an individual row within
a stream's existing bytes — is an explicit non-goal of TOONL and is NOT
specified.**

Rationale:

- In-place mutation breaks [suffix-closure](#suffix-closure) and the R1 resume
  guarantee: a cursor's validity rests on the stream being append-only since the
  cursor was taken, and every anchor/truncation check assumes rewrite
  invalidates. Making splice first-class would force every reader to defend
  against arbitrary interior mutation, which defeats cheap resumption.
- It breaks the self-checking property: trailers count rows in a segment;
  splicing a row either invalidates the trailer or forces a rewrite of the whole
  downstream, which is no longer a splice.
- Append-only streams are the whole point of the format. Interior edit is the
  property JSONL-style logs deliberately give up in exchange for cheap append,
  tail, and concatenation.

**Example of what NOT to do (in-place mutation):**

```toonl
[]{id,status}:
1,pending
2,pending
3,completed
```

Do NOT modify the file to change the second row to `2,completed`. This breaks cursors and trailers. Instead, append a correction record.

Consumers that need to "change" a row MUST express it as a new appended row (a
correction record) plus application-level last-writer-wins semantics, or MUST
rebuild the stream from a source of truth — not by editing bytes in place.

**Correct approach: append a correction record:**

```toonl
[]{id,status}:
1,pending
2,pending
3,completed
2,completed
```

A consumer with last-writer-wins semantics will treat the second `2,completed` as the authoritative state for id `2`.

### The side-journal pattern (blessed retry / re-queue)

The blessed pattern for retry and re-queue is a **side journal**: a separate file
that carries the retries, drained ahead of the main stream.

- Retries are written to a separate `.retry` stream (e.g. `app.toonl.retry`),
  **with its own header**, using the same header-on-open discipline as any
  writer. The `.retry` file is a self-contained, valid TOONL stream in its own
  right.
- A consumer **drains the `.retry` stream ahead of the main stream**: it processes
  the side journal first, then the main stream, so retried work is re-attempted
  before new work.
- Logically, processing is `retry-journal || main-stream` — a valid stream by
  [concatenation closure](#concatenation-closure). The consumer never splices a
  retried row back into the main stream's bytes.
- Because each `.retry` writer follows
  [header-on-open](#header-on-open-discipline), the side journal is safe to append
  to from a reopened producer, safe to concatenate, and safe to trim (R2) and
  resume (R1) exactly like a main stream.

**Step-by-step example of the side-journal pattern:**

1. Main stream `jobs.toonl` starts:

```toonl
[]{id,task}:
1,encode
2,upload
```

2. Consumer processes job 1 (success), then job 2 (fails). It re-queues job 2 to `jobs.toonl.retry`:

```toonl
[]{id,task}:
2,upload
```

3. Consumer's effective input is the logical concatenation `jobs.toonl.retry || jobs.toonl`:

```toonl
[]{id,task}:
2,upload
[]{id,task}:
1,encode
2,upload
```

4. This closes to the row sequence: `2,upload` (from retry), then `1,encode`, `2,upload` (from main).

5. Consumer applies last-writer-wins deduplication: job `2` appears twice; the final state is `upload`. Job `1` succeeds once.

6. To clear the retry journal after all retries succeed, truncate or delete `jobs.toonl.retry`.

This pattern rests entirely on **concatenation closure + header-on-open**: no new
wire syntax is required, and no interior mutation ever occurs.

> **Caveat:** The side-journal pattern requires application-level deduplication (e.g. idempotency keys or last-writer-wins). TOONL does not provide automatic dedup; it is the consumer's responsibility.

#### Worked example — side journal

Main stream `jobs.toonl`:

```toonl
[]{id,payload}:
1,alpha
2,beta
```

A consumer fails job `2` and re-queues it into `jobs.toonl.retry`, its own valid
stream with its own header:

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

(De-duplication of the re-tried `2` is an application concern — last-writer-wins
or an idempotency key — not a stream-format concern.)

## Versioning And Compatibility

TOONL v0.2 is a strict superset of v0.1; this document is the single normative
text for both. The compatibility contract is:

1. **v0.2 readers MUST accept v0.1 streams unchanged.** A stream that uses only
   v0.1 constructs decodes identically under a v0.2 reader; v0.2 changes no v0.1
   semantics.

**Example of v0.1 stream (accepted by v0.2 unchanged):**

```toonl
[]{id,name}:
1,alice
2,bob
[=2]
```

2. **v0.1 decoders MUST cleanly reject v0.2-only constructs.** The v0.2-only
   constructs are the continuation header `[~]{...}:`, named schema declarations
   `[]<tag>{...}:`, and tagged rows `<tag>:...`. A v0.1 decoder MUST reject each
   rather than silently mis-parsing:
   - `[~]{...}:` — the `~` is not a valid v0.1 delimiter symbol (v0.1 allows only
     an empty symbol, `|`, or HTAB inside `[]`), so a v0.1 header parser rejects it.
   - `[]<tag>{...}:` — the `<tag>` between `]` and `{` is not part of the v0.1
     header grammar, so a v0.1 header parser rejects it.
   - `<tag>:...` — a v0.1 decoder parses this as an anonymous row; the leading
     `tag:` is TOON cell data. Because a v0.1 reader has no lane for it and the
     arity will not match the anonymous schema in the general case, it is rejected
     as a malformed row. Producers MUST NOT rely on a v0.1 reader interpreting a
     tagged row as anything meaningful; v0.2-only streams are for v0.2 readers.

**Examples of v0.2-only constructs (MUST be rejected by v0.1 decoders):**

Continuation header (v0.2 R1):

```toonl
[]{ts,event}:
2026-07-14T03:00:00Z,start
[~]{ts,event}:
2026-07-14T03:00:01Z,ready
```

Named schema and tagged row (v0.2 R3):

```toonl
[]<user>{id,name}:
user:1,alice
```

### Structural version signaling

A TOONL stream signals that it requires a v0.2 reader **structurally, by using a
v0.2-only construct** — a continuation header, a named schema declaration, or a
tagged row. There is no version banner line and none is required: a stream that
uses no v0.2-only construct is, by definition, a v0.1 stream and is readable by
v0.1 and v0.2 readers alike.

**Example of structural v0.2 signal:**

This stream requires a v0.2 reader because it uses a named schema declaration:

```toonl
[]<error>{code,message}:
error:500,"internal error"
```

A v0.1 decoder will reject `[]<error>{code,message}:` as malformed header.

This stream is v0.1-compatible (no v0.2-only constructs):

```toonl
[]{id,name}:
1,alice
[=1]
```

Both v0.1 and v0.2 decoders accept it identically.

Producers that want an explicit, human-visible signal MAY document the v0.2
requirement out of band (file naming convention, catalog metadata, or the media
type parameter `application/toonl; version=0.2`). Such out-of-band signaling is
OPTIONAL and MUST NOT be required for correctness; the structural rule above is
authoritative. A decoder MUST NOT depend on an out-of-band signal to decide
whether to accept v0.2 constructs — it accepts them iff it is a v0.2 decoder.

## Relationship To TOON v3.3

- TOONL does not change the TOON v3.3 document specification. The close-transform
  continues to map closed TOONL segments to valid TOON v3.3 documents.
- The reserved `- ` line prefix stays reserved for a future nested-frame syntax.
- The reddb-io TOON extensions (nested tabular headers, keyed-map collapse) are an
  independent concern; see
  [`toon-reddb-spec.md`](toon-reddb-spec.md). The close-transform
  targets canonical TOON v3.3 and does not emit those forms.
- For an annotated walk through the official TOON v3.3 specification and how our
  implementations conform to it, see [`toon-official-spec.md`](toon-official-spec.md).

## Conformance

The shared executable corpus under `tests/toonl/fixtures/` covers decode, encode,
close-transform, and required error cases for the v0.1 base and every v0.2
capability, and pins the Rust crate and the JS package to identical behavior.
Implementations SHOULD treat those fixtures as executable examples for TOONL
behavior.
