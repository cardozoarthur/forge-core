use crate::mcp::{call_mcp_tool, mcp_tools_manifest, McpToolSpec};
use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

pub fn serve_stdio(store: &ForgeStore) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve_stdio_with_io(store, stdin.lock(), stdout.lock())
}

pub fn serve_stdio_with_io<R: BufRead, W: Write>(
    store: &ForgeStore,
    mut reader: R,
    mut writer: W,
) -> Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .context("failed to read MCP stdio request")?;
        if bytes == 0 {
            break;
        }
        if bytes > MAX_MESSAGE_BYTES {
            write_response(
                &mut writer,
                &jsonrpc_error(Value::Null, -32600, "MCP request exceeds 8 MiB"),
            )?;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Value>(trimmed) {
            Ok(Value::Array(requests)) => {
                if requests.is_empty() {
                    Some(jsonrpc_error(
                        Value::Null,
                        -32600,
                        "JSON-RPC batch must not be empty",
                    ))
                } else {
                    let responses = requests
                        .into_iter()
                        .filter_map(|request| handle_request(store, request))
                        .collect::<Vec<_>>();
                    (!responses.is_empty()).then_some(Value::Array(responses))
                }
            }
            Ok(request) => handle_request(store, request),
            Err(_) => Some(jsonrpc_error(Value::Null, -32700, "Invalid JSON")),
        };
        if let Some(response) = response {
            write_response(&mut writer, &response)?;
        }
    }
    Ok(())
}

fn handle_request(store: &ForgeStore, request: Value) -> Option<Value> {
    let object = match request.as_object() {
        Some(object) => object,
        None => {
            return Some(jsonrpc_error(
                Value::Null,
                -32600,
                "JSON-RPC request must be an object",
            ));
        }
    };
    let id = object.get("id").cloned();
    if object.get("jsonrpc").and_then(Value::as_str) != Some(JSONRPC_VERSION) {
        return id.map(|id| jsonrpc_error(id, -32600, "jsonrpc must be 2.0"));
    }
    let method = match object.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => return id.map(|id| jsonrpc_error(id, -32600, "method is required")),
    };
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

    if method.starts_with("notifications/") {
        return None;
    }
    let id = id?;
    let response = match method {
        "initialize" => jsonrpc_result(
            id,
            json!({
                "protocolVersion": negotiated_protocol_version(&params),
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "forge-core",
                    "title": "Forge Core",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "instructions": "Forge is the workflow authority. Mutating tools retain Forge approval, policy, revision and validation gates."
            }),
        ),
        "ping" => jsonrpc_result(id, json!({})),
        "tools/list" => {
            let manifest = mcp_tools_manifest();
            let tools = manifest.tools.iter().map(protocol_tool).collect::<Vec<_>>();
            jsonrpc_result(id, json!({ "tools": tools }))
        }
        "tools/call" => match parse_tool_call(&params) {
            Ok((name, arguments)) => match call_mcp_tool(store, name, arguments) {
                Ok(report) => jsonrpc_result(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&report.result)
                                .unwrap_or_else(|_| "{}".to_owned())
                        }],
                        "structuredContent": report.result,
                        "isError": report.status != "ok" && report.status != "ready"
                    }),
                ),
                Err(error) => jsonrpc_result(
                    id,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": error.to_string()
                        }],
                        "isError": true
                    }),
                ),
            },
            Err(message) => jsonrpc_error(id, -32602, message),
        },
        _ => jsonrpc_error(id, -32601, "Method not found"),
    };
    Some(response)
}

fn negotiated_protocol_version(params: &Value) -> String {
    params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|version| matches!(*version, "2025-06-18" | "2025-03-26" | "2024-11-05"))
        .unwrap_or(MCP_PROTOCOL_VERSION)
        .to_owned()
}

fn protocol_tool(tool: &McpToolSpec) -> Value {
    json!({
        "name": tool.name,
        "title": tool.title,
        "description": tool.description,
        "inputSchema": tool.input_schema,
        "outputSchema": {
            "type": "object",
            "description": format!("Forge output contract {}", tool.output_schema),
            "x-forge-schema-version": tool.output_schema,
            "additionalProperties": true
        },
        "annotations": {
            "readOnlyHint": !tool.mutates_workflow,
            "destructiveHint": tool.mutates_workflow,
            "idempotentHint": !tool.mutates_workflow,
            "openWorldHint": false
        },
        "_meta": {
            "forge/asyncSafe": tool.async_safe,
            "forge/command": tool.forge_command
        }
    })
}

fn parse_tool_call(params: &Value) -> std::result::Result<(&str, Value), &'static str> {
    let object = params
        .as_object()
        .ok_or("tools/call params must be an object")?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or("tools/call requires a non-empty name")?;
    let arguments = object
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err("tools/call arguments must be an object");
    }
    Ok((name, arguments))
}

fn jsonrpc_result(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result
    })
}

fn jsonrpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn write_response(writer: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, response).context("failed to serialize MCP response")?;
    writer
        .write_all(b"\n")
        .context("failed to terminate MCP response")?;
    writer.flush().context("failed to flush MCP response")
}
