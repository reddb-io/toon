//! The `.toonrpc` IDL: a TOON document describing one service.
//!
//! ```toon
//! service: Calculator
//! version: "1.0"
//! types:
//!   Vec2:
//!     x: f64
//!     y: f64
//! methods[2]:
//!   - name: add
//!     params:
//!       a: f64
//!       b: f64
//!     result: f64
//!   - name: norm
//!     params:
//!       v: Vec2
//!     result: f64
//! ```
//!
//! Types are `bool`, `i32`, `i64`, `u32`, `u64`, `f64`, `string`, `json` (any
//! value), `null` (results only), a type declared under `types`, `T[]` for a
//! list and `T?` for an optional value. Declaration order is kept everywhere,
//! so generated code is deterministic.

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub name: String,
    pub version: String,
    pub types: Vec<TypeDef>,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub name: String,
    pub params: Vec<Field>,
    pub result: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Bool,
    I32,
    I64,
    U32,
    U64,
    F64,
    String,
    Json,
    Null,
    Named(String),
    List(Box<Type>),
    Optional(Box<Type>),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IdlError {
    #[error("TOON parse error: {0}")]
    Toon(String),
    #[error("{0}")]
    Invalid(String),
}

fn invalid(message: impl Into<String>) -> IdlError {
    IdlError::Invalid(message.into())
}

/// Words that cannot name a type, method, parameter or field in either
/// generated language.
const RESERVED: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "crate",
    "debugger",
    "default",
    "delete",
    "do",
    "dyn",
    "else",
    "enum",
    "export",
    "extends",
    "extern",
    "false",
    "finally",
    "fn",
    "for",
    "function",
    "if",
    "impl",
    "import",
    "in",
    "instanceof",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "new",
    "null",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "switch",
    "this",
    "throw",
    "trait",
    "true",
    "try",
    "type",
    "typeof",
    "unsafe",
    "use",
    "var",
    "void",
    "where",
    "while",
    "with",
    "yield",
];

fn identifier(name: &str, what: &str) -> Result<String, IdlError> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid || RESERVED.contains(&name) {
        return Err(invalid(format!(
            "{what} `{name}` is not a usable identifier"
        )));
    }
    Ok(name.to_owned())
}

/// Parse and validate an IDL document.
pub fn parse(idl: &str) -> Result<Service, IdlError> {
    let root = reddb_io_toon::decode(idl)
        .map_err(|error| IdlError::Toon(error.message().to_owned()))?
        .to_json_value();
    let root = root
        .as_object()
        .ok_or_else(|| invalid("the IDL must be an object"))?;
    for key in root.keys() {
        if !matches!(key.as_str(), "service" | "version" | "types" | "methods") {
            return Err(invalid(format!("unknown IDL member `{key}`")));
        }
    }

    let name = root
        .get("service")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("`service` must name the service"))?;
    let name = identifier(name, "service")?;
    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
        return Err(invalid(format!(
            "service `{name}` must start with an uppercase letter"
        )));
    }
    let version = match root.get("version") {
        None => "1.0".to_owned(),
        Some(Value::String(version)) => version.clone(),
        Some(_) => return Err(invalid("`version` must be a string")),
    };

    let mut types = Vec::new();
    if let Some(declared) = root.get("types") {
        let declared = declared
            .as_object()
            .ok_or_else(|| invalid("`types` must map type names to fields"))?;
        for (type_name, fields) in declared {
            let type_name = identifier(type_name, "type")?;
            let fields = fields
                .as_object()
                .ok_or_else(|| invalid(format!("type `{type_name}` must map fields to types")))?;
            types.push(TypeDef {
                fields: parse_fields(fields, &format!("type `{type_name}`"))?,
                name: type_name,
            });
        }
    }

    let mut methods = Vec::new();
    for method in root
        .get("methods")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("`methods` must list the service's methods"))?
    {
        let method = method
            .as_object()
            .ok_or_else(|| invalid("each method must be an object"))?;
        for key in method.keys() {
            if !matches!(key.as_str(), "name" | "params" | "result") {
                return Err(invalid(format!("unknown method member `{key}`")));
            }
        }
        let method_name = method
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("each method needs a `name`"))?;
        let method_name = identifier(method_name, "method")?;
        let params = match method.get("params") {
            None => Vec::new(),
            Some(Value::Object(params)) => {
                parse_fields(params, &format!("method `{method_name}`"))?
            }
            Some(_) => {
                return Err(invalid(format!(
                    "method `{method_name}` params must map names to types"
                )))
            }
        };
        let result = match method.get("result") {
            None => Type::Null,
            Some(Value::String(result)) => parse_type(result, true)?,
            Some(_) => {
                return Err(invalid(format!(
                    "method `{method_name}` result must be a type"
                )))
            }
        };
        methods.push(Method {
            name: method_name,
            params,
            result,
        });
    }

    let service = Service {
        name,
        version,
        types,
        methods,
    };
    check_names(&service)?;
    Ok(service)
}

fn parse_fields(fields: &Map<String, Value>, owner: &str) -> Result<Vec<Field>, IdlError> {
    fields
        .iter()
        .map(|(name, ty)| {
            let ty = ty
                .as_str()
                .ok_or_else(|| invalid(format!("{owner}: `{name}` must name a type")))?;
            Ok(Field {
                name: identifier(name, "field")?,
                ty: parse_type(ty, false)?,
            })
        })
        .collect()
}

fn parse_type(text: &str, result: bool) -> Result<Type, IdlError> {
    let text = text.trim();
    if let Some(inner) = text.strip_suffix('?') {
        return Ok(Type::Optional(Box::new(parse_type(inner, false)?)));
    }
    if let Some(inner) = text.strip_suffix("[]") {
        return Ok(Type::List(Box::new(parse_type(inner, false)?)));
    }
    Ok(match text {
        "bool" => Type::Bool,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "f64" => Type::F64,
        "string" => Type::String,
        "json" => Type::Json,
        "null" if result => Type::Null,
        "null" => return Err(invalid("`null` is only a result type")),
        named => Type::Named(identifier(named, "type")?),
    })
}

/// Every named type is declared, and no name is used twice.
fn check_names(service: &Service) -> Result<(), IdlError> {
    let declared = |name: &str| service.types.iter().any(|ty| ty.name == name);
    fn named(ty: &Type) -> Option<&str> {
        match ty {
            Type::Named(name) => Some(name),
            Type::List(inner) | Type::Optional(inner) => named(inner),
            _ => None,
        }
    }
    let fields = service
        .types
        .iter()
        .flat_map(|ty| ty.fields.iter().map(|field| &field.ty))
        .chain(service.methods.iter().flat_map(|method| {
            method
                .params
                .iter()
                .map(|field| &field.ty)
                .chain(std::iter::once(&method.result))
        }));
    for ty in fields {
        if let Some(name) = named(ty).filter(|name| !declared(name)) {
            return Err(invalid(format!(
                "type `{name}` is not declared under `types`"
            )));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for method in &service.methods {
        if !seen.insert(&method.name) {
            return Err(invalid(format!(
                "method `{}` is declared twice",
                method.name
            )));
        }
    }
    Ok(())
}
