# 代码审查 Remediation 实施计划

> 来源：2026-08-29 全库审查（安全 / 冗余 / 架构耦合 / 模块体量）
> 协调分支：`feature/code-review-remediation`（基于 `main`）
> 公共进度：`docs/development/coordination/hub.md`

## 轨道一览

| 轨 ID | 优先级 | 功能 | 主要触碰面 | 并行波次 |
|-------|--------|------|-----------|----------|
| `cr-p0-mcp` | P0 | MCP 认证 + Resource/Tool 策略统一 | `src-tauri/src/mcp/*` | W1 |
| `cr-p0-ext-install` | P0 | 扩展安装路径仅允许原生对话框 | `commands/extensions.rs`, `src/commands/extensions.ts` | W1 |
| `cr-p1-session-id` | P1 | 退役 GUI 路径 `resolve_session` 双模 | `connection_manager.rs`, `driver_command.rs`, `schema.rs`, `WorkflowChatPanel.tsx` | W2 |
| `cr-p1-workflow-ui` | P1 | WorkflowPanel / WorkflowPage UI 统一 | `WorkflowPanel.tsx`, `WorkflowPage.tsx`, `WorkflowForm.tsx` | W2 |
| `cr-p1-split-ai` | P1 | 拆分 `commands/ai.rs` | `src-tauri/src/commands/ai/*` | W2 |
| `cr-p2-sync-pairing` | P2 | sync pairing 单一来源（IPC） | `syncPairing.ts`, `data_sync/pairing.rs` | W3 |
| `cr-p2-schema-cmd` | P2 | Schema 访问收敛 Driver Command | `commands/schema.rs`, driver commands | W3 |
| `cr-p2-navigator-split` | P2 | ConnectionNavigatorTree 拆分 + 共享分类 | `ConnectionNavigatorTree.tsx`, `UnifiedSchemaTree.tsx` | W3 |
| `cr-p3-dedup-dialogs` | P3 | Limitations / AdminCreate / toErrorMessage | `*LimitationsDialog*`, `Create*Dialog.tsx` | W4 |
| `cr-p3-secrets-hardening` | P3 | `.key` chmod + SQL 日志统一脱敏 | `key_store.rs`, `query.rs`, `driver_command.rs`, `log_redact.rs` | W4 |

## 波次编排

### W1（可并行）
- `cr-p0-mcp` ↔ `cr-p0-ext-install`：文件无重叠

### W2（可并行，软依赖：无）
- `cr-p1-session-id` / `cr-p1-workflow-ui` / `cr-p1-split-ai`：不同子系统；`WorkflowChatPanel` 仅 session-id 轨触碰

### W3（可并行）
- 三轨文件面分离；`cr-p2-schema-cmd` 与 `cr-p2-navigator-split` 均碰 schema 概念但文件不同

### W4（可并行）
- `cr-p3-dedup-dialogs` ↔ `cr-p3-secrets-hardening`：无重叠

### R 阶段（全部合并后）
- `cargo test -p datazen --lib`
- `npx vitest run`
- `pnpm exec tsc --noEmit`
- E2E 登记项统一回归（见各轨 progress）

---

## 各轨验收标准（摘要）

### cr-p0-mcp
1. MCP stdio 启动需本地认证（如 `{appData}/mcp.token` 或等价机制），未授权客户端无法连接
2. `read_resource_inner` 应用 `mcp_permission_mode` 与 connection allowlist
3. `query-history` 资源受 allowlist 约束；`read_only` 模式下不暴露敏感 history
4. 单元测试覆盖 allowlist + permission 组合
5. 文档更新 `docs/architecture/backend/mcp.md`

### cr-p0-ext-install
1. `install_extension_from_path` / `inspect_extension_at_path` 不接受 webview 任意路径
2. 路径仅来自 Tauri 原生对话框回调或等价 host-side 白名单（参考 `save_text_with_dialog` 模式）
3. 前端 `extensions.ts` 移除/废弃直接 path 参数入口
4. Host 单测 + 必要 E2E 登记

### cr-p1-session-id
1. GUI IPC（schema/query/export）强制 `db_session_id`，移除对 `connection_id` 的静默回退
2. MCP/db_tools 可保留显式命名的兼容 API，但与 GUI 路径分离
3. `WorkflowChatPanel` 从 live session 解析 `dbSessionId`，不传 `connections[].id`
4. 更新 `docs/architecture/naming.md` 迁移说明

### cr-p1-workflow-ui
1. `WorkflowPanel` 复用 `WorkflowForm` + 共享 run/history 组件
2. 删除嵌入式重复 Form/StepEditor（~500 行净减）
3. Vitest 覆盖 AI 面板内 workflow 路径

### cr-p1-split-ai
1. `commands/ai.rs` 拆为 `commands/ai/{mod,config,chat,generate,prompts,util}.rs`
2. `workflow_*` IPC 迁至 `commands/workflow.rs` 或 `workflow/` 模块
3. `ai_integration_tests.rs` 迁至 `src-tauri/tests/` 或 colocated
4. 行为不变；`cargo test -p datazen --lib` 全绿

### cr-p2-sync-pairing
1. 暴露 `classify_data_sync_pair`（或等价）IPC
2. 前端 `syncPairing.ts` 改为调用 IPC 或 codegen 同步；消除 FE/BE 硬编码列表漂移
3. 单测覆盖 redis/mongodb/kiwi 等边界

### cr-p2-schema-cmd
1. `get_databases` / `get_tables` / `get_table_schema` 迁移至 driver command 或文档化 trait 快路径 + driver 检查清单
2. 前端 `databaseCommands` 统一走 driver command 或保留 trait 路径但有明确 deprecation 计划

### cr-p2-navigator-split
1. 抽取 `schemaTreeCategories.ts` 共享常量
2. `ConnectionNavigatorTree.tsx` 拆分子模块，主文件 <800 行
3. 优先复用 `UnifiedSchemaTree` 减少双实现

### cr-p3-dedup-dialogs
1. 通用 `LimitationsDialog` + `createDismissPrefs(key)` factory
2. 参数化 `AdminCreateDialog`（command id + i18n prefix）
3. `lib/errors.ts` → `toErrorMessage(err: unknown)`

### cr-p3-secrets-hardening
1. `.key` 写入后 `chmod 0600`（Windows 等价 ACL 如适用）
2. 所有 SQL debug preview 经 `log_redact::redact_secrets_for_log`
3. 查询历史写入前可选 scrub（至少文档化风险）
