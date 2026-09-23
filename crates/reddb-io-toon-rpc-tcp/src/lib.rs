//! TOON-RPC over TCP and Unix sockets, framed per spec §8.1.
//!
//! Each connection carries length-prefixed frames in both directions. The
//! server dispatches every request document in order and answers with one
//! frame per non-empty response; a framing error closes the connection, since
//! the stream has no point to resynchronize from.

use std::io;
use std::net::SocketAddr;

use reddb_io_toon_rpc::{serve_framed, Dispatcher, FramedTransport, RpcError};
use tokio::net::{tcp, TcpListener, TcpStream, ToSocketAddrs};

/// A client transport over one TCP connection.
pub type TcpTransport = FramedTransport<tcp::OwnedReadHalf, tcp::OwnedWriteHalf>;

/// Connect to a TOON-RPC TCP server.
pub async fn connect_tcp(addr: impl ToSocketAddrs) -> Result<TcpTransport, RpcError> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|error| RpcError::TransportError(error.to_string()))?;
    let (reader, writer) = stream.into_split();
    Ok(FramedTransport::new(reader, writer))
}

/// A TCP server bound to an address the caller chose.
pub struct TcpServer {
    listener: TcpListener,
    dispatcher: Dispatcher,
}

impl TcpServer {
    pub async fn bind(addr: impl ToSocketAddrs, dispatcher: Dispatcher) -> io::Result<Self> {
        Ok(Self::from_listener(
            TcpListener::bind(addr).await?,
            dispatcher,
        ))
    }

    pub fn from_listener(listener: TcpListener, dispatcher: Dispatcher) -> Self {
        Self {
            listener,
            dispatcher,
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept connections until accepting fails, serving each on its own task.
    pub async fn serve(self) -> io::Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let dispatcher = self.dispatcher.clone();
            tokio::spawn(async move {
                let (reader, writer) = stream.into_split();
                // A failed connection only ends itself.
                let _ = serve_framed(reader, writer, &dispatcher).await;
            });
        }
    }
}

#[cfg(unix)]
pub use unix::{connect_unix, UnixServer, UnixTransport};

#[cfg(unix)]
mod unix {
    use std::io;
    use std::path::{Path, PathBuf};

    use reddb_io_toon_rpc::{serve_framed, Dispatcher, FramedTransport, RpcError};
    use tokio::net::{unix, UnixListener, UnixStream};

    /// A client transport over one Unix socket connection.
    pub type UnixTransport = FramedTransport<unix::OwnedReadHalf, unix::OwnedWriteHalf>;

    /// Connect to a TOON-RPC Unix socket server.
    pub async fn connect_unix(path: impl AsRef<Path>) -> Result<UnixTransport, RpcError> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|error| RpcError::TransportError(error.to_string()))?;
        let (reader, writer) = stream.into_split();
        Ok(FramedTransport::new(reader, writer))
    }

    /// A Unix socket server. Binding replaces a stale socket file.
    pub struct UnixServer {
        listener: UnixListener,
        path: PathBuf,
        dispatcher: Dispatcher,
    }

    impl UnixServer {
        pub fn bind(path: impl Into<PathBuf>, dispatcher: Dispatcher) -> io::Result<Self> {
            let path = path.into();
            match std::fs::remove_file(&path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
            let listener = UnixListener::bind(&path)?;
            Ok(Self {
                listener,
                path,
                dispatcher,
            })
        }

        pub fn path(&self) -> &Path {
            &self.path
        }

        /// Accept connections until accepting fails, serving each on its own task.
        pub async fn serve(self) -> io::Result<()> {
            loop {
                let (stream, _) = self.listener.accept().await?;
                let dispatcher = self.dispatcher.clone();
                tokio::spawn(async move {
                    let (reader, writer) = stream.into_split();
                    let _ = serve_framed(reader, writer, &dispatcher).await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reddb_io_toon_rpc::{Client, ClientError, ClientOptions, ErrorCode, Params};
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn dispatcher() -> Dispatcher {
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("echo", |params, _id| match params {
            Params::ByPosition(mut values) if !values.is_empty() => Ok(values.remove(0)),
            _ => Ok(json!(null)),
        });
        dispatcher
    }

    async fn server() -> SocketAddr {
        let server = TcpServer::bind("127.0.0.1:0", dispatcher()).await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(server.serve());
        addr
    }

    #[tokio::test]
    async fn concurrent_calls_are_correlated_by_id() {
        let client = Client::duplex(
            connect_tcp(server().await).await.unwrap(),
            ClientOptions::default(),
        );
        // A multi-line document with a blank line inside must survive framing.
        let text = "line one\n\nline three";
        let calls = (0..16).map(|n| {
            client.call(
                "echo",
                Params::ByPosition(vec![json!(format!("{text} #{n}"))]),
            )
        });
        let results = futures_join(calls).await;
        for (n, result) in results.into_iter().enumerate() {
            assert_eq!(result.unwrap(), json!(format!("{text} #{n}")));
        }
        let missing = client.call("missing", Params::Absent).await.unwrap_err();
        assert!(
            matches!(missing, ClientError::Rpc(error) if error.code == ErrorCode::MethodNotFound)
        );
        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn notifications_get_no_frame_back() {
        let mut stream = TcpStream::connect(server().await).await.unwrap();
        let notification = b"toonrpc: \"1.0\"\nmethod: echo";
        stream
            .write_all(&reddb_io_toon_rpc::encode_frame(notification))
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, b"");
    }

    #[tokio::test]
    async fn a_framing_error_closes_the_connection() {
        let mut stream = TcpStream::connect(server().await).await.unwrap();
        stream.write_all(b"toonrpc: \"1.0\"\n\n").await.unwrap();
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, b"");
    }

    #[tokio::test]
    async fn the_server_closing_rejects_pending_calls() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream);
        });
        let client = Client::duplex(connect_tcp(addr).await.unwrap(), ClientOptions::default());
        let error = client.call("echo", Params::Absent).await.unwrap_err();
        assert!(matches!(
            error,
            ClientError::Closed(_) | ClientError::Transport(_)
        ));
        assert_eq!(client.pending_call_count(), 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_sockets_use_the_same_framing() {
        let path = std::env::temp_dir().join(format!("toon-rpc-{}.sock", std::process::id()));
        let server = UnixServer::bind(&path, dispatcher()).unwrap();
        tokio::spawn(server.serve());
        let client = Client::duplex(connect_unix(&path).await.unwrap(), ClientOptions::default());
        let result = client
            .call("echo", Params::ByPosition(vec![json!("over unix")]))
            .await
            .unwrap();
        assert_eq!(result, json!("over unix"));
        client.close().await.unwrap();
        let _ = std::fs::remove_file(path);
    }

    async fn futures_join<F: std::future::Future + Send + 'static>(
        futures: impl Iterator<Item = F>,
    ) -> Vec<F::Output>
    where
        F::Output: Send + 'static,
    {
        let handles = futures.map(tokio::spawn).collect::<Vec<_>>();
        let mut outputs = Vec::new();
        for handle in handles {
            outputs.push(handle.await.unwrap());
        }
        outputs
    }
}
