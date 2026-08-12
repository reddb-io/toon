//! Renders the canonical decode event stream as JSON text, one piece at a
//! time, so a decode never materializes the whole document. Mirrors the
//! upstream `@toon-format/cli` writer, including its `indent: 0` compact form.
//!
//! The TypeScript twin is `packages/toon/src/cli/json-from-events.ts`.

use crate::ToonEvent;

use super::errors::CliError;
use super::io::{CliIo, OutputSink};

enum Context {
    Object { needs_comma: bool, expect_value: bool },
    Array { needs_comma: bool },
}

/// The event-driven JSON writer. One instance renders one document.
pub struct JsonWriter {
    stack: Vec<Context>,
    depth: usize,
    indent: usize,
}

impl JsonWriter {
    pub fn new(indent: usize) -> Self {
        Self {
            stack: Vec::new(),
            depth: 0,
            indent,
        }
    }

    pub fn write_event(
        &mut self,
        event: &ToonEvent,
        sink: &mut OutputSink,
        io: &mut dyn CliIo,
    ) -> Result<(), CliError> {
        match event {
            ToonEvent::StartObject { .. } => {
                self.value_prefix(sink, io)?;
                sink.write("{", io)?;
                self.stack.push(Context::Object {
                    needs_comma: false,
                    expect_value: false,
                });
                self.depth += 1;
            }
            ToonEvent::StartArray { .. } => {
                self.value_prefix(sink, io)?;
                sink.write("[", io)?;
                self.stack.push(Context::Array { needs_comma: false });
                self.depth += 1;
            }
            ToonEvent::EndObject { .. } => self.close("}", sink, io)?,
            ToonEvent::EndArray { .. } => self.close("]", sink, io)?,
            ToonEvent::Key { key, .. } => {
                let Some(Context::Object {
                    needs_comma,
                    expect_value,
                }) = self.stack.last_mut()
                else {
                    return Err(mismatched("key event outside of object context"));
                };
                let separated = *needs_comma;
                *expect_value = true;
                *needs_comma = true;

                if separated {
                    sink.write(",", io)?;
                }
                self.newline_indent(self.depth, sink, io)?;
                sink.write(&quote(key), io)?;
                sink.write(if self.indent > 0 { ": " } else { ":" }, io)?;
            }
            ToonEvent::Primitive { value, .. } => {
                if let Some(Context::Object { expect_value, .. }) = self.stack.last() {
                    if !*expect_value {
                        return Err(mismatched("primitive event without a preceding key"));
                    }
                }
                self.value_prefix(sink, io)?;
                let rendered = serde_json::to_string(&value.to_json_value())
                    .map_err(|error| CliError::with_cause("Failed to write JSON", error))?;
                sink.write(&rendered, io)?;
                self.value_complete();
            }
        }
        Ok(())
    }

    /// Reports whether the stream closed every container it opened.
    pub fn finish(&self) -> Result<(), CliError> {
        if self.stack.is_empty() {
            return Ok(());
        }
        Err(mismatched("incomplete event stream: unclosed objects or arrays"))
    }

    fn close(
        &mut self,
        bracket: &str,
        sink: &mut OutputSink,
        io: &mut dyn CliIo,
    ) -> Result<(), CliError> {
        let Some(context) = self.stack.pop() else {
            return Err(mismatched("mismatched container end event"));
        };
        let (populated, matches) = match (&context, bracket) {
            (Context::Object { needs_comma, .. }, "}") => (*needs_comma, true),
            (Context::Array { needs_comma }, "]") => (*needs_comma, true),
            _ => (false, false),
        };
        if !matches {
            return Err(mismatched("mismatched container end event"));
        }

        self.depth -= 1;
        if populated {
            self.newline_indent(self.depth, sink, io)?;
        }
        sink.write(bracket, io)?;
        self.value_complete();
        Ok(())
    }

    /// Writes the comma and indentation an array element needs before itself.
    /// Object members are prefixed by their key event instead.
    fn value_prefix(&mut self, sink: &mut OutputSink, io: &mut dyn CliIo) -> Result<(), CliError> {
        let Some(Context::Array { needs_comma }) = self.stack.last() else {
            return Ok(());
        };
        let separated = *needs_comma;

        if separated {
            sink.write(",", io)?;
        }
        self.newline_indent(self.depth, sink, io)
    }

    fn newline_indent(
        &self,
        depth: usize,
        sink: &mut OutputSink,
        io: &mut dyn CliIo,
    ) -> Result<(), CliError> {
        if self.indent == 0 {
            return Ok(());
        }
        sink.write("\n", io)?;
        sink.write(&" ".repeat(depth * self.indent), io)
    }

    fn value_complete(&mut self) {
        match self.stack.last_mut() {
            Some(Context::Object {
                needs_comma,
                expect_value,
            }) => {
                *expect_value = false;
                *needs_comma = true;
            }
            Some(Context::Array { needs_comma }) => *needs_comma = true,
            None => {}
        }
    }
}

/// `JSON.stringify(text)` for an object key.
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

/// An event sequence the writer cannot render is a defect in the decoder, not
/// input the user can fix — but it still has to leave the process cleanly.
fn mismatched(reason: &str) -> CliError {
    CliError::new(format!("Failed to render JSON: {reason}"))
}
