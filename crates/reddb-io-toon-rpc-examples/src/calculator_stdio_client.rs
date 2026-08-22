//! Calculator client using stdio transport
//!
//! Usage:
//!   echo '{ toonrpc: "1.0" method: add params: [5, 3] id: 1 }' | cargo run --bin calculator_stdio_client
//!
//! Or in a pipe: cat request.txt | cargo run --bin calculator_stdio_client
//!
//! Messages must end with an empty line.

use std::io::{self, BufRead, Write};
use reddb_io_toon_rpc::from_wire;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let mut buffer = String::new();
    for line in stdin.lock().lines() {
        let line = line?;

        if line.is_empty() {
            if !buffer.is_empty() {
                match from_wire(buffer.trim().as_bytes()) {
                    Ok(msg) => {
                        writeln!(stdout, "{:#?}", msg)?;
                        stdout.flush()?;
                    }
                    Err(e) => {
                        writeln!(stdout, "Error: {}", e)?;
                        stdout.flush()?;
                    }
                }
                buffer.clear();
            }
        } else {
            buffer.push_str(&line);
            buffer.push('\n');
        }
    }

    Ok(())
}
