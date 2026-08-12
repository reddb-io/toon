//! The two conversions the `toon` CLI performs, over the canonical
//! event-stream codec: JSON to TOON and TOON back to JSON. Results go to
//! stdout or to `--output`; every diagnostic goes to stderr, so a pipeline
//! stays clean. The TypeScript twin is
//! `packages/toon/src/cli/conversion.ts`.

use std::path::PathBuf;

use crate::{decode_event_reader, encode_with_options, DecodeStreamOptions, EncodeOptions, ParseError, Value};

use super::errors::CliError;
use super::io::{
    format_input_label, open_line_reader, read_input, relative_label, CliIo, InputSource,
    OutputSink, Recorder,
};
use super::json_from_events::JsonWriter;
use super::token_stats::format_statistics;

/// What both conversions need: where the document comes from, where it goes,
/// and the indentation both codecs are configured with.
pub struct Conversion {
    pub input: InputSource,
    pub output: Option<PathBuf>,
    pub indent_size: usize,
}

pub fn encode_to_toon(
    config: &Conversion,
    delimiter: char,
    stats: bool,
    io: &mut dyn CliIo,
) -> Result<(), CliError> {
    let json = read_input(&config.input, io)?;
    let value = Value::from_json_str(&json)
        .map_err(|error| CliError::with_cause(format!("Failed to parse JSON: {error}"), error))?;

    let toon = encode_with_options(
        &value,
        EncodeOptions {
            delimiter,
            indent_size: config.indent_size,
            ..EncodeOptions::default()
        },
    )
    .map_err(|error| CliError::with_cause(format!("Failed to encode TOON: {error}"), error))?;

    let mut sink = OutputSink::open(config.output.as_deref())?;
    sink.write(&toon, io)?;
    sink.finish(io)?;

    report_written("Encoded", config, io);

    if stats {
        io.stderr(&format_statistics(&json, &toon));
    }
    Ok(())
}

pub fn decode_to_json(
    config: &Conversion,
    strict: bool,
    io: &mut dyn CliIo,
) -> Result<(), CliError> {
    let (reader, recorder) = open_line_reader(&config.input, strict, io)?;
    let events = decode_event_reader(
        reader,
        &DecodeStreamOptions {
            indent: config.indent_size,
            strict,
            ..DecodeStreamOptions::default()
        },
    );

    let mut writer = JsonWriter::new(config.indent_size);
    let mut sink = OutputSink::open(config.output.as_deref())?;

    for event in events {
        let event = event.map_err(|error| describe_decode_failure(&error, &recorder))?;
        writer.write_event(&event, &mut sink, io)?;
    }
    writer.finish()?;
    sink.finish(io)?;

    report_written("Decoded", config, io);
    Ok(())
}

/// Turns a positioned decoder failure into the message the user sees. Bytes
/// the strict reader refused never reached the decoder as text, so that
/// failure is reported as what it is rather than as a syntax error.
fn describe_decode_failure(error: &ParseError, recorder: &Recorder) -> CliError {
    if recorder.saw_invalid_utf8() {
        return CliError::new(
            "Input is not valid UTF-8. Pass --no-strict to replace ill-formed bytes",
        );
    }
    let line = error.line();
    CliError::decode(line, error.message(), recorder.line(line).as_deref())
}

fn report_written(verb: &str, config: &Conversion, io: &mut dyn CliIo) {
    let Some(output) = &config.output else {
        return;
    };
    let input_label = format_input_label(&config.input, io.cwd());
    let output_label = relative_label(io.cwd(), output);
    io.stderr(&format!("✔ {verb} `{input_label}` → `{output_label}`\n"));
}
