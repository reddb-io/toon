#!/usr/bin/env python3
"""Generate the TOONL/TOON token benchmark research note.

Run with:
  uv run --with tiktoken python scripts/research_token_benchmark.py --write
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable

import tiktoken


ENCODING_NAME = "o200k_base"
ROW_COUNTS = (10, 100, 10_000)
OUTPUT = Path(".red/researches/token-benchmark-toonl-vs-jsonl.md")


@dataclass(frozen=True)
class Dataset:
    key: str
    label: str
    fields: tuple[str, ...]
    build_row: Callable[[int], dict[str, Any]]
    notes: str


SAFE_BARE = re.compile(r"^[A-Za-z_][A-Za-z0-9_./:@-]*$")
NUMBER_LIKE = re.compile(r"^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$")


def log_flat(i: int) -> dict[str, Any]:
    return {
        "ts": f"2026-07-14T17:{i % 60:02d}:{(i * 7) % 60:02d}Z",
        "level": ("info", "warn", "error", "debug")[i % 4],
        "service": ("api", "worker", "billing", "search")[i % 4],
        "region": ("us-east-1", "eu-west-1", "sa-east-1")[i % 3],
        "request_id": f"req-{i:06d}",
        "latency_ms": 18 + (i * 37) % 900,
        "status": (200, 200, 200, 404, 503)[i % 5],
        "msg": ("ok", "cache_miss", "retrying", "upstream_timeout")[i % 4],
    }


def analytics_export(i: int) -> dict[str, Any]:
    return {
        "account_id": f"acct_{10_000 + i % 997}",
        "day": f"2026-07-{1 + i % 28:02d}",
        "plan": ("free", "team", "business", "enterprise")[i % 4],
        "region": ("na", "eu", "latam", "apac")[i % 4],
        "sessions": 80 + (i * 19) % 7_000,
        "active_users": 12 + (i * 13) % 1_400,
        "revenue_usd": round(19.0 + ((i * 271) % 95_000) / 100, 2),
        "conversion_rate": round(0.018 + ((i * 17) % 260) / 10_000, 4),
    }


def envelope_escape_hatch(i: int) -> dict[str, Any]:
    return {
        "id": f"evt_{i:07d}",
        "kind": ("checkout.created", "checkout.failed", "user.updated")[i % 3],
        "source": ("web", "ios", "android", "worker")[i % 4],
        "received_at": f"2026-07-14T18:{i % 60:02d}:{(i * 11) % 60:02d}Z",
        "payload": {
            "cart_id": f"cart_{i:06d}",
            "amount": round(12.5 + ((i * 313) % 40_000) / 100, 2),
            "items": [
                {"sku": f"sku-{(i + offset) % 250:04d}", "qty": 1 + (i + offset) % 4}
                for offset in range(3)
            ],
            "flags": {"coupon": i % 5 == 0, "gift": i % 17 == 0},
        },
    }


DATASETS = (
    Dataset(
        "log-flat",
        "Log flat",
        ("ts", "level", "service", "region", "request_id", "latency_ms", "status", "msg"),
        log_flat,
        "Flat operational log records.",
    ),
    Dataset(
        "analytics-export",
        "Export analytics",
        (
            "account_id",
            "day",
            "plan",
            "region",
            "sessions",
            "active_users",
            "revenue_usd",
            "conversion_rate",
        ),
        analytics_export,
        "Flat metrics export with repeated dimensional keys.",
    ),
    Dataset(
        "envelope-escape-hatch",
        "Envelope + escape-hatch cell",
        ("id", "kind", "source", "received_at", "payload_json"),
        envelope_escape_hatch,
        "Envelope fields remain tabular; the nested payload is one compact JSON string cell.",
    ),
)


def rows(dataset: Dataset, row_count: int) -> list[dict[str, Any]]:
    return [dataset.build_row(i) for i in range(row_count)]


def jsonl(rows_: Iterable[dict[str, Any]]) -> str:
    return "".join(json.dumps(row, separators=(",", ":"), ensure_ascii=False) + "\n" for row in rows_)


def toon_rows(dataset: Dataset, rows_: Iterable[dict[str, Any]]) -> list[list[Any]]:
    out: list[list[Any]] = []
    for row in rows_:
        converted = dict(row)
        if dataset.key == "envelope-escape-hatch":
            converted["payload_json"] = json.dumps(
                converted.pop("payload"), separators=(",", ":"), ensure_ascii=False
            )
        out.append([converted[field] for field in dataset.fields])
    return out


def quote_cell(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, (int, float)):
        return str(value)
    text = str(value)
    if (
        text
        and SAFE_BARE.match(text)
        and text not in {"true", "false", "null"}
        and not NUMBER_LIKE.match(text)
    ):
        return text
    return json.dumps(text, separators=(",", ":"), ensure_ascii=False)


def toonl(dataset: Dataset, rows_: list[dict[str, Any]]) -> str:
    body = toon_rows(dataset, rows_)
    lines = [f"[]{{{','.join(dataset.fields)}}}:"]
    lines.extend(",".join(quote_cell(cell) for cell in row) for row in body)
    lines.append(f"[={len(body)}]")
    return "\n".join(lines) + "\n"


def closed_toon(dataset: Dataset, rows_: list[dict[str, Any]]) -> str:
    body = toon_rows(dataset, rows_)
    lines = [f"[{len(body)}]{{{','.join(dataset.fields)}}}:"]
    lines.extend("  " + ",".join(quote_cell(cell) for cell in row) for row in body)
    return "\n".join(lines) + "\n"


def pct_saving(baseline: int, value: int) -> float:
    return (baseline - value) * 100 / baseline


def measurements() -> list[dict[str, Any]]:
    encoding = tiktoken.get_encoding(ENCODING_NAME)
    result: list[dict[str, Any]] = []
    for dataset in DATASETS:
        for row_count in ROW_COUNTS:
            records = rows(dataset, row_count)
            payloads = {
                "JSONL": jsonl(records),
                "TOONL verified": toonl(dataset, records),
                "TOON closed": closed_toon(dataset, records),
            }
            row = {
                "dataset": dataset,
                "rows": row_count,
                "formats": {
                    name: {
                        "tokens": len(encoding.encode(payload)),
                        "bytes": len(payload.encode("utf-8")),
                    }
                    for name, payload in payloads.items()
                },
            }
            result.append(row)
    return result


def fmt_pct(value: float) -> str:
    return f"{value:.1f}%"


def render_report() -> str:
    measured = measurements()
    lines = [
        "# Token benchmark: TOONL vs JSONL vs closed TOON",
        "",
        f"Tokenizer: `{ENCODING_NAME}` via `tiktoken`.",
        "",
        "This benchmark serializes the same deterministic streams in three forms:",
        "",
        "- JSONL: compact JSON object per line.",
        "- TOONL verified: one TOONL segment with an open `[]` header and final `[=N]` trailer.",
        "- TOON closed: the TOONL close-transform result with a materialized `[N]` header and indented rows.",
        "",
        "The TOONL/TOON envelope case keeps the top-level envelope tabular and stores the nested payload as one compact JSON string cell, matching the TOONL v0.1 escape-hatch rule.",
        "",
        "## README-ready table",
        "",
        "| Payload | Rows | JSONL tokens | TOONL tokens | TOONL saving | Closed TOON tokens | Closed TOON saving |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for item in measured:
        dataset: Dataset = item["dataset"]
        formats = item["formats"]
        json_tokens = formats["JSONL"]["tokens"]
        toonl_tokens = formats["TOONL verified"]["tokens"]
        toon_tokens = formats["TOON closed"]["tokens"]
        lines.append(
            f"| {dataset.label} | {item['rows']} | {json_tokens:,} | {toonl_tokens:,} | "
            f"{fmt_pct(pct_saving(json_tokens, toonl_tokens))} | {toon_tokens:,} | "
            f"{fmt_pct(pct_saving(json_tokens, toon_tokens))} |"
        )

    lines.extend(
        [
            "",
            "## Bytes",
            "",
            "| Payload | Rows | JSONL bytes | TOONL bytes | Closed TOON bytes |",
            "| --- | ---: | ---: | ---: | ---: |",
        ]
    )
    for item in measured:
        dataset = item["dataset"]
        formats = item["formats"]
        lines.append(
            f"| {dataset.label} | {item['rows']} | {formats['JSONL']['bytes']:,} | "
            f"{formats['TOONL verified']['bytes']:,} | {formats['TOON closed']['bytes']:,} |"
        )

    lines.extend(
        [
            "",
            "## Dataset notes",
            "",
        ]
    )
    for dataset in DATASETS:
        lines.append(f"- {dataset.label}: {dataset.notes}")

    lines.extend(
        [
            "",
            "## Reproduce",
            "",
            "```bash",
            "uv run --with tiktoken python scripts/research_token_benchmark.py --write",
            "```",
            "",
        ]
    )
    return "\n".join(lines)


def check_report() -> None:
    measured = measurements()
    for item in measured:
        rows_ = item["rows"]
        dataset = item["dataset"]
        formats = item["formats"]
        json_tokens = formats["JSONL"]["tokens"]
        toonl_tokens = formats["TOONL verified"]["tokens"]
        toon_tokens = formats["TOON closed"]["tokens"]
        assert toonl_tokens < json_tokens, (dataset.key, rows_, toonl_tokens, json_tokens)
        assert toon_tokens < json_tokens, (dataset.key, rows_, toon_tokens, json_tokens)
        if rows_ >= 100 and dataset.key != "envelope-escape-hatch":
            assert pct_saving(json_tokens, toonl_tokens) >= 30, (dataset.key, rows_)
            assert pct_saving(json_tokens, toon_tokens) >= 30, (dataset.key, rows_)
        if rows_ >= 100 and dataset.key == "envelope-escape-hatch":
            assert pct_saving(json_tokens, toonl_tokens) >= 5, (dataset.key, rows_)
            assert pct_saving(json_tokens, toon_tokens) >= 5, (dataset.key, rows_)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help=f"write {OUTPUT}")
    parser.add_argument("--check", action="store_true", help="validate benchmark invariants")
    args = parser.parse_args()

    if args.check:
        check_report()

    report = render_report()
    if args.write:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(report, encoding="utf-8")
    else:
        print(report)


if __name__ == "__main__":
    main()
