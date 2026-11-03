//! WR-F2 验收：MCP golden 契约两形态逐字节一致。
//!
//! 直接 include 桌面形态的 golden fixture
//! `apps/desktop/src-tauri/src/mcp/fixtures/mcp_external_contract.json`（与
//! 桌面 `contract_tests.rs` 同一文件），server 形态
//! （`server/src/mcp/contract.rs`）的快照必须与其逐字节一致。
//!
//! 本测试经 `#[path]` 直接编译 `server/src/mcp` 模块（依赖 serde_json /
//! axum，不依赖 modules `mcp` feature），因此在主线点亮 feature 之前即可
//! 运行；主线接线清单见 WR-F2 报告。

#[path = "../src/mcp/mod.rs"]
mod mcp;

use serde_json::{json, Value};

const GOLDEN: &str =
    include_str!("../../apps/desktop/src-tauri/src/mcp/fixtures/mcp_external_contract.json");

fn golden() -> Value {
    serde_json::from_str(GOLDEN).expect("golden MCP contract JSON must parse")
}

/// 两形态快照逐字节一致（验收标准原文：两形态逐字节一致）。
#[test]
fn server_form_snapshot_is_byte_identical_to_desktop_golden() {
    let snapshot = mcp::contract::build_mcp_contract_snapshot();
    let serialized_snapshot =
        serde_json::to_string_pretty(&snapshot).expect("snapshot serializes");
    let serialized_golden = serde_json::to_string_pretty(&golden()).expect("golden serializes");
    assert_eq!(
        serialized_snapshot, serialized_golden,
        "MCP external contract changed between forms.\n\
         If intentional, update BOTH the desktop fixture \
         (apps/desktop/src-tauri/src/mcp/fixtures/mcp_external_contract.json) and \
         server/src/mcp/contract.rs in the same PR."
    );
}

/// 工具清单（13 个）与 golden 一致，且包含有状态三件套。
#[test]
fn golden_tool_names_match_server_tool_table() {
    let golden_names: Vec<&str> = golden()["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    let mut expected: Vec<&str> = mcp::contract::MCP_ALL_TOOLS.to_vec();
    expected.sort_unstable();
    assert_eq!(golden_names, expected);
    assert_eq!(expected.len(), 13);
    for name in ["open_session", "use_session", "close_session"] {
        assert!(expected.contains(&name), "stateful tool {name} missing");
    }
}

/// 每个工具的输入属性键/必填键与 golden 无漂移（从桌面
/// `contract_tests.rs` 学到的断言样式）。
#[test]
fn per_tool_input_property_keys_match_golden() {
    let golden_tools = golden()["tools"].as_array().expect("tools array");
    for name in mcp::contract::MCP_ALL_TOOLS {
        let help = mcp::contract::lookup(name).unwrap_or_else(|| panic!("help for {name}"));
        let golden_tool = golden_tools
            .iter()
            .find(|t| t["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("golden fixture missing tool {name}"));

        let mut props: Vec<&str> = help.props.iter().map(|(k, _)| *k).collect();
        props.sort_unstable();
        let golden_props: Vec<&str> = golden_tool
            .get("inputProperties")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(props, golden_props, "inputProperties drift for {name}");

        let mut required: Vec<&str> = help.required.to_vec();
        required.sort_unstable();
        let golden_required: Vec<&str> = golden_tool
            .get("requiredInputProperties")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(required, golden_required, "requiredInputProperties drift for {name}");
    }
}

/// 资源 URI 面两形态一致（fixed URIs + 模板）。
#[test]
fn resource_uri_surface_matches_golden() {
    let snapshot = mcp::contract::build_mcp_contract_snapshot();
    assert_eq!(snapshot["resourceUris"], golden()["resourceUris"]);
    assert_eq!(
        snapshot["resourceUriTemplates"],
        golden()["resourceUriTemplates"]
    );
    assert_eq!(snapshot["serverName"], json!("datazen"));
    assert_eq!(snapshot["removedToolInputFields"], json!(["config_id"]));
}

/// tools/list 的 wire 形状（server HTTP 形态的 `Tool` 对象）。
#[test]
fn list_tools_wire_shape_is_stable() {
    let result = mcp::wire::list_tools_result();
    let tools = result["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 13);
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name");
        assert!(
            mcp::contract::MCP_ALL_TOOLS.contains(&name),
            "unexpected tool {name} in tools/list"
        );
        // MCP `Tool` 三键：name / description / inputSchema（object）。
        assert!(tool["description"].is_string(), "{name} description");
        assert_eq!(tool["inputSchema"]["type"], json!("object"));
        // snake_case 属性键（golden naming 契约）。
        if let Some(props) = tool["inputSchema"]["properties"].as_object() {
            for key in props.keys() {
                assert!(
                    key.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "tool {name} input key {key} is not snake_case"
                );
            }
        }
    }
    assert!(result["nextCursor"].is_null(), "no cursor pagination");
}

/// initialize 结果的 wire 形状（serverInfo 与桌面 get_info 对齐）。
#[test]
fn initialize_wire_shape_is_stable() {
    let result = mcp::wire::initialize_result();
    assert_eq!(result["serverInfo"]["name"], json!("datazen"));
    assert!(result["serverInfo"]["version"].is_string());
    assert_eq!(result["protocolVersion"], json!(mcp::wire::PROTOCOL_VERSION));
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["instructions"].is_string());
}

/// 错误码映射与桌面 rmcp 语义一致（invalid_params=-32602、
/// internal_error=-32603、SessionLost 裸文案不附帮助块）。
#[test]
fn tool_error_wire_codes_match_desktop_semantics() {
    let with_help = mcp::wire::tool_call_error(mcp::wire::ToolCallError::WithHelp {
        tool: "query".into(),
        reason: "missing connection_id".into(),
    });
    assert_eq!(with_help["code"], json!(-32602));
    assert!(with_help["message"].as_str().unwrap().contains("Tool: query"));

    let plain = mcp::wire::tool_call_error(mcp::wire::ToolCallError::Plain(
        "SessionLost: MCP client disconnected".into(),
    ));
    assert_eq!(plain["code"], json!(-32602));
    assert_eq!(plain["message"], json!("SessionLost: MCP client disconnected"));

    let internal = mcp::wire::tool_call_error(mcp::wire::ToolCallError::Internal("x".into()));
    assert_eq!(internal["code"], json!(-32603));
}
