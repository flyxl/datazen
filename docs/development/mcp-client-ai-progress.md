# MCP Client AI 集成 — 开发进度

> 轨道：`feature/mcp-client-ai`  
> Worktree：`/Users/flyxl/code/datazen-mcp-client-ai`  
> 计划：`docs/todo/mcp-client-ai-integration.md`

## 1. 功能总览表

| 编号 | 功能 | 来源 | 状态 | 编码 commit | 测试 commit |
|------|------|------|------|-------------|-------------|
| F1 | Phase 1 配置持久化与启动重连 | 计划 Phase 1 | 编码完成 | 3a11036d | — |
| F2 | Phase 2 Tool Schema 暴露 | 计划 Phase 2 | 编码完成 | 6b6bfc5a | — |
| F3 | Phase 3 AI Tool Loop 集成 | 计划 Phase 3 | 编码完成 | 42d863d0 | — |
| F4 | Phase 4 UI polish + 文档 + E2E | 计划 Phase 4 | 已完成 | 1702a98e | a0084091 |
| F5 | Rust 核心缺口补齐 | 计划遗漏项 | 编码完成 | cc13ebf5 | — |
| F6 | 前端/文档/E2E 缺口补齐 | 计划遗漏项 | 编码完成 | — | — |

## 2. Bug 台账

| Bug ID | 所属功能 | 描述 | 状态 | 记录时间 | 验证记录 |
|--------|----------|------|------|----------|----------|
| F-BUG-001 | F4 / 三件套 | `src/stores/aiStore.ts:6` — `McpServerConfig` 类型 import 未使用，导致 `pnpm exec tsc --noEmit` 报 TS6196 | 已修复 | 2026-08-27 | 修复 b2581b3b；复验 b1627443：`tsc --noEmit` 0 errors |
| F-BUG-002 | F4 / 覆盖率 | `McpClientSection.tsx` 行覆盖率 71.13%（目标 ≥80%），未覆盖 retry/error badge/部分 env 分支 | 已修复 | 2026-08-27 | 修复 b2581b3b；复验 b1627443：定向行覆盖率 98.96% |

## 3. 测试约定

- **三件套**：`cargo test -p datazen --lib` / `npx vitest run`（Host 单测）/ `pnpm exec tsc --noEmit`
- **覆盖率**：改动 TS 文件 vitest `--coverage` 目标 ≥80%
- **E2E**：功能轮登记用例；完整 `pnpm e2e` 标注【留待 R 回归】
- **worktree 前置**：`node scripts/resolve-drivers.mjs --drivers=basic`；`CARGO_TARGET_DIR` 独立目录

## 4. 功能小节

### F1–F4 合计（MCP Client AI MVP）

**范围：** 持久化 MCP Client 配置、启动 auto-reconnect、MCP tool schema 暴露、`run_streaming_tool_loop` 路由、`ai_chat` 注册 MCP tools、Settings UI polish、架构文档、Settings E2E 登记。

**E2E 用例表：**

| 编号 | 前置 | 步骤 | 断言 | 备注 |
|------|------|------|------|------|
| SS-MCP-CLIENT-001 | Settings 打开 | 添加 MCP server + env 保存 | savedConfigs 可见；`get_settings` 含 id/env | 已实现于 `e2e/specs/settings.ts`；**【留待 R 回归】** 本轮未跑 `pnpm e2e` |
| SS-MCP-CLIENT-002 | 连接 filesystem MCP | Chat 请求读文件 | 返回文件内容 | 【留待 R 回归】需真实 MCP + LLM |
| SS-MCP-CLIENT-003 | 重启应用 | enabled server | auto-reconnect | 【本机可执行】手工 |

**测试结果（测试代理 2026-08-27 复验，基线 b1627443）：**

| 套件 | 结果 | 数字 |
|------|------|------|
| `cargo test -p datazen --lib` | ✅ 通过 | 1127 passed, 0 failed, 2 ignored |
| `npx vitest run` | ✅ 通过 | 249 files, 2043 passed |
| `pnpm exec tsc --noEmit` | ✅ 通过 | 0 errors |

**覆盖率（定向 `--coverage.include` MCP 改动文件，复验 b1627443）：**

| 文件 | Stmts | Lines | 判定 |
|------|-------|-------|------|
| `src/stores/aiStore.ts` | 84.91% | 86.74% | ✅ ≥80% |
| `src/windows/settings/McpClientSection.tsx` | 95.61% | 98.96% | ✅ ≥80% |
| `src/stores/settingsStore.ts` | 0% | 0% | ⚠️ 本轮仅增 `mcpClientServers: []` 默认值，无专属单测 |

**范围完整性审查（对照计划 §2.1 MVP）：**

| MVP 目标 | 状态 | 说明 |
|----------|------|------|
| 配置持久化 `settings.json` | ✅ | `AppSettings.mcp_client_servers` + 前端 `mcpClientServers` |
| 启动 auto-reconnect（enabled） | ✅ | `lib.rs` setup 非阻塞 spawn `mcp_client_connect_impl` |
| AI Chat 注册 MCP tools | ✅ | `ai_chat_impl` → `mcp_tool_definitions` + `supports_tools()` gate |
| Tool loop 路由执行 MCP | ✅ | `classify_tool` / `execute_mcp_tool` / loop 重构 |
| Settings UI（saved + tools + env） | ✅ | `McpClientSection.tsx` 两区 + env 编辑 |
| Rust 集成测试 + Settings E2E | ⚠️ 部分 | E2E-001 已登记；缺计划 §6.2 `ai_chat_mcp_tool_roundtrip`、`ai_chat_mcp_and_db_same_round` |
| 对齐 `ai.md` 文档 | ✅ | §1.10 AI Chat MCP 工具已更新 |

**逻辑正确性审查（`git diff 71f9163d..HEAD` 要点）：**

- `ai.rs`：`ToolKind` 分类、`mcp/{serverId}/{toolName}` 路由、executable vs ask_questions 终止条件、DB+MCP 同轮顺序执行 — 与计划一致
- `lib.rs`：启动读取 `mcp_client_servers.filter(enabled)` 并行 connect，失败 warn 不阻塞 — 符合 Phase 1
- `McpClientSection` / `aiStore`：save/connect 分离、`updateSettings` 持久化 — 符合 Phase 1
- `mcp/client.rs`：`qualified_name` + `input_schema` 填充 — 符合 Phase 2

**遗漏 / 非阻塞项：**

- ~~`chat.txt` MCP 前缀说明（计划 Task 3.6 可选）~~ — F6 已实现
- ~~`AiChatPanel` "Calling mcp/…" 流式提示（计划 Task 4.3 可选）~~ — F6 已实现
- HTTP transport — Phase 5，计划外

**设计决策 / 遗留：**

- Tool 命名：`mcp/{serverId}/{toolName}`
- HTTP transport：Phase 5 未实现
- 同轮 ask_questions + MCP：已知边界（executable 与 terminal 分类已处理）
