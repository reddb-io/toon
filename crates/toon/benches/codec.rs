//! Runtime benchmarks for the TOON codec.
//!
//! Measures the four operations a caller actually pays for, over the shared
//! benchmark corpus plus the `html-heavy` payload that the performance axis
//! owns:
//!
//! * `encode`  — value -> TOON wire
//! * `decode`  — TOON wire -> value
//! * `json_to_toon` — JSON text -> TOON wire (what `tq` does)
//! * `toon_to_json` — TOON wire -> JSON text
//!
//! Each group is throughput-annotated with the input's byte size, so criterion
//! reports MiB/s and a size regression shows up as a throughput change rather
//! than a time change on a moved goalpost.
//!
//! See `benchmarks/README.md` for how to read the output and how to record or
//! compare a committed baseline.

use std::fs;
use std::path::{Path, PathBuf};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use reddb_io_toon::Value;

struct Case {
    name: String,
    json_text: String,
    toon_text: String,
    value: Value,
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/toon; the corpora live at the repo root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

/// Every `*.json` under a directory, recursively, in a stable order.
fn json_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            json_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            found.push(path);
        }
    }
}

fn load_cases() -> Vec<Case> {
    let root = repo_root();
    let mut paths = Vec::new();
    // The shared token corpus: representative payload shapes.
    json_files(&root.join("benchmarks/datasets"), &mut paths);
    // The performance axis' own pathological payload (see #194).
    json_files(&root.join("benchmarks/performance/datasets"), &mut paths);

    let cases: Vec<Case> = paths
        .iter()
        .map(|path| {
            let json_text = fs::read_to_string(path).expect("read dataset");
            let value = Value::from_json_str(&json_text).expect("dataset is valid JSON");
            let toon_text = value
                .try_to_canonical_toon()
                .expect("dataset encodes to TOON");
            let name = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .with_extension("")
                .to_string_lossy()
                // Criterion uses the id in directory names; keep it filesystem-safe.
                .replace(['/', '\\'], "::");
            Case {
                name,
                json_text,
                toon_text,
                value,
            }
        })
        .collect();

    assert!(
        !cases.is_empty(),
        "no benchmark datasets found under {}",
        root.display()
    );
    cases
}

fn bench_codec(criterion: &mut Criterion) {
    let cases = load_cases();

    let mut encode = criterion.benchmark_group("encode");
    for case in &cases {
        encode.throughput(Throughput::Bytes(case.json_text.len() as u64));
        encode.bench_with_input(
            BenchmarkId::from_parameter(&case.name),
            case,
            |bencher, case| {
                bencher.iter(|| {
                    black_box(&case.value)
                        .try_to_canonical_toon()
                        .expect("encode")
                });
            },
        );
    }
    encode.finish();

    let mut decode = criterion.benchmark_group("decode");
    for case in &cases {
        decode.throughput(Throughput::Bytes(case.toon_text.len() as u64));
        decode.bench_with_input(
            BenchmarkId::from_parameter(&case.name),
            case,
            |bencher, case| {
                bencher.iter(|| Value::parse_toon(black_box(&case.toon_text)).expect("decode"));
            },
        );
    }
    decode.finish();

    let mut json_to_toon = criterion.benchmark_group("json_to_toon");
    for case in &cases {
        json_to_toon.throughput(Throughput::Bytes(case.json_text.len() as u64));
        json_to_toon.bench_with_input(
            BenchmarkId::from_parameter(&case.name),
            case,
            |bencher, case| {
                bencher.iter(|| {
                    Value::from_json_str(black_box(&case.json_text))
                        .expect("parse json")
                        .try_to_canonical_toon()
                        .expect("encode")
                });
            },
        );
    }
    json_to_toon.finish();

    let mut toon_to_json = criterion.benchmark_group("toon_to_json");
    for case in &cases {
        toon_to_json.throughput(Throughput::Bytes(case.toon_text.len() as u64));
        toon_to_json.bench_with_input(
            BenchmarkId::from_parameter(&case.name),
            case,
            |bencher, case| {
                bencher.iter(|| {
                    Value::parse_toon(black_box(&case.toon_text))
                        .expect("decode")
                        .to_json_value()
                        .to_string()
                });
            },
        );
    }
    toon_to_json.finish();
}

criterion_group!(benches, bench_codec);
criterion_main!(benches);
