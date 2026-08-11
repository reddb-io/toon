use reddb_io_toon::{encode_toonl_values, encode_with_options, EncodeV4Options, Value};
use serde::Serialize;

use super::args::{Format, Options};

pub(super) fn format_values(values: &[Value], options: &Options) -> Result<String, String> {
    if options.output_format == Format::Toonl {
        return encode_toonl_values(values).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for value in values {
        if options.raw_output {
            if let Value::String(value) = value {
                output.push_str(value);
                output.push('\n');
                continue;
            }
        }

        match options.output_format {
            Format::Json => {
                output.push_str(&format_json(value, options.compact, options.indent_size)?);
                output.push('\n');
            }
            Format::Toon => {
                output.push_str(
                    &encode_with_options(
                        value,
                        EncodeV4Options {
                            primitive_array_columns: options.primitive_array_columns,
                            object_array_columns: options.object_array_columns,
                            cyclic_discriminated_arrays: options.cyclic_discriminated_arrays,
                            delimiter: options.delimiter,
                            indent_size: options.indent_size,
                            ..EncodeV4Options::default()
                        },
                    )
                    .map_err(|error| error.to_string())?,
                );
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            Format::Toonl => unreachable!("TOONL output is handled before the loop"),
            Format::Yaml => unreachable!("YAML output is not supported"),
        }
    }
    Ok(output)
}

fn format_json(value: &Value, compact: bool, indent_size: usize) -> Result<String, String> {
    let value = value.to_json_value();
    if compact || indent_size == 0 {
        return serde_json::to_string(&value).map_err(|error| error.to_string());
    }

    let indent = vec![b' '; indent_size];
    let formatter = serde_json::ser::PrettyFormatter::with_indent(&indent);
    let mut output = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    value
        .serialize(&mut serializer)
        .map_err(|error| error.to_string())?;
    String::from_utf8(output).map_err(|error| error.to_string())
}
