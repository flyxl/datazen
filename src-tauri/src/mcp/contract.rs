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
                        let mut keys: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
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
            snapshot,
            golden,
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
            golden_names, expected,
            "golden fixture tool names must stay aligned with MCP_ALL_TOOLS"
        );
    }

    /// [tester] Every MCP tool and its input property keys must appear in the golden fixture.
    #[test]
    fn test_tester_golden_fixture_covers_all_tools_and_input_keys() {
        assert_eq!(
            MCP_ALL_TOOLS.len(),
            10,
            "contract tests assume exactly 10 MCP tools"
        );

        let snapshot = build_mcp_contract_snapshot();
        let golden: Value = serde_json::from_str(GOLDEN).unwrap();
        let golden_tools = golden["tools"]
            .as_array()
            .expect("golden must have tools array");

        assert_eq!(
            golden_tools.len(),
            MCP_ALL_TOOLS.len(),
            "golden fixture must list every MCP tool"
        );

        for name in MCP_ALL_TOOLS {
            let snapshot_tool = snapshot["tools"]
                .as_array()
                .expect("snapshot tools")
                .iter()
                .find(|t| t["name"].as_str() == Some(name))
                .unwrap_or_else(|| panic!("snapshot missing tool {name}"));
            let golden_tool = golden_tools
                .iter()
                .find(|t| t["name"].as_str() == Some(name))
                .unwrap_or_else(|| panic!("golden fixture missing tool {name}"));

            assert_eq!(
                golden_tool.get("inputProperties"),
                snapshot_tool.get("inputProperties"),
                "golden inputProperties drift for {name}"
            );
            assert_eq!(
                golden_tool.get("requiredInputProperties"),
                snapshot_tool.get("requiredInputProperties"),
                "golden requiredInputProperties drift for {name}"
            );
        }
    }

    /// [tester] Renaming a tool in the golden fixture must fail the contract comparison.
    #[test]
    fn test_tester_contract_break_detection_tool_rename() {
        let snapshot = build_mcp_contract_snapshot();
        let mut golden: Value = serde_json::from_str(GOLDEN).unwrap();
        golden["tools"]
            .as_array_mut()
            .expect("tools array")
            .iter_mut()
            .find(|t| t["name"] == "query")
            .expect("query tool in golden")["name"] = json!("execute_sql");

        assert_ne!(
            snapshot, golden,
            "golden test must detect accidental MCP tool renames"
        );
    }

    /// [tester] Removing a required input key from the golden fixture must fail comparison.
    #[test]
    fn test_tester_contract_break_detection_input_key_removal() {
        let snapshot = build_mcp_contract_snapshot();
        let mut golden: Value = serde_json::from_str(GOLDEN).unwrap();
        let query = golden["tools"]
            .as_array_mut()
            .expect("tools array")
            .iter_mut()
            .find(|t| t["name"] == "query")
            .expect("query tool in golden");
        query["requiredInputProperties"] = json!(["connection_id"]);

        assert_ne!(
            snapshot, golden,
            "golden test must detect removal of required MCP input keys"
        );
    }

    /// [tester] Resource URI surfaces are stable and covered by the golden snapshot.
    #[test]
    fn test_tester_resource_uri_contract_snapshot() {
        const FIXED_URIS: &[&str] = &[
            "datazen://connections",
            "datazen://query-history",
            "datazen://workflows",
        ];
        const URI_TEMPLATE: &str = "datazen://schema/{connectionId}/{database}";

        let snapshot = build_mcp_contract_snapshot();
        let golden: Value = serde_json::from_str(GOLDEN).unwrap();

        let snapshot_uris: Vec<&str> = snapshot["resourceUris"]
            .as_array()
            .expect("snapshot resourceUris")
            .iter()
            .map(|v| v.as_str().expect("resource URI string"))
            .collect();
        let golden_uris: Vec<&str> = golden["resourceUris"]
            .as_array()
            .expect("golden resourceUris")
            .iter()
            .map(|v| v.as_str().expect("resource URI string"))
            .collect();

        assert_eq!(snapshot_uris, FIXED_URIS);
        assert_eq!(golden_uris, FIXED_URIS);
        assert_eq!(
            snapshot["resourceUriTemplates"][0].as_str(),
            Some(URI_TEMPLATE)
        );
        assert_eq!(
            golden["resourceUriTemplates"][0].as_str(),
            Some(URI_TEMPLATE)
        );
        assert_eq!(snapshot["resourceUris"], golden["resourceUris"]);
        assert_eq!(
            snapshot["resourceUriTemplates"],
            golden["resourceUriTemplates"]
        );
    }
}
