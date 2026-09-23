//! Length-prefixed stream framing for TOON-RPC byte-stream transports (§8.1).
//!
//! Byte streams (TCP, Unix sockets, stdio) carry no document boundaries of
//! their own, and a multi-line TOON document cannot guarantee to be free of
//! blank lines, so the boundary is explicit:
//!
//! ```text
//! frame = length , LF , payload , LF
//! ```
//!
//! `length` is the payload size in bytes as ASCII decimal digits with no sign
//! or leading zeros (a lone `0` is valid), and `payload` is exactly `length`
//! bytes of one complete RPC document. Any deviation is a framing error, and a
//! decoder fails the stream instead of resynchronizing. This mirrors
//! `packages/toon-rpc/src/framing.ts`.

/// Longest accepted length header: 15 digits keeps the value a safe integer.
pub const MAX_LENGTH_DIGITS: usize = 15;

/// Largest payload a decoder accepts unless configured otherwise.
pub const DEFAULT_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

const LF: u8 = b'\n';

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Invalid TOON-RPC stream frame: {0}")]
pub struct FramingError(pub String);

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

/// Incremental decoder: push arbitrary chunk splits in, take complete
/// documents out. A framing violation poisons the decoder.
#[derive(Debug)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    max_frame_bytes: usize,
    failure: Option<FramingError>,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::with_max_frame_bytes(DEFAULT_MAX_FRAME_BYTES)
    }

    pub fn with_max_frame_bytes(max_frame_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_frame_bytes,
            failure: None,
        }
    }

    /// Append a chunk and return every document completed by it, in order.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, FramingError> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        self.buffer.extend_from_slice(chunk);
        let mut documents = Vec::new();
        while let Some(document) = self.take_frame()? {
            documents.push(document);
        }
        Ok(documents)
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
        let Some(header_end) = self.buffer.iter().position(|&byte| byte == LF) else {
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
        let header = &self.buffer[..header_end];
        if !header.iter().all(u8::is_ascii_digit) {
            return Err(self.fail("frame length is not a decimal integer"));
        }
        if header_end > 1 && header[0] == b'0' {
            return Err(self.fail("frame length has a leading zero"));
        }
        let length = header
            .iter()
            .fold(0u64, |length, &digit| length * 10 + u64::from(digit - b'0'));
        let Some(length) = usize::try_from(length)
            .ok()
            .filter(|&length| length <= self.max_frame_bytes)
        else {
            return Err(self.fail("frame payload exceeds the size limit"));
        };

        let frame_end = header_end + 1 + length;
        if self.buffer.len() <= frame_end {
            return Ok(None);
        }
        if self.buffer[frame_end] != LF {
            return Err(self.fail("frame payload is not terminated"));
        }
        let document = self.buffer[header_end + 1..frame_end].to_vec();
        self.buffer.drain(..=frame_end);
        Ok(Some(document))
    }

    fn fail(&mut self, detail: &str) -> FramingError {
        let failure = FramingError(detail.to_owned());
        self.failure = Some(failure.clone());
        self.buffer = Vec::new();
        failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_documents_across_arbitrary_splits() {
        let mut stream = encode_frame(b"a: 1\n\nb: 2");
        stream.extend(encode_frame(b""));
        stream.extend(encode_frame(b"c"));
        for split in 1..stream.len() {
            let mut decoder = FrameDecoder::new();
            let mut documents = decoder.push(&stream[..split]).unwrap();
            documents.extend(decoder.push(&stream[split..]).unwrap());
            assert_eq!(
                documents,
                [b"a: 1\n\nb: 2".to_vec(), Vec::new(), b"c".to_vec()]
            );
            decoder.finish().unwrap();
        }
    }

    #[test]
    fn rejects_malformed_frames_and_stays_failed() {
        for (input, detail) in [
            (&b"\n"[..], "frame length is empty"),
            (b"1x\n", "frame length is not a decimal integer"),
            (b"01\nab\n", "frame length has a leading zero"),
            (b"1234567890123456", "frame length header is not terminated"),
            (b"1234567890123456\n", "frame length header is too long"),
            (b"1\nab", "frame payload is not terminated"),
        ] {
            let mut decoder = FrameDecoder::new();
            assert_eq!(decoder.push(input), Err(FramingError(detail.into())));
            assert_eq!(decoder.push(b"1\na\n"), Err(FramingError(detail.into())));
        }
    }

    #[test]
    fn enforces_the_payload_limit_before_buffering_it() {
        let mut decoder = FrameDecoder::with_max_frame_bytes(4);
        assert!(decoder.push(b"4\nabcd\n").is_ok());
        assert_eq!(
            decoder.push(b"5\n"),
            Err(FramingError("frame payload exceeds the size limit".into()))
        );
    }

    #[test]
    fn a_stream_ending_inside_a_frame_is_an_error() {
        let mut decoder = FrameDecoder::new();
        decoder.push(b"3\nab").unwrap();
        assert!(decoder.has_partial_frame());
        assert_eq!(
            decoder.finish(),
            Err(FramingError("stream ended inside a frame".into()))
        );
    }
}
