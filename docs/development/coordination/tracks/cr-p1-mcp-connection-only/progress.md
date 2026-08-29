# cr-p1-mcp-connection-only — 进度

**轨 ID：** cr-p1-mcp-connection-only | **分支：** feature/cr-p1-mcp-connection-only | **状态：** 已完成

## 范围

MCP / Workflow / db_tools 入参 **仅 connectionId**；内部经 `get_or_connect_session` 转 dbSessionId 并复用已有 session。

## 验收

- [x] `resolve_session_for_connection(connection_id)` 替代双模 API
- [x] 误传 dbSessionId → `ConnectionConfigNotFound`
- [x] `get_or_connect_session` 复用已有 session
- [x] cargo 1175/1175

## Commit

编码+测试：`cc0cd05f`
