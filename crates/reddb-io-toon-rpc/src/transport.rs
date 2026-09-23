//! Client transport contracts and the framed byte-stream transport (§8).
//!
//! A transport carries complete RPC documents; it never interprets them. The
//! two shapes mirror `packages/toon-rpc/src/transport.ts`: a duplex transport
//! delivers documents in both directions independently (WebSocket, SSE, and
//! byte streams with §8.1 framing), while a request/response transport ties an
//! optional response document to each request (HTTP).

use std::collections::VecDeque;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::dispatcher::Dispatcher;
use crate::error::{Error, ErrorCode, RpcError};
use crate::framing::{encode_frame, FrameDecoder};
use crate::protocol::{Message, Response};
use crate::types::Id;

/// A transport that yields exactly one complete RPC document per item.
#[async_trait]
pub trait DuplexTransport: Send + Sync + 'static {
    /// Send one complete RPC document.
    async fn send(&self, document: Vec<u8>) -> Result<(), RpcError>;

    /// The next complete RPC document, or `None` once the peer ended the
    /// stream cleanly. A client calls this from a single receive task.
    async fn recv(&self) -> Result<Option<Vec<u8>>, RpcError>;

    /// Release the transport. A client stops its receive task first.
    async fn close(&self) -> Result<(), RpcError>;
}

/// A transport where each request directly owns its optional response.
#[async_trait]
pub trait RequestResponseTransport: Send + Sync + 'static {
    /// Send one RPC document and return the response document, if any. An
    /// empty or absent response means the peer had nothing to answer.
    async fn request(&self, document: Vec<u8>) -> Result<Option<Vec<u8>>, RpcError>;

    /// Release the transport.
    async fn close(&self) -> Result<(), RpcError> {
        Ok(())
    }
}

/// Reads §8.1 frames from a byte stream, one complete document at a time.
pub struct FrameReader<R> {
    inner: R,
    decoder: FrameDecoder,
    ready: VecDeque<Vec<u8>>,
    ended: bool,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(inner: R) -> Self {
        Self::with_decoder(inner, FrameDecoder::new())
    }

    pub fn with_decoder(inner: R, decoder: FrameDecoder) -> Self {
        Self {
            inner,
            decoder,
            ready: VecDeque::new(),
            ended: false,
        }
    }

    /// The next document, or `None` when the stream ended on a frame
    /// boundary. A framing violation or a stream ending inside a frame is an
    /// error, and the stream cannot be resumed after it.
    pub async fn next_document(&mut self) -> Result<Option<Vec<u8>>, RpcError> {
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(document) = self.ready.pop_front() {
                return Ok(Some(document));
            }
            if self.ended {
                return Ok(None);
            }
            let read = self.inner.read(&mut chunk).await.map_err(transport_error)?;
            if read == 0 {
                self.ended = true;
                self.decoder.finish().map_err(transport_error)?;
                continue;
            }
            let documents = self.decoder.push(&chunk[..read]).map_err(transport_error)?;
            self.ready.extend(documents);
        }
    }
}

/// Write one document as a §8.1 frame and flush it.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    document: &[u8],
) -> Result<(), RpcError> {
    writer
        .write_all(&encode_frame(document))
        .await
        .map_err(transport_error)?;
    writer.flush().await.map_err(transport_error)
}

/// A duplex transport over any byte stream, framed per §8.1.
pub struct FramedTransport<R, W> {
    reader: Mutex<FrameReader<R>>,
    writer: Mutex<Option<W>>,
}

impl<R, W> FramedTransport<R, W>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self::with_decoder(reader, writer, FrameDecoder::new())
    }

    pub fn with_decoder(reader: R, writer: W, decoder: FrameDecoder) -> Self {
        Self {
            reader: Mutex::new(FrameReader::with_decoder(reader, decoder)),
            writer: Mutex::new(Some(writer)),
        }
    }
}

#[async_trait]
impl<R, W> DuplexTransport for FramedTransport<R, W>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    async fn send(&self, document: Vec<u8>) -> Result<(), RpcError> {
        let mut writer = self.writer.lock().await;
        let writer = writer
            .as_mut()
            .ok_or_else(|| RpcError::TransportError("transport is closed".into()))?;
        write_frame(writer, &document).await
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, RpcError> {
        self.reader.lock().await.next_document().await
    }

    async fn close(&self) -> Result<(), RpcError> {
        if let Some(mut writer) = self.writer.lock().await.take() {
            writer.shutdown().await.map_err(transport_error)?;
        }
        Ok(())
    }
}

/// Serve one framed byte stream: every request document is dispatched in
/// order and every non-empty response goes back as a frame. Returns when the
/// peer ends the stream; a framing error ends it without resynchronizing.
pub async fn serve_framed<R, W>(
    reader: R,
    mut writer: W,
    dispatcher: &Dispatcher,
) -> Result<(), RpcError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut reader = FrameReader::new(reader);
    while let Some(document) = reader.next_document().await? {
        let response = dispatch_document(dispatcher, &document);
        if !response.is_empty() {
            write_frame(&mut writer, &response).await?;
        }
    }
    writer.shutdown().await.map_err(transport_error)
}

/// Dispatch one document. A failure to encode the response becomes a TOON
/// Internal error, so a transport never answers in another format.
pub fn dispatch_document(dispatcher: &Dispatcher, document: &[u8]) -> Vec<u8> {
    dispatcher.dispatch(document).unwrap_or_else(|_| {
        let response = Response::error(Error::new(ErrorCode::InternalError), Id::Null);
        crate::to_wire(&Message::SingleResponse(response)).unwrap_or_default()
    })
}

fn transport_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::TransportError(error.to_string())
}
