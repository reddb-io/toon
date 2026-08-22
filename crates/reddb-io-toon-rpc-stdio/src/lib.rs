use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite, stdin, stdout, Stdout, Stdin};
use std::task::{Context, Poll};

pub struct StdioTransport {
    input: Stdin,
    output: Stdout,
}

impl StdioTransport {
    pub fn new() -> Self {
        Self {
            input: stdin(),
            output: stdout(),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

pub struct StdioSend {
    output: Stdout,
}

pub struct StdioRecv {
    input: Stdin,
}

impl StdioTransport {
    pub fn split(self) -> (StdioSend, StdioRecv) {
        (StdioSend { output: self.output }, StdioRecv { input: self.input })
    }
}

impl Stream for StdioRecv {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = vec![0u8; 4096];
        let mut read_buf = tokio::io::ReadBuf::new(&mut buf);
        
        match Pin::new(&mut self.input).poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => {
                let n = read_buf.filled().len();
                if n == 0 {
                    return Poll::Ready(None);
                }
                buf.truncate(n);
                Poll::Ready(Some(Ok(Bytes::from(buf))))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for StdioSend {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.output).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.output).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.output).poll_shutdown(cx)
    }
}

impl Stream for StdioSend {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}
