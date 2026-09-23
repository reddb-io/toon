//! The CLI end to end: it reports its real version, generates code, and calls
//! a server over each transport it accepts.

use std::process::Output;

use reddb_io_toon_rpc_examples::calculator_dispatcher;
use tokio::process::Command;

async fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reddb-io-toon-rpc"))
        .args(args)
        .output()
        .await
        .unwrap()
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[tokio::test]
async fn version_is_the_crate_version() {
    let output = cli(&["--version"]).await;
    assert_eq!(
        stdout(&output),
        format!("reddb-io-toon-rpc {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn generate_prints_the_module_for_each_language() {
    let idl = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../reddb-io-toon-rpc-codegen/examples/calculator.toonrpc"
    );
    let rust = stdout(&cli(&["generate", idl, "--lang", "rust"]).await);
    assert!(rust.contains("pub trait Calculator"));
    let ts = stdout(&cli(&["generate", idl, "--lang", "ts"]).await);
    assert!(ts.contains("export class CalculatorClient"));
}

#[tokio::test]
async fn call_reaches_http_ws_and_tcp_servers() {
    let http = reddb_io_toon_rpc_http::HttpServer::bind("127.0.0.1:0", calculator_dispatcher())
        .await
        .unwrap();
    let http_url = format!("http://{}/", http.local_addr().unwrap());
    tokio::spawn(http.serve());
    let ws = reddb_io_toon_rpc_ws::WsServer::bind("127.0.0.1:0", calculator_dispatcher())
        .await
        .unwrap();
    let ws_url = format!("ws://{}", ws.local_addr().unwrap());
    tokio::spawn(ws.serve());
    let tcp = reddb_io_toon_rpc_tcp::TcpServer::bind("127.0.0.1:0", calculator_dispatcher())
        .await
        .unwrap();
    let tcp_url = format!("tcp://{}", tcp.local_addr().unwrap());
    tokio::spawn(tcp.serve());

    for url in [&http_url, &ws_url, &tcp_url] {
        assert_eq!(
            stdout(&cli(&["call", url, "add", "[2]: 2,3"]).await),
            "5\n",
            "{url}"
        );
        assert_eq!(
            stdout(&cli(&["call", url, "norm", "v:\n  x: 3\n  y: 4"]).await),
            "5\n",
            "{url}"
        );
    }
    let refused = cli(&["call", &tcp_url, "divide", "[2]: 1,0"]).await;
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("division by zero"));
}
