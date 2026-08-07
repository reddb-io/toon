//! Cross-language event-sequence parity: the same fixtures the TS runner
//! executes (`packages/toon/test/events.test.mjs`) must produce the same
//! positioned events — event by event, line by line (ADR 0006).

use reddb_io_toon::{decode_event_reader, decode_event_stream, DecodeStreamOptions, ToonEvent};
use serde_json::Value as Json;
use std::fs;
use std::io::{self, BufRead, Read};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

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

struct GatedReader {
    bytes: Vec<u8>,
    position: usize,
    first_chunk: usize,
    open: Arc<AtomicBool>,
    consumed: Arc<AtomicUsize>,
}

impl Read for GatedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(output.len());
        output[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl BufRead for GatedReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let limit = if self.open.load(Ordering::SeqCst) {
            self.bytes.len()
        } else {
            self.first_chunk
        };
        if self.position == limit && limit < self.bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "input is not available yet",
            ));
        }
        Ok(&self.bytes[self.position..limit])
    }

    fn consume(&mut self, amount: usize) {
        self.position += amount;
        self.consumed.store(self.position, Ordering::SeqCst);
    }
}

#[test]
fn bufread_decoder_yields_before_the_reader_reaches_eof() {
    let input = b"a: 1\nb: 2\nc: 3\n";
    let bounded_prefix = b"a: 1\nb: 2\n".len();
    let open = Arc::new(AtomicBool::new(false));
    let consumed = Arc::new(AtomicUsize::new(0));
    let reader = GatedReader {
        bytes: input.to_vec(),
        position: 0,
        first_chunk: bounded_prefix,
        open: Arc::clone(&open),
        consumed: Arc::clone(&consumed),
    };

    let mut events = decode_event_reader(reader, &DecodeStreamOptions::default());
    assert_eq!(
        events.next().unwrap().unwrap(),
        ToonEvent::StartObject { line: 1 }
    );
    assert_eq!(
        consumed.load(Ordering::SeqCst),
        bounded_prefix,
        "the decoder must retain at most two lines of lookahead"
    );
    assert_eq!(
        events.next().unwrap().unwrap(),
        ToonEvent::Key {
            key: "a".to_owned(),
            line: 1
        }
    );

    open.store(true, Ordering::SeqCst);
    let remainder: Vec<_> = events.collect::<Result<_, _>>().unwrap();
    assert_eq!(remainder.len(), 6);
    assert_eq!(consumed.load(Ordering::SeqCst), input.len());
}
