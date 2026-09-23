// Lossless JSON input. serde_json (built without `arbitrary_precision`, which
// would change every serde_json user in the workspace) reads an integer beyond
// i64/u64, or a decimal with more digits than an f64 holds, as the nearest f64.
// A cheap pre-scan sends only documents holding such a number through a second
// reader that keeps every number lexeme exactly; everything else, including
// error reporting, stays serde_json's.

/// Mantissas this long may not survive an f64: only 15 significant decimal
/// digits are guaranteed to round-trip, and 16 already pass 2^53.
const LOSSLESS_DIGITS: usize = 16;

/// Whether `input` holds a number whose mantissa has at least
/// [`LOSSLESS_DIGITS`] digits outside strings: the only case where serde_json
/// could round a number.
fn has_long_digit_run(input: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    let mut run = 0usize;
    for &byte in input.as_bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        if byte.is_ascii_digit() {
            run += 1;
            if run >= LOSSLESS_DIGITS {
                return true;
            }
        } else if byte == b'.' {
            // A fraction continues the mantissa.
        } else {
            run = 0;
            in_string = byte == b'"';
        }
    }
    false
}

/// Reads `input` keeping every number lexeme; containers are split with
/// `RawValue`, and every scalar is validated by serde_json as usual, so ranges,
/// escapes and error messages match the fast path.
fn lossless_json_value(input: &str) -> Result<Value, serde_json::Error> {
    let raw: Box<serde_json::value::RawValue> = serde_json::from_str(input)?;
    lossless_node(raw.get())
}

fn lossless_node(text: &str) -> Result<Value, serde_json::Error> {
    match text.as_bytes().first() {
        Some(b'{') => {
            let RawObject(entries) = serde_json::from_str(text)?;
            let fields = entries
                .into_iter()
                .map(|(key, raw)| Ok(Field { key, value: lossless_node(raw.get())? }))
                .collect::<Result<Vec<_>, serde_json::Error>>()?;
            Ok(Value::Object(Document { fields }))
        }
        Some(b'[') => {
            let items: Vec<Box<serde_json::value::RawValue>> = serde_json::from_str(text)?;
            let values = items
                .iter()
                .map(|raw| lossless_node(raw.get()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(Array::List(values)))
        }
        Some(b'-' | b'0'..=b'9') => {
            // Validate the token (range included), then keep its digits verbatim.
            let _: serde_json::Value = serde_json::from_str(text)?;
            Ok(Value::Number(text.to_owned()))
        }
        _ => serde_json::from_str(text).map(Value::from_json_value),
    }
}

/// A JSON object's members in document order, with serde_json's
/// `preserve_order` semantics for a repeated key: the last value wins, at the
/// first occurrence's position.
struct RawObject(Vec<(String, Box<serde_json::value::RawValue>)>);

impl<'de> serde::Deserialize<'de> for RawObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Members;
        impl<'de> serde::de::Visitor<'de> for Members {
            type Value = RawObject;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<RawObject, A::Error> {
                let mut entries: Vec<(String, Box<serde_json::value::RawValue>)> = Vec::new();
                let mut positions = std::collections::HashMap::new();
                while let Some((key, raw)) = map.next_entry::<String, Box<serde_json::value::RawValue>>()? {
                    match positions.get(&key) {
                        Some(&position) => entries[position] = (key, raw),
                        None => {
                            positions.insert(key.clone(), entries.len());
                            entries.push((key, raw));
                        }
                    }
                }
                Ok(RawObject(entries))
            }
        }
        deserializer.deserialize_map(Members)
    }
}
