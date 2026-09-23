//! `reddb-io-toon-rpc-gen <file.toonrpc> --rust|--ts`: print the generated
//! module for one language.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let (Some(file), Some(language)) = (args.first(), args.get(1)) else {
        eprintln!("usage: reddb-io-toon-rpc-gen <file.toonrpc> --rust|--ts");
        return ExitCode::from(2);
    };
    let idl = match std::fs::read_to_string(file) {
        Ok(idl) => idl,
        Err(error) => {
            eprintln!("{file}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let service = match reddb_io_toon_rpc_codegen::parse(&idl) {
        Ok(service) => service,
        Err(error) => {
            eprintln!("{file}: {error}");
            return ExitCode::FAILURE;
        }
    };
    match language.as_str() {
        "--rust" => print!("{}", reddb_io_toon_rpc_codegen::generate_rust(&service)),
        "--ts" => print!(
            "{}",
            reddb_io_toon_rpc_codegen::generate_typescript(&service)
        ),
        other => {
            eprintln!("unknown language {other}: use --rust or --ts");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
