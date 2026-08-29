# cr-p1-session-id — 进度

**轨 ID：** cr-p1-session-id | **分支：** feature/cr-p1-session-id | **状态：** 编码完成，待独立测试

## 范围
GUI IPC 强制 db_session_id；WorkflowChatPanel 修正；MCP/db_tools 保留显式兼容 API（`resolve_session_for_mcp`）。

## 变更摘要

| 区域 | 变更 |
|------|------|
| `connection_manager.rs` | `resolve_session` → `resolve_session_for_mcp`（MCP/Workflow 专用） |
| GUI IPC | `schema.rs` / `export.rs` / `driver_command.rs` 改 `get_session`，误传 connectionId 明确报错 |
| `db_tools.rs` | 继续双模，调用 `resolve_session_for_mcp` |
| `WorkflowChatPanel.tsx` | 从 `activeConnectionStore` 解析 live `dbSessionId` |
| `extensionBridge.ts` | 无活动会话拒绝，不再回退 connectionId |
| `WorkflowForm.tsx` | 未连接时 `getDriverCommands(driverType)` 回退 |
| `naming.md` | P1 迁移说明与双模边界更新 |

## 测试

```bash
CARGO_TARGET_DIR=.../datazen-cr-p1-session-id/target cargo test -p datazen --lib
npx vitest run src/components/ai/__tests__/WorkflowChatPanel.test.tsx
```

新增 Rust 测试：
- `get_databases_rejects_connection_id_without_live_session`
- `rejects_connection_id_on_execute_without_live_session`
- `rejects_connection_id_on_get_connection_commands_without_live_session`

## E2E / 设计决策

- Workflow runtime 仍用 `resolve_session_for_mcp`（YAML `connection` 字段为配置 id）
- 插件桥要求宿主侧已建立会话；插件需先 `connections.get` 确认连接存在且用户已在侧边栏连接
