//! JSON-lines adapter used by the cross-language encoder parity fuzzer.

use reddb_io_toon::{encode_with_options, EncodeOptions, Value};
use std::error::Error;
use std::io::{self, BufRead, BufWriter, Write};

fn main() -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let mut stdout = BufWriter::new(io::stdout().lock());

    for line in stdin.lock().lines() {
        let json = serde_json::from_str(&line?)?;
        let wire = encode_with_options(&Value::from_json_value(json), EncodeOptions::default())?;
        serde_json::to_writer(&mut stdout, &wire)?;
        writeln!(stdout)?;
    }

    Ok(())
}
