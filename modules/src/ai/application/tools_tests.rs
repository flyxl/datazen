//! AI 工具面分发的行为钉子（2026-10-31 自桌面 `commands/ai/tests.rs` 归位：
//! `is_db_tool`/`classify_tool`/`db_tool_definitions` 一直在 modules，
//! 测试却住在宿主命令层——单元测试跟着逻辑走）。

use super::classify_tool;
use super::db_tool_definitions;
use super::is_db_tool;
use super::ToolKind;

#[test]
fn test_is_db_tool() {
    assert!(is_db_tool("list_connections"));
    assert!(is_db_tool("list_databases"));
    assert!(is_db_tool("list_tables"));
    assert!(is_db_tool("get_table_schema"));
    assert!(!is_db_tool("ask_questions"));
    assert!(!is_db_tool("unknown_tool"));
    assert!(!is_db_tool(""));
}

#[test]
fn test_classify_tool() {
    assert_eq!(classify_tool("ask_questions"), ToolKind::AskQuestions);
    assert_eq!(
        classify_tool("list_connections"),
        ToolKind::Db("list_connections".into())
    );
    assert_eq!(
        classify_tool("mcp/files/read_file"),
        ToolKind::Mcp {
            server_id: "files".into(),
            tool_name: "read_file".into(),
        }
    );
    assert_eq!(
        classify_tool("mcp/my-server/my_tool_name"),
        ToolKind::Mcp {
            server_id: "my-server".into(),
            tool_name: "my_tool_name".into(),
        }
    );
    assert_eq!(classify_tool("mcp/bad"), ToolKind::Unknown);
    assert_eq!(classify_tool("mcp//tool"), ToolKind::Unknown);
    assert_eq!(classify_tool("unknown_tool"), ToolKind::Unknown);
}

#[test]
fn test_db_tool_definitions_count() {
    let tools = db_tool_definitions();
    assert_eq!(tools.len(), 5);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"list_connections"));
    assert!(names.contains(&"list_databases"));
    assert!(names.contains(&"list_tables"));
    assert!(names.contains(&"get_table_schema"));
}

#[test]
fn test_db_tool_definitions_have_valid_schemas() {
    for tool in db_tool_definitions() {
        assert!(!tool.name.is_empty());
        assert!(!tool.description.is_empty());
        assert!(tool.parameters.is_object());
        let obj = tool.parameters.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    }
}
