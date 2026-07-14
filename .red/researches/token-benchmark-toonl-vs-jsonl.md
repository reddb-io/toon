# Token benchmark: TOONL vs JSONL vs closed TOON

Tokenizer: `o200k_base` via `tiktoken`.

This benchmark serializes the same deterministic streams in three forms:

- JSONL: compact JSON object per line.
- TOONL verified: one TOONL segment with an open `[]` header and final `[=N]` trailer.
- TOON closed: the TOONL close-transform result with a materialized `[N]` header and indented rows.

The TOONL/TOON envelope case keeps the top-level envelope tabular and stores the nested payload as one compact JSON string cell, matching the TOONL v0.1 escape-hatch rule.

## README-ready table

| Payload | Rows | JSONL tokens | TOONL tokens | TOONL saving | Closed TOON tokens | Closed TOON saving |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Log flat | 10 | 552 | 383 | 30.6% | 391 | 29.2% |
| Log flat | 100 | 5,525 | 3,623 | 34.4% | 3,721 | 32.7% |
| Log flat | 10000 | 552,500 | 360,024 | 34.8% | 370,022 | 33.0% |
| Export analytics | 10 | 523 | 320 | 38.8% | 328 | 37.3% |
| Export analytics | 100 | 5,315 | 3,042 | 42.8% | 3,140 | 40.9% |
| Export analytics | 10000 | 535,576 | 305,604 | 42.9% | 315,602 | 41.1% |
| Envelope + escape-hatch cell | 10 | 990 | 925 | 6.6% | 933 | 5.8% |
| Envelope + escape-hatch cell | 100 | 9,900 | 9,085 | 8.2% | 9,183 | 7.2% |
| Envelope + escape-hatch cell | 10000 | 990,000 | 906,686 | 8.4% | 916,684 | 7.4% |

## Bytes

| Payload | Rows | JSONL bytes | TOONL bytes | Closed TOON bytes |
| --- | ---: | ---: | ---: | ---: |
| Log flat | 10 | 1,578 | 796 | 812 |
| Log flat | 100 | 15,888 | 7,457 | 7,653 |
| Log flat | 10000 | 1,589,086 | 739,157 | 759,153 |
| Export analytics | 10 | 1,559 | 637 | 653 |
| Export analytics | 100 | 15,891 | 5,880 | 6,076 |
| Export analytics | 10000 | 1,595,435 | 585,526 | 605,522 |
| Envelope + escape-hatch cell | 10 | 2,785 | 2,646 | 2,662 |
| Envelope + escape-hatch cell | 100 | 27,946 | 26,098 | 26,294 |
| Envelope + escape-hatch cell | 10000 | 2,795,036 | 2,605,090 | 2,625,086 |

## Dataset notes

- Log flat: Flat operational log records.
- Export analytics: Flat metrics export with repeated dimensional keys.
- Envelope + escape-hatch cell: Envelope fields remain tabular; the nested payload is one compact JSON string cell.

## Reproduce

```bash
uv run --with tiktoken python scripts/research_token_benchmark.py --write
```
