//! `reddb-io-toon-rpc`: generate service code and call TOON-RPC servers.
//!
//! - `generate <file.toonrpc> --lang rust|ts` prints the generated module.
//! - `call <url> <method> [params]` calls a server and prints the result as
//!   TOON. The URL picks the transport: `http://` (request/response),
//!   `ws://` (WebSocket) or `tcp://host:port` (§8.1 frames). Params are a
//!   TOON array (by position) or object (by name).

use clap::{Parser, Subcommand, ValueEnum};
use reddb_io_toon_rpc::{Client, ClientOptions, Params};
use reddb_io_toon_rpc_http::HttpTransport;
use reddb_io_toon_rpc_tcp::connect_tcp;
use reddb_io_toon_rpc_ws::WsClient;

#[derive(Parser)]
#[command(
    name = "reddb-io-toon-rpc",
    version,
    about = "Generate TOON-RPC service code and call TOON-RPC servers"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the code generated from a `.toonrpc` IDL file.
    Generate {
        file: String,
        #[arg(long, value_enum, default_value_t = Language::Rust)]
        lang: Language,
    },
    /// Call a method and print its result as TOON.
    Call {
        /// `http://…`, `ws://…` or `tcp://host:port`.
        url: String,
        method: String,
        /// Params as TOON: an array (by position) or an object (by name).
        params: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Language {
    Rust,
    Ts,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Generate { file, lang } => {
            let service = reddb_io_toon_rpc_codegen::parse(&std::fs::read_to_string(&file)?)
                .map_err(|error| anyhow::anyhow!("{file}: {error}"))?;
            print!(
                "{}",
                match lang {
                    Language::Rust => reddb_io_toon_rpc_codegen::generate_rust(&service),
                    Language::Ts => reddb_io_toon_rpc_codegen::generate_typescript(&service),
                }
            );
        }
        Command::Call {
            url,
            method,
            params,
        } => {
            let params = match params {
                None => Params::Absent,
                Some(text) => match reddb_io_toon::decode(&text)?.to_json_value() {
                    serde_json::Value::Array(values) => Params::ByPosition(values),
                    serde_json::Value::Object(object) => Params::ByName(object),
                    _ => anyhow::bail!("params must be a TOON array or object"),
                },
            };
            let client = connect(&url).await?;
            let result = client.call(&method, params).await;
            client.close().await?;
            let result = reddb_io_toon::Value::from_json_value(result?);
            println!("{}", reddb_io_toon::encode(&result)?);
        }
    }
    Ok(())
}

async fn connect(url: &str) -> anyhow::Result<Client> {
    let options = ClientOptions::default();
    Ok(if url.starts_with("http://") {
        Client::request_response(HttpTransport::new(url.parse()?), options)
    } else if url.starts_with("ws://") {
        Client::duplex(WsClient::connect(url).await?, options)
    } else if let Some(address) = url.strip_prefix("tcp://") {
        Client::duplex(connect_tcp(address).await?, options)
    } else {
        anyhow::bail!("unsupported URL {url}: use http://, ws:// or tcp://")
    })
}
