//! Cross-language event-sequence parity: the same fixtures the TS runner
//! executes (`packages/toon/test/events.test.mjs`) must produce the same
//! positioned events — event by event, line by line (ADR 0006).

use reddb_io_toon::{decode_event_stream, DecodeStreamOptions, ToonEvent};
use serde_json::Value as Json;
use std::fs;

const FIXTURES: &str = "../../tests/corpus/events";

fn event_to_json(event: &ToonEvent) -> Json {
    match event {
        ToonEvent::StartObject { line } => serde_json::json!({"type":"startObject","line":line}),
        ToonEvent::EndObject { line } => serde_json::json!({"type":"endObject","line":line}),
        ToonEvent::StartArray { length, line } => {
            serde_json::json!({"type":"startArray","length":length,"line":line})
        }
        ToonEvent::EndArray { line } => serde_json::json!({"type":"endArray","line":line}),
        ToonEvent::Key { key, line } => serde_json::json!({"type":"key","key":key,"line":line}),
        ToonEvent::Primitive { value, line } => {
            serde_json::json!({"type":"primitive","value":value.to_json_value(),"line":line})
        }
    }
}

#[test]
fn event_sequences_match_the_shared_fixtures() {
    let mut checked = 0;
    let mut entries: Vec<_> = fs::read_dir(FIXTURES)
        .expect("event fixtures directory")
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        let cases: Json = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let input = case["input"].as_str().unwrap();
            let options = DecodeStreamOptions {
                strict: case.get("strict").and_then(|v| v.as_bool()).unwrap_or(true),
                ..Default::default()
            };
            let mut emitted = Vec::new();
            let mut failure = None;
            for item in decode_event_stream(input, &options) {
                match item {
                    Ok(event) => emitted.push(event_to_json(&event)),
                    Err(error) => failure = Some(error),
                }
            }
            let expected = case["events"].as_array().unwrap().clone();
            assert_eq!(emitted, expected, "{name}: event sequence diverged");
            match case.get("error") {
                Some(error) => {
                    let failure = failure.unwrap_or_else(|| panic!("{name}: expected an error"));
                    assert_eq!(
                        failure.line(),
                        error["line"].as_u64().unwrap() as usize,
                        "{name}: error line diverged"
                    );
                }
                None => {
                    if let Some(failure) = failure {
                        panic!("{name}: unexpected error: {failure:?}");
                    }
                }
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 70,
        "fixture corpus unexpectedly small: {checked}"
    );
}
