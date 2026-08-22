use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "toon-rpc")]
#[command(version = "0.1.0")]
#[command(about = "TOON-RPC: JSON-RPC 2.0 with TOON serialization", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Dev {
        #[arg(help = "Service file (.toonrpc)")]
        service: String,
    },
    Call {
        #[arg(help = "Method name")]
        method: String,
        #[arg(help = "Parameters as TOON")]
        params: Option<String>,
    },
    Generate {
        #[arg(help = "IDL file (.toonrpc)")]
        file: String,
        #[arg(long, default_value = "rust")]
        lang: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Dev { service } => {
            println!("toon-rpc dev {} (not implemented yet)", service);
        }
        Commands::Call { method, params } => {
            println!("toon-rpc call {} {:?}", method, params);
        }
        Commands::Generate { file, lang } => {
            println!("toon-rpc generate {} --lang {}", file, lang);
        }
    }

    Ok(())
}
