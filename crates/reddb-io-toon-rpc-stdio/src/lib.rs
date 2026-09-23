//! TOON-RPC over stdio, framed per spec §8.1.
//!
//! A server reads request frames from stdin and writes response frames to
//! stdout. A client spawns the server process and talks to it through the
//! child's pipes. Anything else the server prints must go to stderr.

use reddb_io_toon_rpc::{serve_framed, Dispatcher, FramedTransport, RpcError};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Serve this process's stdin and stdout until stdin ends.
pub async fn serve_stdio(dispatcher: &Dispatcher) -> Result<(), RpcError> {
    serve_framed(tokio::io::stdin(), tokio::io::stdout(), dispatcher).await
}

/// A client transport over a child process's stdout (in) and stdin (out).
pub type StdioTransport = FramedTransport<ChildStdout, ChildStdin>;

/// Spawn `command` with piped stdin and stdout and wrap its pipes. The child
/// is killed when the returned handle is dropped; stderr is inherited.
pub fn spawn(command: &mut Command) -> Result<(StdioTransport, Child), RpcError> {
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| RpcError::TransportError(error.to_string()))?;
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    Ok((FramedTransport::new(stdout, stdin), child))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use reddb_io_toon_rpc::DuplexTransport;

    #[tokio::test]
    async fn frames_cross_the_child_pipes_intact() {
        // `cat` echoes every frame, so each document must come back whole.
        let (transport, mut child) = spawn(&mut Command::new("cat")).unwrap();
        for document in [&b"a: 1\n\nb: 2"[..], b"", b"c"] {
            transport.send(document.to_vec()).await.unwrap();
            assert_eq!(transport.recv().await.unwrap().as_deref(), Some(document));
        }
        transport.close().await.unwrap();
        assert_eq!(transport.recv().await.unwrap(), None);
        assert!(child.wait().await.unwrap().success());
    }
}
