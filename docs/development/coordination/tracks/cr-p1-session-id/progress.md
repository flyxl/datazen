# cr-p1-session-id — 进度

**轨 ID：** cr-p1-session-id | **分支：** feature/cr-p1-session-id | **状态：** 测试未通过（见 bugs.md）

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

## 独立测试验收（390d4efc）

**环境：** worktree `datazen-cr-p1-session-id`；测试前 `node scripts/resolve-drivers.mjs --drivers=basic` + `CI=true pnpm install`（补齐 capabilities / builtinLocales）。

### Plan 对照

| # | 标准（code-review-remediation-plan § cr-p1-session-id） | 结果 | 证据 |
|---|------|------|------|
| 1 | GUI IPC（schema/query/export）强制 `db_session_id`，移除 connection_id 静默回退 | ✅ | `commands/` 无 `resolve_session`；`schema`/`export`/`driver_command`/`query` 均 `get_session`；3 个新增拒绝测试通过 |
| 2 | MCP/db_tools 保留 `resolve_session_for_mcp`，与 GUI 分离 | ✅ | `db_tools.rs`、`workflow/command_runtime.rs`、`workflow/executor.rs` 仍调用 `resolve_session_for_mcp` |
| 3 | `WorkflowChatPanel` 从 live session 解析 `dbSessionId` | ✅ | Vitest 6/6；源码读 `activeConnectionStore` |
| 4 | 更新 `naming.md` 迁移说明 | ✅ | §4 P1 迁移摘要与双模边界表 |

### 逻辑抽查

- **GUI 误传 connectionId：** `get_databases` / `execute_driver_command` / `get_connection_commands` 对仅落盘、无 live session 的 id 返回含 `DB session` 的错误。
- **插件桥：** `handleCommandInvoke` 查 `activeConnectionStore`；未连接 → bridge 拒绝（实现正确；单测未同步，见 BUG-001）。
- **Workflow 编辑器：** 无 live session 时 `WorkflowForm` 回退 `getDriverCommands(driverType)`。

### 测试结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib`（`CARGO_TARGET_DIR=.../target`） | **1173 passed; 1 failed** | 失败：`connect_dedicated_opens_separate_session_from_reuse`（BUG-002） |
| `cargo test -p datazen --lib rejects_connection_id` | **3 passed** | 本轨新增回归 |
| `vitest WorkflowChatPanel.test.tsx` | **6 passed** | |
| `vitest extensionBridge*.test.ts` | **43 passed; 6 failed** | BUG-001 |

### Bug

见 [bugs.md](./bugs.md)：BUG-001（extensionBridge 单测）、BUG-002（MockDriver dedicated 会话 id）。

## Commit

编码：`390d4efc` | 测试：`1a10bcfb`
