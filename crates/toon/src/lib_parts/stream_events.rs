// The iterator boundary of the event decoder (ADR 0006): the sink the
// recursive grammar emits through, and the `EventDecoder` handle that pulls
// events across a zero-capacity channel. Splitting it from the grammar in
// `stream.rs` keeps both parts inside the shared file-length budget.

const EVENT_DECODER_STACK_SIZE: usize = 8 * 1024 * 1024;

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
    sender: SyncSender<Result<ToonEvent, ParseError>>,
}

impl EventSink for ChannelSink {
    fn emit(&mut self, event: ToonEvent) -> Result<(), ParseError> {
        let line = event.line();
        self.sender
            .send(Ok(event))
            .map_err(|_| stream_error(line, "event consumer disconnected"))
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
/// parser coupled to iteration, so neither input nor events are accumulated.
pub struct EventDecoder {
    receiver: Receiver<Result<ToonEvent, ParseError>>,
    worker: Option<JoinHandle<()>>,
}

impl Iterator for EventDecoder {
    type Item = Result<ToonEvent, ParseError>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.receiver.recv() {
            Ok(event) => Some(event),
            Err(_) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                None
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

/// Decode events directly from a buffered reader with one classified line of
/// lookahead. The reader is moved to a worker so each iterator step can suspend
/// the recursive grammar exactly at an event boundary.
pub fn decode_event_reader<R>(reader: R, options: &DecodeStreamOptions) -> EventDecoder
where
    R: BufRead + Send + 'static,
{
    let (sender, receiver) = sync_channel(0);
    let ctx = StreamCtx {
        indent_size: options.indent,
        strict: options.strict,
        object_array_columns: options.object_array_columns,
        max_depth: options.max_depth,
        truncation_span: Cell::new(None),
    };
    let error_sender = sender.clone();
    let worker = std::thread::Builder::new()
        .stack_size(EVENT_DECODER_STACK_SIZE)
        .spawn(move || {
            let mut sink = ChannelSink { sender };
            if let Err(error) = decode_events_into(reader, &ctx, &mut sink) {
                let _ = error_sender.send(Err(error));
            }
        })
        .expect("failed to spawn TOON event decoder");
    EventDecoder {
        receiver,
        worker: Some(worker),
    }
}

pub fn decode_event_stream(input: &str, options: &DecodeStreamOptions) -> EventDecoder {
    decode_event_reader(Cursor::new(input.as_bytes().to_vec()), options)
}
