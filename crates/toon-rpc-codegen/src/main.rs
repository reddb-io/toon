use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: toon-rpc-gen <file.toonrpc> [--rust] [--ts]");
        std::process::exit(1);
    }

    let file = &args[1];
    let path = Path::new(file);

    if !path.exists() {
        eprintln!("File not found: {}", file);
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(path).expect("Failed to read file");
    let spec: serde_json::Value = serde_json::from_str(&content).expect("Failed to parse TOON");

    let rust_code = generate_rust(&spec);
    let ts_code = generate_typescript(&spec);

    println!("// === Rust ===\n{}", rust_code);
    println!("// === TypeScript ===\n{}", ts_code);
}

fn generate_rust(spec: &serde_json::Value) -> String {
    let service_name = spec["service"].as_str().unwrap_or("Service");
    let methods = spec["methods"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);

    let mut output = String::new();

    output.push_str("use toon_rpc::{RpcContext, RpcResult};\n\n");
    output.push_str(&format!("pub trait {} {{\n", service_name));

    for method in methods {
        let name = method["name"].as_str().unwrap_or("unnamed");
        let params = method["params"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
        let result = method["result"].as_str().unwrap_or("()");

        let params_str: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let type_name = p.as_str().unwrap_or("()");
                format!("arg{}: {}", i, type_name)
            })
            .collect();

        output.push_str(&format!(
            "    fn {}(&self, ctx: &RpcContext, {}) -> RpcResult<{}>;\n",
            name,
            params_str.join(", "),
            result
        ));
    }

    output.push_str("}\n");

    if let Some(types_obj) = spec["types"].as_object() {
        if !types_obj.is_empty() {
            output.push_str("\n#[derive(Debug, Clone, Serialize, Deserialize)]\n");
            output.push_str("pub struct Types {\n");
            for (name, def) in types_obj {
                output.push_str(&format!("    pub {}: {},\n", name, def));
            }
            output.push_str("}\n");
        }
    }

    output
}

fn generate_typescript(spec: &serde_json::Value) -> String {
    let service_name = spec["service"].as_str().unwrap_or("Service");
    let methods = spec["methods"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);

    let mut output = String::new();

    output.push_str("import { ToonRpcTransport } from 'toon-rpc';\n\n");

    if let Some(types_obj) = spec["types"].as_object() {
        for (name, def) in types_obj {
            output.push_str(&format!("export interface {} {{\n", name));
            if let Some(obj) = def.as_object() {
                for (field, type_) in obj {
                    output.push_str(&format!("  {}: {};\n", field, ts_type(type_)));
                }
            }
            output.push_str("}\n\n");
        }
    }

    output.push_str(&format!(
        "export class {}Client {{\n",
        service_name
    ));
    output.push_str("  constructor(private transport: ToonRpcTransport) {}\n\n");

    for method in methods {
        let name = method["name"].as_str().unwrap_or("unnamed");
        let params = method["params"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
        let result = method["result"].as_str().unwrap_or("void");

        let params_str: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let type_name = p.as_str().unwrap_or("any");
                format!("arg{}: {}", i, ts_type_str(type_name))
            })
            .collect();

        output.push_str(&format!(
            "  async {}({}): Promise<{}> {{\n",
            name,
            params_str.join(", "),
            ts_type_str(result)
        ));
        output.push_str(&format!(
            "    return this.transport.call('{}', [{}]);\n",
            name,
            (0..params.len()).map(|i| format!("arg{}", i)).collect::<Vec<_>>().join(", ")
        ));
        output.push_str("  }\n\n");
    }

    output.push_str("}\n");
    output
}

fn ts_type(v: &serde_json::Value) -> String {
    ts_type_str(&v.to_string())
}

fn ts_type_str(t: &str) -> String {
    match t {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" => "number".to_string(),
        "bool" => "boolean".to_string(),
        "string" => "string".to_string(),
        "bytes" => "Uint8Array".to_string(),
        "null" => "null".to_string(),
        _ => t.to_string(),
    }
}
