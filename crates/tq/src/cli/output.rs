use reddb_io_toon::{encode_toonl_values, encode_with_options, EncodeV4Options, Value};
use serde::Serialize;

use super::args::{Format, Options};
use super::xml::format_xml_value;

pub(super) fn format_values(values: &[Value], options: &Options) -> Result<String, String> {
    let sorted_values;
    let values = if options.sort_keys {
        sorted_values = values
            .iter()
            .cloned()
            .map(|mut value| {
                value.sort_object_keys();
                value
            })
            .collect::<Vec<_>>();
        &sorted_values
    } else {
        values
    };

    if options.output_format == Format::Toonl {
        return encode_toonl_values(values).map_err(|error| error.to_string());
    }

    let mut output = String::new();
    for value in values {
        if options.raw_output || options.join_output {
            if let Value::String(value) = value {
                output.push_str(value);
                if !options.join_output {
                    output.push('\n');
                }
                continue;
            }
        }

        match options.output_format {
            Format::Json => {
                output.push_str(&format_json(value, options.compact, options.indent_size)?);
                if !options.join_output {
                    output.push('\n');
                }
            }
            Format::Toon => {
                let encoded = encode_with_options(
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
                .map_err(|error| error.to_string())?;
                if options.join_output {
                    output.push_str(encoded.trim_end_matches('\n'));
                } else {
                    output.push_str(&encoded);
                }
                if !options.join_output && !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            Format::Xml => {
                output.push_str(&format_xml_value(value)?);
                if !options.join_output {
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
    if matches!(value, Value::Number(number) if number == "-0") {
        return Ok("-0".to_owned());
    }
    let value = json_value(value);
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

fn json_value(value: &Value) -> serde_json::Value {
    match value {
        Value::Array(array) => {
            serde_json::Value::Array(array.values().iter().map(json_value).collect())
        }
        Value::Number(number) if number.parse::<f64>().is_ok_and(f64::is_nan) => {
            serde_json::Value::Null
        }
        Value::Number(number) if number.parse::<f64>().is_ok_and(f64::is_infinite) => {
            let finite = if number.starts_with('-') {
                "-1.7976931348623157e+308"
            } else {
                "1.7976931348623157e+308"
            };
            serde_json::from_str(finite).expect("valid finite JSON number")
        }
        _ => value.to_json_value(),
    }
}
