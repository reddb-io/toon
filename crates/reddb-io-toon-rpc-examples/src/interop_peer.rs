//! One side of the TS ↔ Rust interop matrix (`pnpm test:rpc-interop`).
//!
//! - `interop_peer serve <transport>` serves the calculator and prints
//!   `ready <url>` once it listens (stdio serves this process's stdin/stdout
//!   and prints nothing).
//! - `interop_peer call <transport> <url>` runs the shared call sequence
//!   against a server and exits non-zero on the first mismatch. For stdio the
//!   "url" is the server command as a JSON array, which the client spawns.
//!
//! Transports: `http`, `ws`, `tcp`, `sse`, `stdio`. The TypeScript peer is
//! `packages/toon-rpc/test/interop/peer.mjs`.

use reddb_io_toon_rpc::{Client, ClientError, ClientOptions, ErrorCode, Params};
use reddb_io_toon_rpc_examples::calculator_api::{CalculatorClient, Stats, Vec2};
use reddb_io_toon_rpc_examples::calculator_dispatcher;
use serde_json::json;

type Failure = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let outcome = match args.as_slice() {
        ["serve", transport] => serve(transport).await,
        ["call", transport, url] => call(transport, url).await,
        _ => Err("usage: interop_peer serve <transport> | call <transport> <url>".into()),
    };
    if let Err(error) = outcome {
        eprintln!("interop_peer: {error}");
        std::process::exit(1);
    }
}

async fn serve(transport: &str) -> Result<(), Failure> {
    let dispatcher = calculator_dispatcher();
    let local = "127.0.0.1:0";
    match transport {
        "tcp" => {
            let server = reddb_io_toon_rpc_tcp::TcpServer::bind(local, dispatcher).await?;
            println!("ready tcp://{}", server.local_addr()?);
            server.serve().await?;
        }
        "http" => {
            let server = reddb_io_toon_rpc_http::HttpServer::bind(local, dispatcher).await?;
            println!("ready http://{}/rpc", server.local_addr()?);
            server.serve().await?;
        }
        "ws" => {
            let server = reddb_io_toon_rpc_ws::WsServer::bind(local, dispatcher).await?;
            println!("ready ws://{}", server.local_addr()?);
            server.serve().await?;
        }
        "sse" => {
            let server = reddb_io_toon_rpc_sse::SseServer::bind(local, dispatcher).await?;
            println!("ready http://{}/rpc", server.local_addr()?);
            server.serve().await?;
        }
        "stdio" => reddb_io_toon_rpc_stdio::serve_stdio(&dispatcher).await?,
        other => return Err(format!("unknown transport {other}").into()),
    }
    Ok(())
}

async fn call(transport: &str, url: &str) -> Result<(), Failure> {
    let options = ClientOptions::default();
    let mut child = None;
    let client = match transport {
        "tcp" => Client::duplex(
            reddb_io_toon_rpc_tcp::connect_tcp(url.trim_start_matches("tcp://")).await?,
            options,
        ),
        "http" => Client::request_response(
            reddb_io_toon_rpc_http::HttpTransport::new(url.parse()?),
            options,
        ),
        "ws" => Client::duplex(reddb_io_toon_rpc_ws::WsClient::connect(url).await?, options),
        "sse" => Client::duplex(
            reddb_io_toon_rpc_sse::SseTransport::connect(url.parse()?).await?,
            options,
        ),
        "stdio" => {
            let command: Vec<String> = serde_json::from_str(url)?;
            let (program, arguments) = command.split_first().ok_or("empty server command")?;
            let mut spawn = tokio::process::Command::new(program);
            spawn.args(arguments);
            let (transport, spawned) = reddb_io_toon_rpc_stdio::spawn(&mut spawn)?;
            child = Some(spawned);
            Client::duplex(transport, options)
        }
        other => return Err(format!("unknown transport {other}").into()),
    };
    let outcome = sequence(&client).await;
    client.close().await?;
    drop(child);
    outcome
}

/// The shared call sequence; `peer.mjs` runs the same one.
async fn sequence(client: &Client) -> Result<(), Failure> {
    let calculator = CalculatorClient::new(client.clone());
    expect("add", calculator.add(2.0, 3.0).await?, 5.0)?;
    expect("norm", calculator.norm(Vec2 { x: 3.0, y: 4.0 }).await?, 5.0)?;
    let text = "multi\n\nline: with, delimiters";
    expect("echo", calculator.echo(text.into()).await?, text.to_owned())?;
    expect(
        "stats",
        calculator.stats(vec![1.0, 2.0, 6.0]).await?,
        Stats {
            count: 3,
            mean: Some(3.0),
        },
    )?;
    expect(
        "empty stats",
        calculator.stats(vec![]).await?,
        Stats {
            count: 0,
            mean: None,
        },
    )?;
    match calculator.divide(1.0, 0.0).await {
        Err(ClientError::Rpc(error)) if error.code == ErrorCode::InvalidParams => {}
        other => return Err(format!("divide by zero: {other:?}").into()),
    }
    client
        .notify("add", Params::ByPosition(vec![json!(1), json!(2)]))
        .await?;
    let calls = (0..8)
        .map(|n| tokio::spawn(client.call("add", Params::ByPosition(vec![json!(n), json!(100)]))))
        .collect::<Vec<_>>();
    for (n, call) in calls.into_iter().enumerate() {
        let sum = call.await??.as_f64();
        expect("concurrent add", sum, Some(n as f64 + 100.0))?;
    }
    Ok(())
}

fn expect<T: PartialEq + std::fmt::Debug>(
    what: &str,
    actual: T,
    expected: T,
) -> Result<(), Failure> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what}: expected {expected:?}, got {actual:?}").into())
    }
}
