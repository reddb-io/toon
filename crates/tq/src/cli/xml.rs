use quick_xml::events::{BytesCData, BytesDecl, BytesEnd, BytesPI, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use reddb_io_toon::Value;
use serde_json::{Map, Value as JsonValue};

const MAX_XML_DEPTH: usize = 256;
const MAX_XML_NODES: usize = 1_000_000;
const MAX_DIAGNOSTIC_CHARS: usize = 300;

struct ElementBuilder {
    name: String,
    attributes: Vec<JsonValue>,
    children: Vec<JsonValue>,
}

pub(super) fn parse_xml_value(input: &str) -> Result<Value, String> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().check_end_names = true;
    reader.config_mut().expand_empty_elements = false;

    let mut declaration = JsonValue::Null;
    let mut children = Vec::new();
    let mut stack: Vec<ElementBuilder> = Vec::new();
    let mut root_seen = false;
    let mut root_closed = false;
    let mut nodes = 0_usize;

    loop {
        let event = reader.read_event().map_err(|error| {
            xml_error(
                reader.error_position(),
                &format!("malformed input: {error}"),
            )
        })?;
        match event {
            Event::Decl(event) => {
                if declaration != JsonValue::Null || root_seen || !stack.is_empty() {
                    return Err(xml_error(reader.error_position(), "misplaced declaration"));
                }
                declaration = declaration_value(&reader, &event)?;
            }
            Event::Start(event) => {
                count_node(&mut nodes, reader.error_position())?;
                if stack.len() >= MAX_XML_DEPTH {
                    return Err(xml_error(
                        reader.error_position(),
                        &format!("maximum depth of {MAX_XML_DEPTH} exceeded"),
                    ));
                }
                if stack.is_empty() {
                    begin_root(&mut root_seen, root_closed, reader.error_position())?;
                }
                stack.push(element_builder(&reader, &event)?);
            }
            Event::Empty(event) => {
                count_node(&mut nodes, reader.error_position())?;
                if stack.is_empty() {
                    begin_root(&mut root_seen, root_closed, reader.error_position())?;
                }
                let element = element_value(element_builder(&reader, &event)?, true);
                append_node(&mut stack, &mut children, element);
                if stack.is_empty() {
                    root_closed = true;
                }
            }
            Event::End(_) => {
                let element = stack.pop().ok_or_else(|| {
                    xml_error(reader.error_position(), "closing element without a start")
                })?;
                append_node(&mut stack, &mut children, element_value(element, false));
                if stack.is_empty() {
                    root_closed = true;
                }
            }
            Event::Text(event) => {
                let value = event.unescape().map_err(|error| {
                    xml_error(
                        reader.error_position(),
                        &format!("invalid text entity: {error}"),
                    )
                })?;
                if stack.is_empty() {
                    if !value.chars().all(char::is_whitespace) {
                        return Err(xml_error(
                            reader.error_position(),
                            "text is not allowed outside the document element",
                        ));
                    }
                } else if !value.is_empty() {
                    count_node(&mut nodes, reader.error_position())?;
                    append_node(
                        &mut stack,
                        &mut children,
                        leaf_value("text", value.into_owned()),
                    );
                }
            }
            Event::CData(event) => {
                require_inside_element(&stack, reader.error_position(), "CDATA")?;
                count_node(&mut nodes, reader.error_position())?;
                let value = event.decode().map_err(|error| {
                    xml_error(reader.error_position(), &format!("invalid CDATA: {error}"))
                })?;
                append_node(
                    &mut stack,
                    &mut children,
                    leaf_value("cdata", value.into_owned()),
                );
            }
            Event::Comment(event) => {
                count_node(&mut nodes, reader.error_position())?;
                let value = decode(&reader, event.as_ref(), "comment")?;
                append_node(&mut stack, &mut children, leaf_value("comment", value));
            }
            Event::PI(event) => {
                count_node(&mut nodes, reader.error_position())?;
                let target = decode(&reader, event.target(), "processing instruction target")?;
                let value = decode(&reader, event.content(), "processing instruction")?
                    .trim_start_matches([' ', '\t', '\r', '\n'])
                    .to_owned();
                append_node(
                    &mut stack,
                    &mut children,
                    processing_instruction_value(target, value),
                );
            }
            Event::DocType(_) => {
                return Err(xml_error(
                    reader.error_position(),
                    "DOCTYPE declarations are not supported",
                ));
            }
            Event::Eof => break,
        }
    }

    if !stack.is_empty() {
        return Err(xml_error(input.len() as u64, "unclosed element"));
    }
    if !root_seen {
        return Err(xml_error(0, "document element is missing"));
    }

    let mut document = Map::new();
    document.insert("declaration".to_owned(), declaration);
    document.insert("children".to_owned(), JsonValue::Array(children));
    let mut wrapper = Map::new();
    wrapper.insert("$xml".to_owned(), JsonValue::Object(document));
    Ok(Value::from_json_value(JsonValue::Object(wrapper)))
}

pub(super) fn format_xml_value(value: &Value) -> Result<String, String> {
    let json = value.to_json_value();
    let wrapper = object(
        &json,
        "expected canonical XML document with a `$xml` object",
    )?;
    exact_keys(wrapper, &["$xml"], "canonical XML wrapper")?;
    let document = object(
        required(wrapper, "$xml", "canonical XML wrapper")?,
        "`$xml` must be an object",
    )?;
    exact_keys(
        document,
        &["declaration", "children"],
        "canonical XML document",
    )?;

    let mut writer = Writer::new(Vec::new());
    write_declaration(
        &mut writer,
        required(document, "declaration", "XML document")?,
    )?;
    let children = array(
        required(document, "children", "XML document")?,
        "XML children",
    )?;
    for child in children {
        write_node(&mut writer, child, 0)?;
    }
    let bytes = writer.into_inner();
    let output = String::from_utf8(bytes).map_err(|error| format!("XML output: {error}"))?;
    parse_xml_value(&output).map_err(|error| format!("invalid canonical XML tree: {error}"))?;
    Ok(output)
}

fn declaration_value(reader: &Reader<&[u8]>, event: &BytesDecl<'_>) -> Result<JsonValue, String> {
    let mut declaration = Map::new();
    let version = event.version().map_err(|error| {
        xml_error(
            reader.error_position(),
            &format!("invalid declaration: {error}"),
        )
    })?;
    declaration.insert(
        "version".to_owned(),
        JsonValue::String(decode(reader, &version, "declaration version")?),
    );
    if let Some(encoding) = event.encoding() {
        let encoding = encoding.map_err(|error| {
            xml_error(
                reader.error_position(),
                &format!("invalid declaration encoding: {error}"),
            )
        })?;
        declaration.insert(
            "encoding".to_owned(),
            JsonValue::String(decode(reader, &encoding, "declaration encoding")?),
        );
    }
    if let Some(standalone) = event.standalone() {
        let standalone = standalone.map_err(|error| {
            xml_error(
                reader.error_position(),
                &format!("invalid standalone declaration: {error}"),
            )
        })?;
        declaration.insert(
            "standalone".to_owned(),
            JsonValue::String(decode(reader, &standalone, "standalone declaration")?),
        );
    }
    Ok(JsonValue::Object(declaration))
}

fn element_builder(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
) -> Result<ElementBuilder, String> {
    let name = decode(reader, event.name().as_ref(), "element name")?;
    let mut attributes = Vec::new();
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| {
            xml_error(
                reader.error_position(),
                &format!("invalid attribute: {error}"),
            )
        })?;
        let mut value = Map::new();
        value.insert(
            "name".to_owned(),
            JsonValue::String(decode(reader, attribute.key.as_ref(), "attribute name")?),
        );
        let decoded = attribute
            .decode_and_unescape_value(reader.decoder())
            .map_err(|error| {
                xml_error(
                    reader.error_position(),
                    &format!("invalid attribute value: {error}"),
                )
            })?;
        value.insert("value".to_owned(), JsonValue::String(decoded.into_owned()));
        attributes.push(JsonValue::Object(value));
    }
    Ok(ElementBuilder {
        name,
        attributes,
        children: Vec::new(),
    })
}

fn element_value(element: ElementBuilder, empty: bool) -> JsonValue {
    let mut value = Map::new();
    value.insert("type".to_owned(), JsonValue::String("element".to_owned()));
    value.insert("name".to_owned(), JsonValue::String(element.name));
    value.insert(
        "attributes".to_owned(),
        JsonValue::Array(element.attributes),
    );
    value.insert("children".to_owned(), JsonValue::Array(element.children));
    value.insert("empty".to_owned(), JsonValue::Bool(empty));
    JsonValue::Object(value)
}

fn leaf_value(kind: &str, value: String) -> JsonValue {
    let mut node = Map::new();
    node.insert("type".to_owned(), JsonValue::String(kind.to_owned()));
    node.insert("value".to_owned(), JsonValue::String(value));
    JsonValue::Object(node)
}

fn processing_instruction_value(target: String, value: String) -> JsonValue {
    let mut node = Map::new();
    node.insert(
        "type".to_owned(),
        JsonValue::String("processing_instruction".to_owned()),
    );
    node.insert("target".to_owned(), JsonValue::String(target));
    node.insert("value".to_owned(), JsonValue::String(value));
    JsonValue::Object(node)
}

fn append_node(
    stack: &mut [ElementBuilder],
    document_children: &mut Vec<JsonValue>,
    node: JsonValue,
) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        document_children.push(node);
    }
}

fn begin_root(root_seen: &mut bool, root_closed: bool, position: u64) -> Result<(), String> {
    if *root_seen || root_closed {
        return Err(xml_error(position, "multiple document elements"));
    }
    *root_seen = true;
    Ok(())
}

fn require_inside_element(
    stack: &[ElementBuilder],
    position: u64,
    kind: &str,
) -> Result<(), String> {
    if stack.is_empty() {
        Err(xml_error(
            position,
            &format!("{kind} is not allowed outside the document element"),
        ))
    } else {
        Ok(())
    }
}

fn count_node(nodes: &mut usize, position: u64) -> Result<(), String> {
    *nodes += 1;
    if *nodes > MAX_XML_NODES {
        Err(xml_error(
            position,
            &format!("maximum node count of {MAX_XML_NODES} exceeded"),
        ))
    } else {
        Ok(())
    }
}

fn decode(reader: &Reader<&[u8]>, bytes: &[u8], kind: &str) -> Result<String, String> {
    reader
        .decoder()
        .decode(bytes)
        .map(|value| value.into_owned())
        .map_err(|error| xml_error(reader.error_position(), &format!("invalid {kind}: {error}")))
}

fn write_declaration(writer: &mut Writer<Vec<u8>>, value: &JsonValue) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    let declaration = object(value, "XML declaration must be an object or null")?;
    for key in declaration.keys() {
        if !matches!(key.as_str(), "version" | "encoding" | "standalone") {
            return Err(format!("XML declaration has unsupported field `{key}`"));
        }
    }
    let version = string(
        required(declaration, "version", "XML declaration")?,
        "XML declaration version",
    )?;
    let encoding = optional_string(declaration, "encoding", "XML declaration encoding")?;
    let standalone = optional_string(declaration, "standalone", "XML standalone declaration")?;
    if !matches!(version, "1.0" | "1.1") {
        return Err("XML declaration version must be `1.0` or `1.1`".to_owned());
    }
    if standalone.is_some_and(|value| !matches!(value, "yes" | "no")) {
        return Err("XML standalone declaration must be `yes` or `no`".to_owned());
    }
    writer
        .write_event(Event::Decl(BytesDecl::new(version, encoding, standalone)))
        .map_err(write_error)
}

fn write_node(writer: &mut Writer<Vec<u8>>, value: &JsonValue, depth: usize) -> Result<(), String> {
    if depth > MAX_XML_DEPTH {
        return Err(format!("XML maximum depth of {MAX_XML_DEPTH} exceeded"));
    }
    let node = object(value, "XML child node must be an object")?;
    let kind = string(required(node, "type", "XML node")?, "XML node type")?;
    match kind {
        "element" => write_element(writer, node, depth),
        "text" | "cdata" | "comment" => write_leaf(writer, node, kind),
        "processing_instruction" => write_processing_instruction(writer, node),
        _ => Err(format!("unsupported XML node type `{kind}`")),
    }
}

fn write_element(
    writer: &mut Writer<Vec<u8>>,
    node: &Map<String, JsonValue>,
    depth: usize,
) -> Result<(), String> {
    exact_keys(
        node,
        &["type", "name", "attributes", "children", "empty"],
        "XML element",
    )?;
    let name = string(required(node, "name", "XML element")?, "XML element name")?;
    let attributes = array(
        required(node, "attributes", "XML element")?,
        "XML element attributes",
    )?;
    let children = array(
        required(node, "children", "XML element")?,
        "XML element children",
    )?;
    let empty = boolean(
        required(node, "empty", "XML element")?,
        "XML element empty flag",
    )?;
    if empty && !children.is_empty() {
        return Err("empty XML element cannot contain children".to_owned());
    }

    let mut start = BytesStart::new(name);
    for attribute in attributes {
        let attribute = object(attribute, "XML attribute must be an object")?;
        exact_keys(attribute, &["name", "value"], "XML attribute")?;
        let key = string(
            required(attribute, "name", "XML attribute")?,
            "XML attribute name",
        )?;
        let value = string(
            required(attribute, "value", "XML attribute")?,
            "XML attribute value",
        )?;
        start.push_attribute((key, value));
    }

    if empty {
        writer
            .write_event(Event::Empty(start))
            .map_err(write_error)?;
        return Ok(());
    }
    writer
        .write_event(Event::Start(start))
        .map_err(write_error)?;
    for child in children {
        write_node(writer, child, depth + 1)?;
    }
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .map_err(write_error)
}

fn write_leaf(
    writer: &mut Writer<Vec<u8>>,
    node: &Map<String, JsonValue>,
    kind: &str,
) -> Result<(), String> {
    exact_keys(node, &["type", "value"], "XML leaf node")?;
    let value = string(required(node, "value", "XML leaf node")?, "XML node value")?;
    let event = match kind {
        "text" => Event::Text(BytesText::new(value)),
        "cdata" => Event::CData(BytesCData::new(value)),
        "comment" => Event::Comment(BytesText::new(value)),
        _ => unreachable!("write_leaf only handles leaf node types"),
    };
    writer.write_event(event).map_err(write_error)
}

fn write_processing_instruction(
    writer: &mut Writer<Vec<u8>>,
    node: &Map<String, JsonValue>,
) -> Result<(), String> {
    exact_keys(
        node,
        &["type", "target", "value"],
        "XML processing instruction",
    )?;
    let target = string(
        required(node, "target", "XML processing instruction")?,
        "XML processing instruction target",
    )?;
    let value = string(
        required(node, "value", "XML processing instruction")?,
        "XML processing instruction value",
    )?;
    let content = if value.is_empty() {
        target.to_owned()
    } else {
        format!("{target} {value}")
    };
    writer
        .write_event(Event::PI(BytesPI::new(content)))
        .map_err(write_error)
}

fn object<'a>(value: &'a JsonValue, message: &str) -> Result<&'a Map<String, JsonValue>, String> {
    value.as_object().ok_or_else(|| message.to_owned())
}

fn array<'a>(value: &'a JsonValue, message: &str) -> Result<&'a [JsonValue], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| message.to_owned())
}

fn string<'a>(value: &'a JsonValue, field: &str) -> Result<&'a str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("{field} must be a string"))
}

fn boolean(value: &JsonValue, field: &str) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{field} must be a boolean"))
}

fn optional_string<'a>(
    object: &'a Map<String, JsonValue>,
    key: &str,
    field: &str,
) -> Result<Option<&'a str>, String> {
    object
        .get(key)
        .map(|value| string(value, field))
        .transpose()
}

fn required<'a>(
    object: &'a Map<String, JsonValue>,
    key: &str,
    context: &str,
) -> Result<&'a JsonValue, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{context} is missing `{key}`"))
}

fn exact_keys(
    object: &Map<String, JsonValue>,
    expected: &[&str],
    context: &str,
) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !expected.contains(&key.as_str())) {
        return Err(format!("{context} has unsupported field `{key}`"));
    }
    Ok(())
}

fn write_error(error: std::io::Error) -> String {
    format!("XML output: {error}")
}

fn xml_error(position: u64, message: &str) -> String {
    let bounded: String = message.chars().take(MAX_DIAGNOSTIC_CHARS).collect();
    let suffix = if message.chars().count() > MAX_DIAGNOSTIC_CHARS {
        "…"
    } else {
        ""
    };
    format!("XML error at byte {position}: {bounded}{suffix}")
}
