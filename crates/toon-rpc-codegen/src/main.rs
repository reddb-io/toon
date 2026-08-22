use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: toon-rpc-gen <file.toonrpc> [--rust] [--ts] [--both]");
        std::process::exit(1);
    }

    let file = &args[1];
    let path = Path::new(file);

    if !path.exists() {
        eprintln!("File not found: {}", file);
        std::process::exit(1);
    }

    let lang = if args.len() >= 3 {
        &args[2]
    } else {
        "--both"
    };

    let content = std::fs::read_to_string(path).expect("Failed to read file");

    // Parse the .toonrpc IDL file using TOON
    let service = match idl::parse(&content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("IDL parse error: {}", e);
            std::process::exit(1);
        }
    };

    match lang {
        "--rust" => {
            println!("{}", rust::generate(&service));
        }
        "--ts" => {
            println!("{}", ts::generate(&service));
        }
        "--both" => {
            println!("// === Rust ===\n{}", rust::generate(&service));
            println!("\n// === TypeScript ===\n{}", ts::generate(&service));
        }
        _ => {
            eprintln!("Unknown language: {}. Use --rust, --ts, or --both", lang);
            std::process::exit(1);
        }
    }
}

mod idl {
    use std::collections::HashMap;
    use thiserror::Error;

    #[derive(Debug, Clone)]
    pub struct Service {
        pub name: String,
        pub version: String,
        pub types: HashMap<String, TypeDef>,
        pub methods: Vec<Method>,
        pub events: Vec<Event>,
    }

    #[derive(Debug, Clone)]
    pub struct TypeDef {
        pub name: String,
        pub fields: Vec<(String, String)>,
    }

    #[derive(Debug, Clone)]
    pub struct Method {
        pub name: String,
        pub params: Vec<(String, String)>,
        pub result: String,
    }

    #[derive(Debug, Clone)]
    pub struct Event {
        pub name: String,
        pub payload: String,
    }

    #[derive(Error, Debug)]
    pub enum ParseError {
        #[error("TOON parse error: {0}")]
        Toon(String),
        #[error("missing required field: {0}")]
        MissingField(&'static str),
        #[error("invalid format: {0}")]
        InvalidFormat(String),
    }

    pub fn parse(content: &str) -> Result<Service, ParseError> {
        // First parse the TOON content to get a JSON value
        let json_value = reddb_io_toon::decode(content)
            .map_err(|e| ParseError::Toon(e.message().to_string()))?
            .to_json_value();

        let json_obj = json_value
            .as_object()
            .ok_or_else(|| ParseError::InvalidFormat("expected top-level object".to_string()))?;

        let name = json_obj
            .get("service")
            .and_then(|v| v.as_str())
            .ok_or(ParseError::MissingField("service"))?
            .to_string();

        let version = json_obj
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0")
            .to_string();

        let types = parse_types(json_obj.get("types"))?;
        let methods = parse_methods(json_obj.get("methods"))?;
        let events = parse_events(json_obj.get("events"))?;

        Ok(Service {
            name,
            version,
            types,
            methods,
            events,
        })
    }

    fn parse_types(v: Option<&serde_json::Value>) -> Result<HashMap<String, TypeDef>, ParseError> {
        let mut types = HashMap::new();
        if let Some(obj) = v.and_then(|v| v.as_object()) {
            for (name, def) in obj {
                let fields = if let Some(fields_obj) = def.as_object() {
                    fields_obj
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_string().trim_matches('"').to_string()))
                        .collect()
                } else {
                    Vec::new()
                };
                types.insert(
                    name.clone(),
                    TypeDef {
                        name: name.clone(),
                        fields,
                    },
                );
            }
        }
        Ok(types)
    }

    fn parse_methods(v: Option<&serde_json::Value>) -> Result<Vec<Method>, ParseError> {
        let mut methods = Vec::new();
        if let Some(arr) = v.and_then(|v| v.as_array()) {
            for m in arr {
                let name = m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or(ParseError::MissingField("method.name"))?
                    .to_string();

                let params = if let Some(params_arr) = m.get("params").and_then(|v| v.as_array()) {
                    params_arr
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let pname = p
                                .as_object()
                                .and_then(|o| o.keys().next().cloned())
                                .unwrap_or_else(|| format!("arg{}", i));
                            let ptype = p
                                .as_object()
                                .and_then(|o| o.values().next())
                                .map(|v| v.to_string().trim_matches('"').to_string())
                                .unwrap_or_else(|| "()".to_string());
                            (pname, ptype)
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                let result = m
                    .get("result")
                    .map(|v| v.to_string().trim_matches('"').to_string())
                    .unwrap_or_else(|| "()".to_string());

                methods.push(Method {
                    name,
                    params,
                    result,
                });
            }
        }
        Ok(methods)
    }

    fn parse_events(v: Option<&serde_json::Value>) -> Result<Vec<Event>, ParseError> {
        let mut events = Vec::new();
        if let Some(arr) = v.and_then(|v| v.as_array()) {
            for e in arr {
                let name = e
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("event")
                    .to_string();
                let payload = e
                    .get("payload")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                events.push(Event { name, payload });
            }
        }
        Ok(events)
    }
}

mod rust {
    use super::idl::{Method, Service, TypeDef};

    pub fn generate(service: &Service) -> String {
        let mut out = String::new();
        out.push_str("//! Auto-generated by toon-rpc-gen. Do not edit.\n\n");
        out.push_str("use serde::{Deserialize, Serialize};\n");
        out.push_str("use toon_rpc::{RpcContext, RpcResult, Dispatcher};\n\n");

        // Generate types
        for typedef in service.types.values() {
            out.push_str(&generate_type_struct(typedef));
        }
        if !service.types.is_empty() {
            out.push('\n');
        }

        // Generate trait
        out.push_str(&format!("pub trait {} {{\n", service.name));
        for method in &service.methods {
            out.push_str(&generate_trait_method(method));
        }
        out.push_str("}\n\n");

        // Generate dispatcher builder
        out.push_str(&generate_dispatcher_builder(service));

        out
    }

    fn generate_type_struct(typedef: &TypeDef) -> String {
        let mut out = String::new();
        out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
        out.push_str(&format!("pub struct {} {{\n", typedef.name));
        for (name, type_) in &typedef.fields {
            out.push_str(&format!("    pub {}: {},\n", name, type_));
        }
        out.push_str("}\n");
        out
    }

    fn generate_trait_method(method: &Method) -> String {
        let params: Vec<String> = method
            .params
            .iter()
            .map(|(name, type_)| format!("{}: {}", name, type_))
            .collect();
        format!(
            "    fn {}(&self, ctx: &RpcContext, {}) -> RpcResult<{}>;\n",
            method.name,
            params.join(", "),
            method.result
        )
    }

    fn generate_dispatcher_builder(service: &Service) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "pub fn register_{}(dispatcher: &mut Dispatcher, service: impl {})\n",
            service.name.to_lowercase(),
            service.name
        ));
        out.push_str("{\n");
        for method in &service.methods {
            out.push_str(&format!(
                "    dispatcher.register(\"{}\", |params, id| {{\n",
                method.name
            ));
            out.push_str("        use serde_json::Value;\n");
            out.push_str(&generate_param_extraction(method));
            out.push_str(&format!(
                "        service.{}(ctx, {})\n",
                method.name,
                (0..method.params.len()).map(|i| format!("arg{}", i)).collect::<Vec<_>>().join(", ")
            ));
            out.push_str("    });\n");
        }
        out.push_str("}\n");
        out
    }

    fn generate_param_extraction(method: &Method) -> String {
        let mut out = String::new();
        out.push_str("        let ctx = toon_rpc::RpcContext::new();\n");
        for (i, (name, type_)) in method.params.iter().enumerate() {
            out.push_str(&format!(
                "        let arg{}: {} = /* TODO: extract from params */ todo!();\n",
                i, type_
            ));
            let _ = name;
        }
        out
    }
}

mod ts {
    use super::idl::{Method, Service, TypeDef};

    pub fn generate(service: &Service) -> String {
        let mut out = String::new();
        out.push_str("// Auto-generated by toon-rpc-gen. Do not edit.\n\n");
        out.push_str("import type { JsonValue } from '@reddb-io/toon';\n");
        out.push_str("import { Client, Server } from '@reddb-io/toon-rpc';\n\n");

        // Generate types
        for typedef in service.types.values() {
            out.push_str(&generate_type_interface(typedef));
        }
        if !service.types.is_empty() {
            out.push('\n');
        }

        // Generate client
        out.push_str(&generate_client(service));

        // Generate server helper
        out.push_str(&generate_server(service));

        out
    }

    fn generate_type_interface(typedef: &TypeDef) -> String {
        let mut out = String::new();
        out.push_str(&format!("export interface {} {{\n", typedef.name));
        for (name, type_) in &typedef.fields {
            out.push_str(&format!("  {}: {};\n", name, ts_type(type_)));
        }
        out.push_str("}\n");
        out
    }

    fn generate_client(service: &Service) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "export class {}Client {{\n",
            service.name
        ));
        out.push_str("  constructor(private client: Client) {}\n\n");
        for method in &service.methods {
            let params: Vec<String> = method
                .params
                .iter()
                .map(|(name, type_)| format!("{}: {}", name, ts_type(type_)))
                .collect();
            let call_params: Vec<String> = method.params.iter().map(|(name, _)| name.clone()).collect();

            out.push_str(&format!(
                "  async {}({}): Promise<{}> {{\n",
                method.name,
                params.join(", "),
                ts_type(&method.result)
            ));
            out.push_str(&format!(
                "    return this.client.call('{}', [{}]) as Promise<{}>;\n",
                method.name,
                call_params.join(", "),
                ts_type(&method.result)
            ));
            out.push_str("  }\n\n");
        }
        out.push_str("}\n\n");
        out
    }

    fn generate_server(service: &Service) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "export interface {}Impl {{\n",
            service.name
        ));
        for method in &service.methods {
            let params: Vec<String> = method
                .params
                .iter()
                .map(|(name, type_)| format!("{}: {}", name, ts_type(type_)))
                .collect();
            out.push_str(&format!(
                "  {}({}): {} | Promise<{}>;\n",
                method.name,
                params.join(", "),
                ts_type(&method.result),
                ts_type(&method.result)
            ));
        }
        out.push_str("}\n\n");

        out.push_str(&format!(
            "export function register{}(server: Server, impl: {}Impl) {{\n",
            service.name, service.name
        ));
        for method in &service.methods {
            let args: Vec<String> = method.params.iter().map(|(name, _)| name.clone()).collect();
            out.push_str(&format!(
                "  server.register('{}', async (params) => impl.{}({}));\n",
                method.name, method.name, args.join(", ")
            ));
        }
        out.push_str("}\n");
        out
    }

    fn ts_type(t: &str) -> String {
        match t {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" => {
                "number".to_string()
            }
            "bool" => "boolean".to_string(),
            "string" => "string".to_string(),
            "bytes" => "Uint8Array".to_string(),
            "null" => "null".to_string(),
            _ => t.to_string(),
        }
    }
}
