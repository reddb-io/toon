// The iterator boundary of the event decoder (ADR 0006): the sink the
// recursive grammar emits through, and the `EventDecoder` handle that pulls
// batches of events across a zero-capacity channel. Splitting it from the grammar in
// `stream.rs` keeps both parts inside the shared file-length budget.

/// Stack for the recursive grammar. The default `max_depth` of 1000 needs a few
/// MiB in a debug build; 16 MiB leaves room for larger limits and error types,
/// and is reserved address space that only deep documents actually touch.
const EVENT_DECODER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// Events per channel message. One thread handoff per event cost about 3.5 µs
/// per key or value; a batch amortizes it while memory stays bounded.
const EVENT_BATCH: usize = 256;

type EventBatch = Vec<Result<ToonEvent, ParseError>>;

trait EventSink {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError>;
}

impl EventSink for Vec<ToonEvent> {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        self.push(event);
        Ok(())
    }
}

struct ChannelSink {
    sender: SyncSender<EventBatch>,
    batch: EventBatch,
}

/// The worker's sink, shared with [`FlushingReader`] so a pending batch goes
/// out before the parser waits on input.
struct SharedSink(std::rc::Rc<std::cell::RefCell<ChannelSink>>);

impl EventSink for SharedSink {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        self.0.borrow_mut().emit(event)
    }
}

/// Wraps the input so the parser only reads from the source on demand: under
/// `BufRead` semantics, once every byte the last `fill_buf` returned is
/// consumed, the next `fill_buf` goes to the underlying source, so the pending
/// batch is delivered and the worker waits for the consumer first. Input that
/// is already buffered decodes in full batches without a handoff per event.
struct FlushingReader<R> {
    inner: R,
    buffered: usize,
    sink: std::rc::Rc<std::cell::RefCell<ChannelSink>>,
}

impl<R: BufRead> std::io::Read for FlushingReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(out.len());
        out[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl<R: BufRead> BufRead for FlushingReader<R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.buffered == 0 {
            self.sink
                .borrow_mut()
                .wait_for_demand()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "event consumer disconnected"))?;
        }
        let available = self.inner.fill_buf()?;
        self.buffered = available.len();
        Ok(available)
    }

    fn consume(&mut self, amount: usize) {
        self.buffered = self.buffered.saturating_sub(amount);
        self.inner.consume(amount);
    }
}

impl ChannelSink {
    /// Blocks until the consumer has drained everything delivered so far and
    /// asks for more: a rendezvous send of an empty batch only completes on the
    /// consumer's next `recv`. Called before a read that may go to the source,
    /// so the parser never reads ahead of demand.
    fn wait_for_demand(&mut self) -> Result<(), ParseError> {
        self.flush(0)?;
        self.sender
            .send(Vec::new())
            .map_err(|_| stream_error(0, "event consumer disconnected"))
    }

    fn flush(&mut self, line: usize) -> Result<(), ParseError> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let batch = std::mem::replace(&mut self.batch, Vec::with_capacity(EVENT_BATCH));
        self.sender
            .send(batch)
            .map_err(|_| stream_error(line, "event consumer disconnected"))
    }
}

impl EventSink for ChannelSink {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        let line = event.line();
        self.batch.push(Ok(event));
        if self.batch.len() >= EVENT_BATCH {
            self.flush(line)?;
        }
        Ok(())
    }
}

impl ToonEvent {
    fn line(&self) -> usize {
        match self {
            Self::StartObject { line }
            | Self::EndObject { line }
            | Self::StartArray { line, .. }
            | Self::EndArray { line }
            | Self::Key { line, .. }
            | Self::Primitive { line, .. } => *line,
        }
    }
}

/// Iterator over positioned decode events. A zero-capacity channel keeps the
/// parser coupled to iteration: it runs at most one batch of events ahead, so
/// memory stays bounded whatever the input size. A consumer of a slow source
/// sees events a batch at a time rather than one by one.
pub struct EventDecoder {
    receiver: Receiver<EventBatch>,
    pending: std::vec::IntoIter<Result<ToonEvent, ParseError>>,
    worker: Option<JoinHandle<()>>,
}

impl Iterator for EventDecoder {
    type Item = Result<ToonEvent, ParseError>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.pending.next() {
                return Some(event);
            }
            match self.receiver.recv() {
                Ok(batch) => self.pending = batch.into_iter(),
                Err(_) => {
                    if let Some(worker) = self.worker.take() {
                        let _ = worker.join();
                    }
                    return None;
                }
            }
        }
    }
}

impl Drop for EventDecoder {
    fn drop(&mut self) {
        // Replacing the receiver disconnects a parser blocked on event delivery.
        let (_sender, replacement) = sync_channel(0);
        let old = std::mem::replace(&mut self.receiver, replacement);
        drop(old);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Runs `work` on a scoped worker with the decoder's stack, so the recursive
/// grammar has room for deep documents whichever thread the caller is on (a
/// spawned thread defaults to 2 MiB, where 1000 levels overflow).
fn on_decoder_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(EVENT_DECODER_STACK_SIZE)
            .spawn_scoped(scope, work)
            .expect("failed to spawn TOON decoder")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// Decode events directly from a buffered reader with one classified line of
/// lookahead. The reader is moved to a worker so each iterator step can suspend
/// the recursive grammar exactly at an event boundary.
pub fn decode_event_reader<R>(reader: R, options: &DecodeStreamOptions) -> EventDecoder
where
    R: BufRead + Send + 'static,
{
    let (sender, receiver) = sync_channel(0);
    let ctx = StreamCtx::new(options);
    let worker = std::thread::Builder::new()
        .stack_size(EVENT_DECODER_STACK_SIZE)
        .spawn(move || {
            let shared = std::rc::Rc::new(std::cell::RefCell::new(ChannelSink {
                sender,
                batch: Vec::with_capacity(EVENT_BATCH),
            }));
            let reader = FlushingReader {
                inner: reader,
                buffered: 0,
                sink: std::rc::Rc::clone(&shared),
            };
            let mut sink = SharedSink(std::rc::Rc::clone(&shared));
            if let Err(error) = decode_events_into(reader, &ctx, &mut sink) {
                shared.borrow_mut().batch.push(Err(error));
            }
            // The consumer may already be gone; nothing is left to report to.
            let _ = shared.borrow_mut().flush(0);
        })
        .expect("failed to spawn TOON event decoder");
    EventDecoder {
        receiver,
        pending: Vec::new().into_iter(),
        worker: Some(worker),
    }
}

pub fn decode_event_stream(input: &str, options: &DecodeStreamOptions) -> EventDecoder {
    decode_event_reader(Cursor::new(input.as_bytes().to_vec()), options)
}

/// Decodes the document into the full event sequence, stopping at the first
/// error. The events emitted before the error are returned alongside it, so
/// iterator consumers observe the same prefix the TS generator yields.
pub fn decode_events(
    input: &str,
    options: &DecodeStreamOptions,
) -> (Vec<ToonEvent>, Option<ParseError>) {
    on_decoder_stack(|| collect_events(input, options))
}

fn collect_events(input: &str, options: &DecodeStreamOptions) -> (Vec<ToonEvent>, Option<ParseError>) {
    let ctx = StreamCtx::new(options);
    let mut events = Vec::new();
    let error = decode_events_into(Cursor::new(input.as_bytes()), &ctx, &mut events).err();
    (events, error)
}

fn decode_events_for_truncation(
    input: &str,
    options: &DecodeStreamOptions,
) -> (Option<ParseError>, Option<ArraySpanState>) {
    on_decoder_stack(|| scan_for_truncation(input, options))
}

fn scan_for_truncation(
    input: &str,
    options: &DecodeStreamOptions,
) -> (Option<ParseError>, Option<ArraySpanState>) {
    let ctx = StreamCtx {
        indent_size: options.indent.max(1),
        ..StreamCtx::new(options)
    };
    let mut events = Vec::new();
    let error = decode_events_into(Cursor::new(input.as_bytes()), &ctx, &mut events).err();
    (error, ctx.truncation_span.get())
}
