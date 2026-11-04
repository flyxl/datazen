//! MCP 对外契约面（server web 形态）。
//!
//! 桌面形态从 `tool_help.rs`（帮助文案）+ `#[tool]` 宏派生的 Input
//! JsonSchema（`inputProperties`）合成快照；server 形态没有 rmcp/schemars
//! 依赖，同一契约面改为静态表承载——两表由
//! `tests/mcp_contract_parity.rs` 直接 include 桌面的
//! `fixtures/mcp_external_contract.json` golden 做逐字节对齐（验收标准）。
//! 任何一形态改契约必须同 PR 更新 golden 与本文档表。

use serde_json::{json, Value};

/// 与桌面 `mcp::server::MCP_ALL_TOOLS` 逐字一致（顺序含语义：注册序）。
pub const MCP_ALL_TOOLS: &[&str] = &[
    "list_connections",
    "list_databases",
    "list_tables",
    "search_tables",
    "query",
    "get_schema",
    "explain_query",
    "describe_table",
    "list_workflows",
    "run_workflow",
    // §4.2 stateful protocol: explicit open / use / close.
    "open_session",
    "use_session",
    "close_session",
];

/// MCP initialize 结果的 serverName（golden `"serverName": "datazen"`）。
pub const SERVER_NAME: &str = "datazen";

/// 与桌面 `tool_help::ToolHelp` 同形：静态帮助元数据。
/// `props`/`required` 与 golden 契约的 `inputProperties` /
/// `requiredInputProperties` 对齐（golden 内已排序，此处保持 golden 序）；
/// `prop_types` 给 `inputSchema` 的 JSON Schema 类型（snake_case 属性键）。
pub struct ToolHelp {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
    pub example: &'static str,
    pub props: &'static [(&'static str, &'static str)],
    pub required: &'static [&'static str],
}

/// 与桌面 `tool_help::TOOL_HELPS` 逐字对齐（文案不改一个字节）。
pub const TOOL_HELPS: &[ToolHelp] = &[
    ToolHelp {
        name: "list_connections",
        description: "List all configured database connections.",
        parameters: "(no parameters)",
        example: r#"{"name":"list_connections","arguments":{}}"#,
        props: &[],
        required: &[],
    },
    ToolHelp {
        name: "list_databases",
        description: "List all databases on a connected server.",
        parameters:
            "connection_id (string, required) — persistent connection id from list_connections",
        example: r#"{"name":"list_databases","arguments":{"connection_id":"<uuid>"}}"#,
        props: &[("connection_id", "string")],
        required: &["connection_id"],
    },
    ToolHelp {
        name: "list_tables",
        description: "List tables in a database with types and row counts.",
        parameters:
            "connection_id (string, required); database (string, optional) — defaults to connection default",
        example: r#"{"name":"list_tables","arguments":{"connection_id":"<uuid>","database":"app"}}"#,
        props: &[
            ("connection_id", "string"),
            ("database", "string"),
            ("schema", "string"),
        ],
        required: &["connection_id"],
    },
    ToolHelp {
        name: "search_tables",
        description: "Search tables by name pattern (case-insensitive substring).",
        parameters:
            "connection_id (string, required); pattern (string, required); database (string, optional); limit (number, optional, default 20)",
        example: r#"{"name":"search_tables","arguments":{"connection_id":"<uuid>","pattern":"user","limit":20}}"#,
        props: &[
            ("connection_id", "string"),
            ("pattern", "string"),
            ("database", "string"),
            ("schema", "string"),
            ("limit", "number"),
        ],
        required: &["connection_id", "pattern"],
    },
    ToolHelp {
        name: "query",
        description: "Execute SQL and return JSON rows.",
        parameters:
            "connection_id (string, required); sql (string, required); limit (number, optional, default 100, max 50000)",
        example: r#"{"name":"query","arguments":{"connection_id":"<uuid>","sql":"SELECT 1","limit":100}}"#,
        props: &[
            ("connection_id", "string"),
            ("sql", "string"),
            ("limit", "number"),
        ],
        required: &["connection_id", "sql"],
    },
    ToolHelp {
        name: "get_schema",
        description: "Get table schema: columns, PKs, FKs, indexes.",
        parameters: "connection_id (string, required); table (string, required)",
        example: r#"{"name":"get_schema","arguments":{"connection_id":"<uuid>","table":"users"}}"#,
        props: &[
            ("connection_id", "string"),
            ("database", "string"),
            ("schema", "string"),
            ("table", "string"),
        ],
        required: &["connection_id", "table"],
    },
    ToolHelp {
        name: "explain_query",
        description: "Return EXPLAIN plan for a SQL query.",
        parameters: "connection_id (string, required); sql (string, required)",
        example: r#"{"name":"explain_query","arguments":{"connection_id":"<uuid>","sql":"SELECT * FROM users"}}"#,
        props: &[("connection_id", "string"), ("sql", "string")],
        required: &["connection_id", "sql"],
    },
    ToolHelp {
        name: "describe_table",
        description: "Human-readable table description.",
        parameters: "connection_id (string, required); table (string, required)",
        example: r#"{"name":"describe_table","arguments":{"connection_id":"<uuid>","table":"users"}}"#,
        props: &[
            ("connection_id", "string"),
            ("database", "string"),
            ("schema", "string"),
            ("table", "string"),
        ],
        required: &["connection_id", "table"],
    },
    ToolHelp {
        name: "list_workflows",
        description: "List available user-defined workflows.",
        parameters: "(no parameters)",
        example: r#"{"name":"list_workflows","arguments":{}}"#,
        props: &[],
        required: &[],
    },
    ToolHelp {
        name: "run_workflow",
        description: "Execute a workflow by id (see list_workflows).",
        parameters:
            "workflow_id (string, required); variables (object, optional); connection_id (string, optional)",
        example: r#"{"name":"run_workflow","arguments":{"workflow_id":"my-flow","variables":{}}}"#,
        props: &[
            ("workflow_id", "string"),
            ("variables", "object"),
            ("connection_id", "string"),
        ],
        required: &["workflow_id"],
    },
    ToolHelp {
        name: "open_session",
        description: "Open a session held for this client and return an opaque client_session_ref. The session is released when this client disconnects or the ttl expires.",
        parameters:
            "connection_id (string, required); database (string, required); schema (string, optional); ttl_ms (number, optional, default 300000, max 3600000)",
        example: r#"{"name":"open_session","arguments":{"connection_id":"<uuid>","database":"app","ttl_ms":600000}}"#,
        props: &[
            ("connection_id", "string"),
            ("database", "string"),
            ("schema", "string"),
            ("ttl_ms", "number"),
        ],
        required: &["connection_id", "database"],
    },
    ToolHelp {
        name: "use_session",
        description: "Run one driver command inside a session this client opened, and return its rows.",
        parameters:
            "client_session_ref (string, required); command (string, required); input (object, required); timeout_ms (number, optional, default 60000, max 300000)",
        example: r#"{"name":"use_session","arguments":{"client_session_ref":"<opaque-ref>","command":"query","input":{"sql":"SELECT 1"}}}"#,
        props: &[
            ("client_session_ref", "string"),
            ("command", "string"),
            ("input", "object"),
            ("timeout_ms", "number"),
        ],
        required: &["client_session_ref", "command"],
    },
    ToolHelp {
        name: "close_session",
        description: "Close a session this client opened. Idempotent, and never touches another client resources.",
        parameters:
            "client_session_ref (string, required); mode (string, optional: requireNoTransaction default, rollbackAndClose)",
        example: r#"{"name":"close_session","arguments":{"client_session_ref":"<opaque-ref>","mode":"rollbackAndClose"}}"#,
        props: &[("client_session_ref", "string"), ("mode", "string")],
        required: &["client_session_ref"],
    },
];

/// Lookup static help for a tool name（与桌面 `tool_help::lookup` 同语义）。
pub fn lookup(tool_name: &str) -> Option<&'static ToolHelp> {
    TOOL_HELPS.iter().find(|h| h.name == tool_name)
}

/// 构造 `tools/list` 的单个工具定义（MCP `Tool` wire 形状）：
/// `inputSchema` 为 JSON Schema object，属性键 snake_case（golden naming）。
pub fn tool_definition(name: &str) -> Option<Value> {
    let help = lookup(name)?;
    let mut properties = serde_json::Map::new();
    for (key, ty) in help.props {
        properties.insert((*key).to_owned(), json!({ "type": ty }));
    }
    let mut schema = json!({
        "type": "object",
        "properties": Value::Object(properties),
    });
    if !help.required.is_empty() {
        schema["required"] = json!(help.required);
    }
    Some(json!({
        "name": name,
        "description": help.description,
        "inputSchema": schema,
    }))
}

/// 桌面形态 `build_mcp_contract_snapshot` 的 server 侧镜像（同形输出）：
/// golden 契约的快照源——`tests/mcp_contract_parity.rs` 用它与桌面
/// fixture 逐字节比对。
pub fn build_mcp_contract_snapshot() -> Value {
    let mut tools: Vec<Value> = MCP_ALL_TOOLS
        .iter()
        .map(|name| {
            let mut entry = json!({ "name": name });
            let help = lookup(name);
            if let Some(help) = help {
                if !help.props.is_empty() {
                    let mut keys: Vec<&str> = help.props.iter().map(|(k, _)| *k).collect();
                    keys.sort_unstable();
                    entry["inputProperties"] = json!(keys);
                }
                if !help.required.is_empty() {
                    let mut keys: Vec<&str> = help.required.to_vec();
                    keys.sort_unstable();
                    entry["requiredInputProperties"] = json!(keys);
                }
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
        "serverName": SERVER_NAME,
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
    fn mcp_all_tools_list_is_stable() {
        assert_eq!(MCP_ALL_TOOLS.len(), 13);
        assert!(MCP_ALL_TOOLS.contains(&"list_connections"));
        assert!(MCP_ALL_TOOLS.contains(&"open_session"));
        assert!(MCP_ALL_TOOLS.contains(&"close_session"));
    }

    #[test]
    fn every_registered_tool_has_static_help() {
        for name in MCP_ALL_TOOLS {
            assert!(lookup(name).is_some(), "missing ToolHelp for {name}");
        }
    }

    #[test]
    fn tool_definition_carries_snake_case_input_schema() {
        let def = tool_definition("open_session").expect("open_session definition");
        let props: Vec<String> = def["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(props, vec!["connection_id", "database", "schema", "ttl_ms"]);
        assert_eq!(
            def["inputSchema"]["required"],
            json!(["connection_id", "database"])
        );
    }

    #[test]
    fn snapshot_orders_tools_and_keys_deterministically() {
        let snapshot = build_mcp_contract_snapshot();
        let names: Vec<&str> = snapshot["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "snapshot tool list must be name-sorted");
    }
}
