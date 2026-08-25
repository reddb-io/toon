//! Length-prefixed stream framing for TOON-RPC byte-stream transports.
//!
//! The core protocol operates on complete RPC documents, and byte streams
//! (TCP, Unix sockets, stdio) carry no document boundaries of their own. This
//! profile — normative in `docs/toon-rpc-spec.md` section 8.1 — makes the
//! boundary explicit instead of inferring it from newlines, which a multi-line
//! TOON document cannot guarantee to be free of:
//!
//! ```text
//! frame = length , LF , payload , LF
//! ```
//!
//! `length` is the payload size in bytes as ASCII decimal digits with no sign
//! and no leading zeros (a lone `0` is valid), LF is byte 0x0A, and `payload`
//! is exactly `length` bytes of one complete RPC document. The trailing LF is
//! a frame terminator, not part of the payload. Any deviation — a non-digit in
//! the length, a missing terminator, a length too large to represent — is a
//! framing error, and a decoder MUST fail the stream rather than resynchronize.

/// Longest accepted length header: 15 digits keeps the value a safe integer.
const MAX_LENGTH_DIGITS: usize = 15;

const LF: u8 = 0x0a;
const DIGIT_0: u8 = b'0';
const DIGIT_9: u8 = b'9';

/// A violation of the length-prefixed framing profile. The stream carrying it
/// has no recoverable resynchronization point and must be abandoned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Invalid TOON-RPC stream frame: {detail}")]
pub struct FramingError {
    detail: String,
}

impl FramingError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// The violation, without the shared prefix.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Encode one complete RPC document as a single stream frame.
pub fn encode_frame(document: &[u8]) -> Vec<u8> {
    let header = document.len().to_string();
    let mut frame = Vec::with_capacity(header.len() + document.len() + 2);
    frame.extend_from_slice(header.as_bytes());
    frame.push(LF);
    frame.extend_from_slice(document);
    frame.push(LF);
    frame
}

/// Incremental decoder: push arbitrary chunk splits in, pull complete
/// documents out. A framing violation poisons the decoder — every later call
/// returns the same error, because the stream cannot be resynchronized.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    failure: Option<FramingError>,
}

impl FrameDecoder {
    /// A decoder positioned at a frame boundary with an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a chunk and return every document completed by it, in order.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, FramingError> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        self.buffer.extend_from_slice(chunk);

        let mut documents = Vec::new();
        loop {
            match self.take_frame() {
                Ok(Some(document)) => documents.push(document),
                Ok(None) => return Ok(documents),
                Err(failure) => return Err(failure),
            }
        }
    }

    /// True when a partially received frame is still buffered.
    pub fn has_partial_frame(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Assert the stream ended cleanly on a frame boundary.
    pub fn finish(&mut self) -> Result<(), FramingError> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        if !self.buffer.is_empty() {
            return Err(self.fail("stream ended inside a frame"));
        }
        Ok(())
    }

    fn take_frame(&mut self) -> Result<Option<Vec<u8>>, FramingError> {
        let Some(header_end) = self.buffer.iter().position(|byte| *byte == LF) else {
            if self.buffer.len() > MAX_LENGTH_DIGITS {
                return Err(self.fail("frame length header is not terminated"));
            }
            return Ok(None);
        };
        if header_end == 0 {
            return Err(self.fail("frame length is empty"));
        }
        if header_end > MAX_LENGTH_DIGITS {
            return Err(self.fail("frame length header is too long"));
        }

        // At most 15 digits, so the accumulator cannot overflow a u64; the
        // conversion to usize is what a 32-bit target can legitimately reject.
        let mut length: u64 = 0;
        for index in 0..header_end {
            let byte = self.buffer[index];
            if !(DIGIT_0..=DIGIT_9).contains(&byte) {
                return Err(self.fail("frame length is not a decimal integer"));
            }
            length = length * 10 + u64::from(byte - DIGIT_0);
        }
        if header_end > 1 && self.buffer[0] == DIGIT_0 {
            return Err(self.fail("frame length has a leading zero"));
        }
        let Ok(length) = usize::try_from(length) else {
            return Err(self.fail("frame length exceeds the addressable range"));
        };

        let frame_end = header_end + 1 + length;
        if self.buffer.len() <= frame_end {
            return Ok(None);
        }
        if self.buffer[frame_end] != LF {
            return Err(self.fail("frame payload is not terminated"));
        }

        let document = self.buffer[header_end + 1..frame_end].to_vec();
        self.buffer.drain(..frame_end + 1);
        Ok(Some(document))
    }

    fn fail(&mut self, detail: &str) -> FramingError {
        let failure = FramingError::new(detail);
        self.failure = Some(failure.clone());
        failure
    }
}
