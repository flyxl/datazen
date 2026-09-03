//! MCP external contract snapshot and golden regression test.
//!
//! Guards tool names, resource URIs, and tool input property keys against
//! accidental breaking changes. See `docs/development/external-contract-policy.md`.

use super::server::MCP_ALL_TOOLS;
use super::tool_help;
use serde_json::{json, Value};

const GOLDEN: &str = include_str!("fixtures/mcp_external_contract.json");

/// Build a deterministic JSON snapshot of the MCP surfaces external clients rely on.
pub(crate) fn build_mcp_contract_snapshot() -> Value {
    let mut tools: Vec<Value> = MCP_ALL_TOOLS
        .iter()
        .map(|name| {
            let mut entry = json!({ "name": name });
            if let Some(schema_str) = tool_help::input_schema_json(name) {
                let schema: Value =
                    serde_json::from_str(&schema_str).expect("tool input schema must be JSON");
                let properties = schema
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|obj| {
                        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
                        keys.sort_unstable();
                        keys
                    })
                    .unwrap_or_default();
                let required = schema
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        let mut keys: Vec<&str> = arr
                            .iter()
                            .filter_map(|v| v.as_str())
                            .collect();
                        keys.sort_unstable();
                        keys
                    })
                    .unwrap_or_default();
                entry["inputProperties"] = json!(properties);
                entry["requiredInputProperties"] = json!(required);
            }
            entry
        })
        .collect();
    tools.sort_by(|a, b| {
        a.get("name")
            .and_then(|n| n.as_str())
            .cmp(&b.get("name").and_then(|n| n.as_str()))
    });

    json!({
        "contractVersion": 1,
        "serverName": "datazen",
        "tools": tools,
        "resourceUris": [
            "datazen://connections",
            "datazen://query-history",
            "datazen://workflows"
        ],
        "resourceUriTemplates": [
            "datazen://schema/{connectionId}/{database}"
        ],
        "removedToolInputFields": [
            "config_id"
        ],
        "naming": {
            "toolInputKeys": "snake_case",
            "resourceJsonFields": "camelCase"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_external_contract_matches_golden() {
        let snapshot = build_mcp_contract_snapshot();
        let golden: Value =
            serde_json::from_str(GOLDEN).expect("golden MCP contract JSON must parse");
        assert_eq!(
            snapshot, golden,
            "MCP external contract changed.\n\
             If intentional, update src-tauri/src/mcp/fixtures/mcp_external_contract.json \
             and docs/development/external-contract-policy.md in the same PR.\n\
             Actual:\n{}\nExpected:\n{}",
            serde_json::to_string_pretty(&snapshot).unwrap(),
            serde_json::to_string_pretty(&golden).unwrap()
        );
    }

    /// Regenerate golden: `cargo test -p datazen --lib dump_mcp_contract_snapshot -- --ignored --nocapture`
    #[test]
    #[ignore = "manual: prints golden JSON for fixtures/mcp_external_contract.json"]
    fn dump_mcp_contract_snapshot() {
        println!(
            "{}",
            serde_json::to_string_pretty(&build_mcp_contract_snapshot()).unwrap()
        );
    }

    #[test]
    fn golden_tool_names_match_mcp_all_tools_constant() {
        let golden: Value = serde_json::from_str(GOLDEN).unwrap();
        let golden_names: Vec<&str> = golden["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().expect("tool name"))
            .collect();
        let mut expected: Vec<&str> = MCP_ALL_TOOLS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            golden_names,
            expected,
            "golden fixture tool names must stay aligned with MCP_ALL_TOOLS"
        );
    }
}
