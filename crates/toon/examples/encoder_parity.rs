//! JSON-lines adapter used by the cross-language encoder parity fuzzer.

use reddb_io_toon::{encode_v4, EncodeV4Options, Value};
use std::error::Error;
use std::io::{self, BufRead, BufWriter, Write};

fn main() -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let mut stdout = BufWriter::new(io::stdout().lock());

    for line in stdin.lock().lines() {
        let json = serde_json::from_str(&line?)?;
        let wire = encode_v4(&Value::from_json_value(json), EncodeV4Options::default())?;
        serde_json::to_writer(&mut stdout, &wire)?;
        writeln!(stdout)?;
    }

    Ok(())
}
