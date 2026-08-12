//! Every byte the `toon` CLI reads or writes goes through this seam, so a test
//! can drive a whole run in-process while the real entry point binds it to the
//! process. The TypeScript twin is `packages/toon/src/cli/io.ts`.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::errors::CliError;

/// The process surface one run needs: where relative paths resolve, the two
/// output streams, and the bytes on stdin.
pub trait CliIo {
    /// Resolves relative input, output, and label paths.
    fn cwd(&self) -> &Path;
    fn stdout(&mut self, text: &str);
    fn stderr(&mut self, text: &str);
    fn stdin(&mut self) -> Box<dyn Read + Send>;
}

/// Where one invocation reads its document from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    Stdin,
    File(PathBuf),
}

/// Batched write size: large enough to amortize syscalls, small enough to
/// stream. A run that fails mid-document drops the batch it never flushed,
/// which is what keeps a failed decode from writing half a JSON document.
const WRITE_BATCH_BYTES: usize = 64 * 1024;

/// How many recent input lines stay available for a decode failure to quote.
/// Bounded, so decoding a large document still streams rather than retains it;
/// an error pointing further back than this prints its header alone.
const RECORDED_LINES: usize = 64;

/// The `toon` CLI bound to the real process.
pub struct ProcessIo {
    cwd: PathBuf,
}

impl ProcessIo {
    /// Binds to the current working directory, falling back to `.` when the
    /// process has none the platform will name.
    pub fn new() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

impl Default for ProcessIo {
    fn default() -> Self {
        Self::new()
    }
}

impl CliIo for ProcessIo {
    fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn stdout(&mut self, text: &str) {
        let _ = io::stdout().write_all(text.as_bytes());
    }

    fn stderr(&mut self, text: &str) {
        let _ = io::stderr().write_all(text.as_bytes());
    }

    fn stdin(&mut self) -> Box<dyn Read + Send> {
        Box::new(io::stdin())
    }
}

/// Reads a whole input as text, replacing ill-formed bytes the way Node's
/// non-fatal `TextDecoder` does.
pub fn read_input(source: &InputSource, io: &mut dyn CliIo) -> Result<String, CliError> {
    let mut bytes = Vec::new();
    match source {
        InputSource::File(path) => {
            let mut file = open_file(path)?;
            file.read_to_end(&mut bytes)
                .map_err(|error| read_failed(path, error))?;
        }
        InputSource::Stdin => {
            io.stdin()
                .read_to_end(&mut bytes)
                .map_err(|error| CliError::with_cause("Failed to read stdin", error))?;
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Opens an input for the streaming decoder, together with the window of lines
/// a failure quotes and the flag that says the bytes were not valid UTF-8.
pub fn open_line_reader(
    source: &InputSource,
    strict: bool,
    io: &mut dyn CliIo,
) -> Result<(BufReader<TextReader>, Arc<Recorder>), CliError> {
    let inner: Box<dyn Read + Send> = match source {
        InputSource::File(path) => Box::new(open_file(path)?),
        InputSource::Stdin => io.stdin(),
    };
    let recorder = Arc::new(Recorder::default());
    let reader = TextReader {
        inner,
        strict,
        recorder: Arc::clone(&recorder),
        pending: Vec::new(),
        ready: VecDeque::new(),
        at_eof: false,
    };
    Ok((BufReader::new(reader), recorder))
}

/// Names an input the way the upstream success lines do.
pub fn format_input_label(source: &InputSource, cwd: &Path) -> String {
    match source {
        InputSource::Stdin => "stdin".to_owned(),
        InputSource::File(path) => relative_label(cwd, path),
    }
}

/// `path.relative(cwd, target)` for the shapes a CLI run produces: a path
/// under the working directory becomes its tail, and anything else keeps the
/// name it was given.
pub fn relative_label(cwd: &Path, target: &Path) -> String {
    match target.strip_prefix(cwd) {
        Ok(relative) if relative.as_os_str().is_empty() => file_name(target),
        Ok(relative) => relative.to_string_lossy().into_owned(),
        Err(_) => target.to_string_lossy().into_owned(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy().into_owned(), |name| {
            name.to_string_lossy().into_owned()
        })
}

fn open_file(path: &Path) -> Result<File, CliError> {
    File::open(path).map_err(|error| read_failed(path, error))
}

fn read_failed(path: &Path, error: io::Error) -> CliError {
    CliError::with_cause(
        format!("Failed to read `{}`: {error}", path.display()),
        error,
    )
}

/// The bounded window of source lines a decode failure quotes, filled by the
/// reader as it hands bytes to the decoder.
#[derive(Default)]
pub struct Recorder {
    state: Mutex<Window>,
    invalid_utf8: AtomicBool,
}

#[derive(Default)]
struct Window {
    /// 1-based number of the oldest line still held.
    first: usize,
    lines: VecDeque<String>,
    partial: String,
    next: usize,
}

impl Recorder {
    /// Returns the recorded source of `line`, if it is still in the window.
    pub fn line(&self, line: usize) -> Option<String> {
        let state = self.state.lock().expect("line window is not poisoned");
        let index = line.checked_sub(state.first)?;
        state.lines.get(index).cloned()
    }

    /// Reports whether strict decoding rejected the input as invalid UTF-8.
    pub fn saw_invalid_utf8(&self) -> bool {
        self.invalid_utf8.load(Ordering::Relaxed)
    }

    fn record(&self, text: &str) {
        let mut state = self.state.lock().expect("line window is not poisoned");
        for character in text.chars() {
            if character == '\n' {
                let line = std::mem::take(&mut state.partial);
                state.push(line);
            } else {
                state.partial.push(character);
            }
        }
    }

    fn finish(&self) {
        let mut state = self.state.lock().expect("line window is not poisoned");
        if !state.partial.is_empty() {
            let line = std::mem::take(&mut state.partial);
            state.push(line);
        }
    }
}

impl Window {
    fn push(&mut self, line: String) {
        if self.next == 0 {
            self.first = 1;
            self.next = 1;
        }
        self.lines.push_back(line);
        self.next += 1;
        while self.lines.len() > RECORDED_LINES {
            self.lines.pop_front();
            self.first += 1;
        }
    }
}

/// Decodes input bytes to text on the way to the decoder, recording each line
/// it hands over. Strict decoding refuses to substitute U+FFFD, the way Node's
/// fatal `TextDecoder` does; `--no-strict` replaces ill-formed bytes instead.
pub struct TextReader {
    inner: Box<dyn Read + Send>,
    strict: bool,
    recorder: Arc<Recorder>,
    pending: Vec<u8>,
    ready: VecDeque<u8>,
    at_eof: bool,
}

impl TextReader {
    fn fill(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 8 * 1024];
        let read = self.inner.read(&mut chunk)?;
        if read == 0 {
            self.at_eof = true;
        } else {
            self.pending.extend_from_slice(&chunk[..read]);
        }
        self.transcode()
    }

    fn transcode(&mut self) -> io::Result<()> {
        let mut pending = std::mem::take(&mut self.pending);
        let result = self.transcode_pending(&mut pending);
        self.pending = pending;

        if self.at_eof {
            self.recorder.finish();
        }
        result
    }

    fn transcode_pending(&mut self, pending: &mut Vec<u8>) -> io::Result<()> {
        loop {
            match std::str::from_utf8(pending) {
                Ok(text) => {
                    self.emit(text);
                    pending.clear();
                    return Ok(());
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // An incomplete trailing sequence is only ill-formed once
                    // no further byte can complete it.
                    let width = error.error_len().or_else(|| self.at_eof.then_some(1));
                    self.emit(std::str::from_utf8(&pending[..valid]).expect("prefix is valid"));

                    let Some(width) = width else {
                        pending.drain(..valid);
                        return Ok(());
                    };
                    if self.strict {
                        self.recorder.invalid_utf8.store(true, Ordering::Relaxed);
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "input is not valid UTF-8",
                        ));
                    }
                    pending.drain(..valid + width);
                    self.emit("\u{FFFD}");
                }
            }
        }
    }

    fn emit(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.recorder.record(text);
        self.ready.extend(text.as_bytes());
    }
}

impl Read for TextReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.ready.is_empty() && !self.at_eof {
            self.fill()?;
        }

        let count = out.len().min(self.ready.len());
        for slot in out.iter_mut().take(count) {
            *slot = self.ready.pop_front().expect("the queue holds `count` bytes");
        }
        Ok(count)
    }
}

/// Writes the pieces of one conversion to a file or to stdout, always ending
/// with a newline. Pieces accumulate in a batch, so a large document costs one
/// write per batch rather than one per piece — and a run that fails partway
/// through drops the batch instead of emitting a half-written document.
pub struct OutputSink {
    file: Option<File>,
    batch: String,
}

impl OutputSink {
    pub fn open(output: Option<&Path>) -> Result<Self, CliError> {
        let file = match output {
            None => None,
            Some(path) => Some(File::create(path).map_err(|error| {
                CliError::with_cause(format!("Failed to write `{}`: {error}", path.display()), error)
            })?),
        };
        Ok(Self {
            file,
            batch: String::new(),
        })
    }

    pub fn write(&mut self, text: &str, io: &mut dyn CliIo) -> Result<(), CliError> {
        self.batch.push_str(text);
        if self.batch.len() >= WRITE_BATCH_BYTES {
            self.flush(io)?;
        }
        Ok(())
    }

    /// Ends the document with the newline every upstream conversion writes.
    pub fn finish(mut self, io: &mut dyn CliIo) -> Result<(), CliError> {
        self.batch.push('\n');
        self.flush(io)
    }

    fn flush(&mut self, io: &mut dyn CliIo) -> Result<(), CliError> {
        if self.batch.is_empty() {
            return Ok(());
        }
        match &mut self.file {
            Some(file) => file.write_all(self.batch.as_bytes()).map_err(|error| {
                CliError::with_cause(format!("Failed to write output: {error}"), error)
            })?,
            None => io.stdout(&self.batch),
        }
        self.batch.clear();
        Ok(())
    }
}
