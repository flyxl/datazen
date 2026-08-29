# cr-p1-mcp-connection-only — 进度

**轨 ID：** cr-p1-mcp-connection-only | **分支：** feature/cr-p1-mcp-connection-only | **状态：** 完成

## 范围

MCP / Workflow / db_tools 入参明确为 **仅 connectionId**；内部转换为 dbSessionId；复用已有 session 由 ConnectionManager 内部按 connectionId 查 owner 映射，不再「先当 dbSessionId 试」。

## 验收

- [x] `resolve_session_for_mcp` 重命名为 `resolve_session_for_connection(connection_id: &str)`
- [x] 移除「id 可能是 dbSessionId」的 get_session 第一步；复用改为 connectionId → find existing session
- [x] db_tools / workflow / mcp 调用方与文档更新
- [x] 单测：同 connectionId 复用 session；误传 dbSessionId 明确报错
- [x] `cargo test -p datazen --lib` 全绿
