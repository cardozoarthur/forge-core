use foundry_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use foundry_core::storage::FoundryStore;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn mcp_manifest_exposes_bounded_executor_wave_without_auto_completion() {
    let tool = mcp_tools_manifest()
        .tools
        .into_iter()
        .find(|tool| tool.name == "foundry.run.execute_wave")
        .expect("executor wave must be exposed over MCP");

    assert!(tool.async_safe);
    assert!(tool.mutates_workflow);
    assert_eq!(tool.output_schema, "foundry.request_executor_wave.v1");
    assert!(tool
        .description
        .contains("Process receipts never complete or promote Foundry tasks"));
    assert!(tool
        .foundry_command
        .windows(2)
        .any(|window| window == ["request", "execute-wave"]));

    let required = tool.input_schema["required"]
        .as_array()
        .expect("tool schema should declare required authorization fields");
    for field in ["run_id", "allow_exec", "approved_by", "reason"] {
        assert!(
            required.iter().any(|required| required == field),
            "{field} must be required"
        );
    }
}

#[test]
fn mcp_executor_wave_fails_closed_before_loading_or_driving_a_run() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();

    let missing_allow_exec = call_mcp_tool(
        &store,
        "foundry.run.execute_wave",
        json!({
            "run_id": "missing-run",
            "approved_by": "mcp-wave-test",
            "reason": "prove explicit authorization"
        }),
    )
    .unwrap_err();
    assert!(format!("{missing_allow_exec:#}").contains("missing field `allow_exec`"));

    let denied = call_mcp_tool(
        &store,
        "foundry.run.execute_wave",
        json!({
            "run_id": "missing-run",
            "allow_exec": false,
            "approved_by": "mcp-wave-test",
            "reason": "prove explicit authorization"
        }),
    )
    .unwrap_err();
    assert!(format!("{denied:#}")
        .contains("request execute-wave requires explicit --allow-exec authorization"));

    let missing_approver = call_mcp_tool(
        &store,
        "foundry.run.execute_wave",
        json!({
            "run_id": "missing-run",
            "allow_exec": true,
            "approved_by": " ",
            "reason": "prove explicit authorization"
        }),
    )
    .unwrap_err();
    assert!(format!("{missing_approver:#}")
        .contains("request execute-wave requires a non-empty --approved-by value"));

    let missing_reason = call_mcp_tool(
        &store,
        "foundry.run.execute_wave",
        json!({
            "run_id": "missing-run",
            "allow_exec": true,
            "approved_by": "mcp-wave-test",
            "reason": " "
        }),
    )
    .unwrap_err();
    assert!(format!("{missing_reason:#}")
        .contains("request execute-wave requires a non-empty authorization reason"));
}
