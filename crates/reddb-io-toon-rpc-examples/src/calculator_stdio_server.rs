//! The calculator over stdio (§8.1 frames); `calculator_stdio_client` spawns it.

use reddb_io_toon_rpc_examples::calculator_dispatcher;

#[tokio::main]
async fn main() -> Result<(), reddb_io_toon_rpc::RpcError> {
    eprintln!("calculator stdio server ready");
    reddb_io_toon_rpc_stdio::serve_stdio(&calculator_dispatcher()).await
}
