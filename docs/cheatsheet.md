# TOON cheatsheet

A one-page reference: the official TOON v4.1 syntax first, then the reddb-io
opt-in extensions. The normative text is the
[pinned official specification](toon-official-spec.md); the extensions are
specified in [RedDB TOON extensions](toon-reddb-spec.md) and the stream form in
[TOONL](toonl-reddb-spec.md). Every example below is the canonical output of
`@reddb-io/toon`, and the Rust crate emits the same bytes.

## Official TOON v4.1

| JSON shape | TOON |
| --- | --- |
| Object | `name: Ada` on its own line; nested objects indent by two spaces |
| Primitive array | `tags[2]: x,y` |
| Array of uniform objects (tabular form) | `users[2]{id,name}:` then one row per object |
| Uniform nested object in a row (nested field group) | `orders[2]{id,customer{name,country}}:` |
| Object whose values share a shape (keyed tabular form) | `people[2:]{first,last}:` then `ada: Ada,Lovelace` rows |
| Mixed or non-uniform array (list form) | `items[3]:` then `- ` items |
| Empty array / empty object / empty string | `a: []` / `b:` / `c: ""` |
| Root primitive | the bare token, e.g. `42` or `"quoted text"` |

```toon
id: 1
name: Ada
active: true
tags[2]: x,y
users[2]{id,name}:
  1,Ada
  2,Linus
orders[2]{id,customer{name,country}}:
  1,Ada,UK
  2,Linus,FI
people[2:]{first,last}:
  ada: Ada,Lovelace
  grace: Grace,Hopper
items[4]:
  - 1
  - a: 1
  - [2]: 1,2
  - x
```

### Rules worth remembering

- **Lengths are declared.** `[N]` is the item count; a decoder rejects a
  mismatch, which is what makes truncated model output detectable.
- **Delimiters.** Comma by default. A header selects tab or pipe for its rows
  and inline values: `rows[1|]{a|b}:` or `rows[1<TAB>]{a<TAB>b}:`.
- **Quoting.** A string is quoted when it is empty, has leading or trailing
  space or tab, looks like `true`/`false`/`null` or a number, contains
  `: " \ [ ] { }`, a control character or the active delimiter, or starts with
  `-` or `#`. A root string that starts with U+FEFF is also quoted, because a
  document-leading U+FEFF is a byte-order mark.
- **Escapes.** Only `\\ \" \n \r \t` and `\uXXXX`.
- **Whitespace.** Indentation is spaces only, two per level by default. Token
  trimming removes U+0020 and nothing else, so NBSP, U+3000 or a tab inside a
  cell is content.
- **Comments.** A full line whose first non-space character is `#`. There are
  no inline comments.
- **Numbers.** Canonical decimal form: no leading zeros, no trailing fractional
  zeros, `-0` is `0`, exponent form below `1e-6` and from `1e21` up
  (`5e-324`, `1e+21`).

## reddb-io opt-in extensions

Extensions are opt-in at the SDK and CLI; the wire carries no flag. A decoder
recognizes array columns by their header shape, and cyclic arrays only when
`cyclicDiscriminatedArrays` is enabled.

| Extension | Encode option | Header shape |
| --- | --- | --- |
| Primitive-array columns | `primitiveArrayColumns` | `rows[2]{id,tags[;]}:` then `1,a;b` |
| Object-array columns (child tables) | `objectArrayColumns` | `rows[2]{id,items{sku,q}}:` then a count cell and indented child rows |
| Cyclic discriminated arrays | `cyclicDiscriminatedArrays` | `order: cycle(login,purchase,logout)*2` plus per-label tables |

```toon
rows[2]{id,tags[;]}:
  1,a;b
  2,c
```

```toon
rows[2]{id,items{sku,q}}:
  1,1
    A,2
  2,2
    B,1
    C,3
```

## TOONL (streams)

One record per line, the header once, and an optional closing trailer with the
record count:

```toonl
[]{id,name}:
1,Ada
2,Linus
[=2]
```

## Decoder options for untrusted text

| Option | Guards against |
| --- | --- |
| `strict` (default on) | duplicate keys, length mismatches, blank lines inside arrays, bad indentation |
| `maxDepth` (default 1000) | runaway nesting |
| `maxInputBytes` | oversized documents |
| `maxArrayLength` | huge declared lengths |
| `maxKeys` | objects or field lists with too many keys |

Errors carry `line`, `kind` (`syntax`, `indentation`, `length-mismatch`,
`duplicate-key`, `depth-limit`, `input-limit`) and, for indentation, `column`.
See [LLM prompting](llm-prompting.md) for the generate-validate-retry loop.
