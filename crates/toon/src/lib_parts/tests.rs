#[cfg(test)]
static TABULAR_ROW_DECODE_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn count_tabular_row_decode_for_tests() {
    #[cfg(test)]
    {
        TABULAR_ROW_DECODE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
fn reset_tabular_row_decode_count_for_tests() {
    TABULAR_ROW_DECODE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
fn tabular_row_decode_count_for_tests() -> usize {
    TABULAR_ROW_DECODE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::{Document, EncodeOptions, Value};

    #[test]
    fn parses_flat_fields_and_serializes_canonical_toon() {
        let document = Document::parse("name : Ada\nactive: true\ncount: 3\n").unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "name: Ada\nactive: true\ncount: 3\n"
        );
    }

    #[test]
    fn returns_top_level_value_by_name() {
        let document = Document::parse("name: Ada\n").unwrap();

        assert_eq!(
            document.get("name").unwrap().to_canonical_toon(),
            "Ada".to_owned()
        );
    }

    #[test]
    fn parses_nested_objects() {
        let document = Document::parse("person:\n  address:\n    city: London\n").unwrap();

        let person = document.get("person").and_then(Value::as_object).unwrap();
        let address = person.get("address").and_then(Value::as_object).unwrap();

        assert_eq!(
            address.get("city").unwrap().to_canonical_toon(),
            "London".to_owned()
        );
    }

    #[test]
    fn rejects_scalar_children() {
        let error = Document::parse("person: Ada\n  city: London\n").unwrap_err();

        assert_eq!(error.line(), 2);
        assert_eq!(error.message(), "invalid indentation");
    }

    #[test]
    fn parses_inline_list_arrays_and_serializes_canonical_toon() {
        let document = Document::parse("tags[3]: admin,ops,dev\n").unwrap();

        assert_eq!(document.to_canonical_toon(), "tags[3]: admin,ops,dev\n");
    }

    #[test]
    fn parses_tabular_arrays_and_serializes_canonical_toon() {
        let document =
            Document::parse("users[2]{id,name,active}:\n  1,Ada,true\n  2,\"Bob Smith\",false\n")
                .unwrap();

        assert_eq!(
            document.to_canonical_toon(),
            "users[2]{id,name,active}:\n  1,Ada,true\n  2,Bob Smith,false\n"
        );
    }

    #[test]
    fn parses_nested_tabular_headers() {
        let document = Document::parse(
            "orders[2]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n  2,Bob,US,20\n",
        )
        .unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({
                "orders": [
                    { "id": 1, "customer": { "name": "Ada", "country": "UK" }, "total": 10.5 },
                    { "id": 2, "customer": { "name": "Bob", "country": "US" }, "total": 20 }
                ]
            })
        );
    }

    #[test]
    fn serializes_nested_tabular_headers_only_when_opted_in() {
        let document = Value::from_json_value(serde_json::json!({
            "orders": [
                { "id": 1, "customer": { "name": "Ada", "country": "UK" }, "total": 10.5 },
                { "id": 2, "customer": { "name": "Bob", "country": "US" }, "total": 20 }
            ]
        }));
        let expanded =
            "orders[2]:\n  - id: 1\n    customer:\n      name: Ada\n      country: UK\n    total: 10.5\n  - id: 2\n    customer:\n      name: Bob\n      country: US\n    total: 20\n";
        let nested =
            "orders[2]{id,customer{name,country},total}:\n  1,Ada,UK,10.5\n  2,Bob,US,20\n";
        let options = EncodeOptions {
            nested_tabular_headers: true,
            keyed_map_collapse: false,
            ..EncodeOptions::default()
        };

        assert_eq!(document.to_canonical_toon(), expanded);
        assert_eq!(document.to_toon_with_options(options), nested);
        assert_eq!(
            Value::parse_toon(nested).unwrap().to_json_value(),
            document.to_json_value()
        );
    }

    #[test]
    fn nested_tabular_serialization_falls_back_on_recursive_shape_mismatch() {
        let document = Value::from_json_value(serde_json::json!({
            "rows": [
                { "id": 1, "point": { "x": 1, "y": 2 } },
                { "id": 2, "point": { "x": 3, "z": 4 } }
            ]
        }));

        assert_eq!(
            document.to_toon_with_options(EncodeOptions {
                nested_tabular_headers: true,
                keyed_map_collapse: false,
                ..EncodeOptions::default()
            }),
            "rows[2]:\n  - id: 1\n    point:\n      x: 1\n      y: 2\n  - id: 2\n    point:\n      x: 3\n      z: 4\n"
        );
    }

    #[test]
    fn nested_tabular_headers_validate_leaf_arity_and_shape() {
        let arity = Document::parse("orders[1]{id,customer{name,country}}:\n  1,Ada\n")
            .expect_err("leaf count controls row arity");
        assert_eq!(arity.line(), 2);
        assert_eq!(arity.message(), "array row length mismatch");

        let empty = Document::parse("orders[1]{id,customer{}}:\n  1\n")
            .expect_err("empty nested groups are invalid");
        assert_eq!(empty.line(), 1);
        assert_eq!(empty.message(), "invalid array header");

        let duplicate = Document::parse("orders[1]{customer{name},customer{name}}:\n  Ada,Bob\n")
            .expect_err("duplicate leaf paths are invalid");
        assert_eq!(duplicate.line(), 1);
        assert_eq!(duplicate.message(), "duplicate key");

        let unbalanced = Document::parse("orders[1]{id,customer{name,country}:\n  1,Ada,UK\n")
            .expect_err("unbalanced nested groups are invalid");
        assert_eq!(unbalanced.line(), 1);
        assert_eq!(unbalanced.message(), "invalid array header");
    }

    #[test]
    fn parses_keyed_map_collapse_rows() {
        let document = Document::parse(
            "people{first,last,meta{active,score}}:\n  joe: Joe,Schmoe,true,7\n  mary: Mary,Jane,false,9\n",
        )
        .unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({
                "people": {
                    "joe": { "first": "Joe", "last": "Schmoe", "meta": { "active": true, "score": 7 } },
                    "mary": { "first": "Mary", "last": "Jane", "meta": { "active": false, "score": 9 } }
                }
            })
        );
    }

    #[test]
    fn serializes_keyed_map_collapse_only_when_opted_in() {
        let document = Value::from_json_value(serde_json::json!({
            "people": {
                "joe": { "first": "Joe", "last": "Schmoe" },
                "mary": { "first": "Mary", "last": "Jane" }
            }
        }));
        let expanded =
            "people:\n  joe:\n    first: Joe\n    last: Schmoe\n  mary:\n    first: Mary\n    last: Jane\n";
        let collapsed = "people{first,last}:\n  joe: Joe,Schmoe\n  mary: Mary,Jane\n";

        assert_eq!(document.to_canonical_toon(), expanded);
        assert_eq!(
            document.to_toon_with_options(EncodeOptions {
                nested_tabular_headers: false,
                keyed_map_collapse: true,
                ..EncodeOptions::default()
            }),
            collapsed
        );
    }

    #[test]
    fn keyed_map_collapse_falls_back_for_non_uniform_maps() {
        let document = Value::from_json_value(serde_json::json!({
            "people": {
                "joe": { "first": "Joe", "last": "Schmoe" },
                "mary": { "first": "Mary", "role": "admin" }
            }
        }));

        assert_eq!(
            document.to_toon_with_options(EncodeOptions {
                nested_tabular_headers: false,
                keyed_map_collapse: true,
                ..EncodeOptions::default()
            }),
            "people:\n  joe:\n    first: Joe\n    last: Schmoe\n  mary:\n    first: Mary\n    role: admin\n"
        );
    }

    #[test]
    fn treats_leading_plus_tokens_as_strings() {
        // The spec is silent on leading-plus tokens (upstream spec PR #52);
        // the reference implementation keeps them as strings while exponent
        // plus signs stay numeric.
        let document = Document::parse("values[3]: +1,+1.5,+1e2\nexponent: 1e+2\n").unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({"values": ["+1", "+1.5", "+1e2"], "exponent": 100})
        );
    }

    #[test]
    fn nested_empty_object_list_items_round_trip_as_bare_hyphen() {
        // The bare `-` marker for an empty object list item applies
        // recursively inside nested expanded arrays, with no trailing space
        // (upstream spec PR #53).
        let input = "items[2]:\n  - [1]:\n    -\n  - [2]:\n    - x\n    -\n";
        let document = Document::parse(input).unwrap();

        assert_eq!(
            document.to_json_value(),
            serde_json::json!({"items": [[{}], ["x", {}]]})
        );
        assert_eq!(document.to_canonical_toon(), input);
    }

    #[test]
    fn rejects_array_length_mismatches() {
        let error = Document::parse("tags[2]: admin,ops,dev\n").unwrap_err();

        assert_eq!(error.line(), 1);
        assert_eq!(error.message(), "array length mismatch");
    }

    #[test]
    fn decodes_only_touched_tabular_rows() {
        let document =
            Document::parse("users[3]{id,name}:\n  1,Ada\n  2,Bob\n  3,Chloe\n").unwrap();
        let users = document.get("users").and_then(Value::as_array).unwrap();

        super::reset_tabular_row_decode_count_for_tests();
        let row = users.get(1).unwrap();

        assert_eq!(row.to_canonical_toon(), "id: 2\nname: Bob\n");
        assert_eq!(super::tabular_row_decode_count_for_tests(), 1);
    }
}
