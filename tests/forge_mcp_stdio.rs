use assert_cmd::Command;
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn stdio_server_negotiates_and_lists_protocol_tools() {
    let directory = tempdir().expect("tempdir");
    let store = directory.path().join("forge.sqlite");
    let input = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "contract-test", "version": "1"}
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "ping",
            "params": {}
        }),
    ]
    .into_iter()
    .map(|request| request.to_string())
    .collect::<Vec<_>>()
    .join("\n");

    let output = Command::cargo_bin("forge")
        .expect("forge binary")
        .args([
            "--store",
            store.to_str().expect("store path"),
            "mcp",
            "serve",
        ])
        .write_stdin(format!("{input}\n"))
        .output()
        .expect("run MCP server");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let responses = String::from_utf8(output.stdout)
        .expect("utf8 responses")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC response"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "forge-core");
    let tools = responses[1]["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert!(!tools.is_empty());
    assert!(tools[0]["name"].is_string());
    assert!(tools[0]["inputSchema"].is_object());
    assert!(tools[0]["outputSchema"].is_object());
    assert_eq!(responses[2]["result"], json!({}));
}

#[test]
fn stdio_server_returns_json_rpc_errors_without_exiting() {
    let directory = tempdir().expect("tempdir");
    let store = directory.path().join("forge.sqlite");
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"missing\"}\n",
        "not-json\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n"
    );

    let output = Command::cargo_bin("forge")
        .expect("forge binary")
        .args([
            "--store",
            store.to_str().expect("store path"),
            "mcp",
            "serve",
        ])
        .write_stdin(input)
        .output()
        .expect("run MCP server");
    assert!(output.status.success());
    let responses = String::from_utf8(output.stdout)
        .expect("utf8 responses")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC response"))
        .collect::<Vec<_>>();
    assert_eq!(responses[0]["error"]["code"], -32601);
    assert_eq!(responses[1]["error"]["code"], -32700);
    assert_eq!(responses[2]["result"], json!({}));
}
