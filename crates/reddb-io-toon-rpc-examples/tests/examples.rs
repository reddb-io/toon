//! The examples, end to end: the generated client and dispatcher in process,
//! and every binary as a real process.

use std::process::Stdio;

use reddb_io_toon_rpc::{encode_frame, Client, ClientError, ClientOptions, ErrorCode, Params};
use reddb_io_toon_rpc_examples::calculator_api::{CalculatorClient, Stats, Vec2};
use reddb_io_toon_rpc_examples::calculator_dispatcher;
use reddb_io_toon_rpc_tcp::{connect_tcp, TcpServer};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[tokio::test]
async fn the_generated_client_calls_the_generated_dispatcher() {
    let server = TcpServer::bind("127.0.0.1:0", calculator_dispatcher())
        .await
        .unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(server.serve());
    let client = Client::duplex(connect_tcp(addr).await.unwrap(), ClientOptions::default());
    let calculator = CalculatorClient::new(client.clone());

    assert_eq!(calculator.add(2.0, 3.0).await.unwrap(), 5.0);
    assert_eq!(calculator.norm(Vec2 { x: 3.0, y: 4.0 }).await.unwrap(), 5.0);
    assert_eq!(
        calculator.stats(vec![1.0, 2.0, 6.0]).await.unwrap(),
        Stats {
            count: 3,
            mean: Some(3.0)
        }
    );
    assert_eq!(
        calculator.stats(vec![]).await.unwrap(),
        Stats {
            count: 0,
            mean: None
        }
    );
    let zero = calculator.divide(1.0, 0.0).await.unwrap_err();
    assert!(matches!(zero, ClientError::Rpc(error) if error.code == ErrorCode::InvalidParams));

    // Positional params in declaration order work too; the wrong shape is refused.
    let positional = client
        .call("add", Params::ByPosition(vec![json!(2), json!(3)]))
        .await;
    // TOON writes 5.0 canonically as 5, so compare the number, not the token.
    assert_eq!(positional.unwrap().as_f64(), Some(5.0));
    let wrong = client
        .call("add", Params::ByPosition(vec![json!(2)]))
        .await
        .unwrap_err();
    assert!(matches!(wrong, ClientError::Rpc(error) if error.code == ErrorCode::InvalidParams));
    client.close().await.unwrap();
}

async fn output(command: &mut Command) -> String {
    let output = command.output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn the_http_binaries_talk_to_each_other() {
    let mut server = Command::new(env!("CARGO_BIN_EXE_calculator_server"))
        .arg("127.0.0.1:0")
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let ready = lines.next_line().await.unwrap().unwrap();
    let url = ready.strip_prefix("listening on ").unwrap().to_owned() + "/";
    let answer =
        output(Command::new(env!("CARGO_BIN_EXE_calculator_client")).args(["add", "2", "3", &url]))
            .await;
    assert_eq!(answer, "add 2 3 = 5\n");
}

#[tokio::test]
async fn the_stdio_client_spawns_its_server() {
    let answer = output(
        Command::new(env!("CARGO_BIN_EXE_calculator_stdio_client")).args(["divide", "9", "3"]),
    )
    .await;
    assert_eq!(answer, "divide 9 3 = 3\n");
}

#[tokio::test]
async fn the_multi_dialect_server_answers_each_frame_in_kind() {
    let mut server = Command::new(env!("CARGO_BIN_EXE_multi_calculator_server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = server.stdin.take().unwrap();
    stdin
        .write_all(&encode_frame(
            br#"{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}"#,
        ))
        .await
        .unwrap();
    stdin
        .write_all(&encode_frame(
            b"toonrpc: \"1.0\"\nmethod: add\nparams[2]: 4,5\nid: 2",
        ))
        .await
        .unwrap();
    drop(stdin);
    let output = server.wait_with_output().await.unwrap();
    let mut reader = reddb_io_toon_rpc::FrameReader::new(&output.stdout[..]);
    let json = reader.next_document().await.unwrap().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        (json["jsonrpc"].clone(), json["result"].clone()),
        (json!("2.0"), json!(5.0))
    );
    let toon = reader.next_document().await.unwrap().unwrap();
    let toon = reddb_io_toon_rpc::response_from_wire(&toon).unwrap();
    assert_eq!(toon.result.and_then(|result| result.as_f64()), Some(9.0));
}
