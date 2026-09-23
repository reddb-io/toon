//! The calculator over stdio (§8.1 frames), answering JSON-RPC 2.0 and
//! TOON-RPC 1.0 alike: each frame is detected on its own and answered in its
//! own dialect.

use reddb_io_toon_rpc::{write_frame, FrameReader, MultiRpc, RpcError};
use reddb_io_toon_rpc_examples::calculator_dispatcher;

#[tokio::main]
async fn main() -> Result<(), RpcError> {
    let multi = MultiRpc::new(calculator_dispatcher());
    let mut input = FrameReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    eprintln!("multi-dialect calculator ready");
    while let Some(document) = input.next_document().await? {
        let response = multi.handle(&document, None)?;
        if !response.is_empty() {
            write_frame(&mut output, &response).await?;
        }
    }
    Ok(())
}
