# AI 功能开发进度

> 跟踪 `feat/ai-features` 分支上所有 AI 功能模块的开发和测试状态。  
> 每完成一个功能模块后更新此文件。

## 总体进度

| Phase | 模块 | 状态 | 单元测试 | E2E 测试 | 提交 |
|-------|------|------|---------|---------|------|
| 0 | `packages/ai-api` crate 基础结构 | ✅ 已完成 | ✅ 17 pass | — | — |
| 0 | OpenAI Provider 实现 | ✅ 已完成 | ✅ 7 pass | — | — |
| 0 | Anthropic Provider 实现 | ✅ 已完成 | ✅ 5 pass | — | — |
| 0 | Ollama Provider 实现 | ✅ 已完成 | ✅ 5 pass | — | — |
| 0 | AiProviderRegistry + init | ✅ 已完成 | ✅ 7 pass | — | — |
| 0 | AppState 扩展 + lib.rs 初始化 | ✅ 已完成 | ✅ 编译通过 | — | — |
| 0 | AI 配置持久化 (Store) | ✅ 已完成 | ✅ 编译通过 | — | — |
| 0 | AI IPC Commands | ✅ 已完成 | ✅ 1 pass | — | — |
| 0 | 前端: types + commands + aiStore | ✅ 已完成 | ✅ TS 编译通过 | — | — |
| 0 | 前端: AI 设置页面 | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 0 | i18n (zh-CN + en) | ✅ 已完成 | ✅ TS 编译通过 | — | — |
| 1 | SchemaContextBuilder + PromptBuilder | ✅ 已完成 | ✅ 编译通过 | — | — |
| 1 | NL2SQL + 诊断 IPC 命令 | ✅ 已完成 | ✅ 编译通过 | — | — |
| 1 | NL2SQL 前端 UI (Nl2SqlPanel) | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 1 | SQL 错误诊断前端 UI (DiagnosisPanel) | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 2 | EXPLAIN 分析后端 IPC | ✅ 已完成 | ✅ 编译通过 | — | — |
| 2 | EXPLAIN 前端 UI (ExplainPanel) | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 3 | AI Chat 后端 IPC | ✅ 已完成 | ✅ 编译通过 | — | — |
| 3 | AI Chat 前端 (AiChatPanel) | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 7 | 智能筛选后端 (ai_parse_filter) | ✅ 已完成 | ✅ 编译通过 | — | ✅ |
| 7 | 智能筛选前端 (NlFilterInput) | ✅ 已完成 | ✅ TS 编译通过 | ✅ | ✅ |
| 4 | MCP Server 基础 (tools + resources + prompts) | ✅ 已完成 | ✅ 6 pass | 🔲 | — |
| 4 | MCP 设置页面 (Settings UI) | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 5 | Skills 后端 (SkillRegistry + SkillExecutor) | ✅ 已完成 | ✅ 9 pass | — | — |
| 5 | Skills 前端 (SkillsPanel + Chat集成) | ✅ 已完成 | ✅ TS 编译通过 | 🔲 | — |
| 5 | Skills MCP 集成 (list_skills + run_skill) | ✅ 已完成 | ✅ 编译通过 | 🔲 | — |
| 6 | MCP Client | 🔲 未开始 | 🔲 | 🔲 | — |

## 状态说明

- 🔲 未开始
- 🔨 开发中
- ✅ 已完成
- ❌ 测试不通过（需修复）
- 🐛 有已知 Bug

## Phase 0 详细记录

### `packages/ai-api` crate
- **完成时间**: 2026-08-01
- **单元测试**: 17 pass (types/traits/factory/mock-provider)
- **测试文件**: `packages/ai-api/tests/api_tests.rs`

### OpenAI / Anthropic / Ollama Provider
- **完成时间**: 2026-08-01
- **单元测试**: OpenAI 7 pass, Anthropic 5 pass, Ollama 5 pass
- **测试文件**: 各 provider 文件内 `#[cfg(test)]` 模块

### AiProviderRegistry + init
- **完成时间**: 2026-08-01
- **单元测试**: 7 pass (empty/register/get/overwrite/list/init/all)
- **测试文件**: `src-tauri/src/ai/registry.rs` 内 `#[cfg(test)]` 模块

### AppState + Store + IPC Commands
- **完成时间**: 2026-08-01
- **变更**: 
  - `commands/mod.rs` 添加 `ai_registry: Arc<AiProviderRegistry>` 到 AppState
  - `store/mod.rs` 添加 AI 配置加密存储 (`ai_config.enc`)
  - `commands/ai.rs` 新增 6 个 IPC 命令
  - `lib.rs` 初始化 AI registry 并恢复保存的配置

### 前端 (types + commands + store + UI)
- **完成时间**: 2026-08-01
- **变更**:
  - `src/types/index.ts` 添加 AI 类型定义
  - `src/commands/ai.ts` IPC 封装
  - `src/stores/aiStore.ts` Zustand store
  - `src/windows/settings/SettingsWindow.tsx` 添加 AI 设置区域
  - `src/locales/zh-CN.ts` + `en.ts` 添加翻译键

## Phase 1 详细记录

### SchemaContextBuilder + PromptBuilder
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/ai/context.rs` — 构建紧凑 DDL 作为 LLM 上下文，支持 token 预算控制
  - `src-tauri/src/ai/prompt.rs` — NL2SQL / 错误诊断 / EXPLAIN 分析的 prompt 模板管理
  - `src-tauri/src/ai/mod.rs` 导出新模块

### NL2SQL + 诊断 IPC 命令
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/commands/ai.rs` 新增 `ai_generate_sql` (流式) 和 `ai_diagnose_error`
  - `src-tauri/src/commands/mod.rs` 注册新命令
  - `src-tauri/src/lib.rs` 添加 `schema_context_builder` 到 AppState

### NL2SQL 前端 UI
- **完成时间**: 2026-08-01
- **变更**:
  - `src/components/ai/Nl2SqlPanel.tsx` — 可折叠的 NL2SQL 输入面板，实时流式显示生成的 SQL
  - `src/windows/connection/QueryPanel.tsx` — 集成 AI 按钮到工具栏，嵌入 Nl2SqlPanel
  - `src/stores/aiStore.ts` — 添加 NL2SQL 状态管理和流处理
  - `src/commands/ai.ts` — 添加 `generateSql` IPC 调用和事件监听

### SQL 错误诊断前端 UI
- **完成时间**: 2026-08-01
- **变更**:
  - `src/components/ai/DiagnosisPanel.tsx` — 诊断结果展示面板，含修正 SQL 应用功能
  - `src/windows/connection/QueryPanel.tsx` — 在错误区域添加"诊断"按钮，嵌入 DiagnosisPanel
  - `src/stores/aiStore.ts` — 添加诊断状态管理
  - `src/commands/ai.ts` — 添加 `diagnoseError` IPC 调用

### 前端初始化
- `src/windows/connection/ConnectionWindow.tsx` — 启动时加载 AI 配置

### i18n
- `src/locales/zh-CN.ts` + `en.ts` — 添加 NL2SQL 和诊断相关翻译键

## Phase 2 详细记录

### EXPLAIN 分析后端 IPC
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/commands/ai.rs` 新增 `ai_analyze_explain` 命令
  - `src-tauri/src/lib.rs` 注册新命令
  - 复用已有的 `PromptBuilder::explain_analysis_system` prompt
  - 复用 `strip_markdown_fences` 处理 LLM 响应

### EXPLAIN 前端 UI
- **完成时间**: 2026-08-01
- **变更**:
  - `src/types/index.ts` 添加 `ExplainAnalysis`, `Bottleneck`, `ExplainSuggestion` 类型
  - `src/commands/ai.ts` 添加 `analyzeExplain` IPC 封装
  - `src/stores/aiStore.ts` 添加 EXPLAIN 分析状态管理
  - `src/components/ai/ExplainPanel.tsx` 创建 EXPLAIN 可视化面板（含 AI 分析）
  - `src/windows/connection/QueryPanel.tsx` 集成 Explain 按钮和视图切换
  - `src/locales/zh-CN.ts` + `en.ts` 添加 EXPLAIN 翻译键

## Phase 3 详细记录

### AI Chat 后端 IPC
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/commands/ai.rs` 新增 `ai_chat` 流式命令（支持可选 schema 注入）
  - `src-tauri/src/lib.rs` 注册新命令

### AI Chat 前端 UI
- **完成时间**: 2026-08-01
- **变更**:
  - `src/components/ai/AiChatPanel.tsx` — 侧边栏对话面板（消息渲染 + 代码块提取 + SQL 插入）
  - `src/windows/connection/SqlConnectionView.tsx` — 集成 Chat 按钮和 320px 侧边栏
  - `src/stores/aiStore.ts` — Chat session 管理、流式处理路由（NL2SQL + Chat）
  - `src/commands/ai.ts` — 添加 `chat` IPC 封装
  - `src/types/index.ts` — 添加 `AiChatMessage`, `AiChatSession` 类型
  - `src/locales/zh-CN.ts` + `en.ts` — Chat 翻译键

## Phase 7 详细记录

### 智能筛选后端 (ai_parse_filter)
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/commands/ai.rs` 新增 `ai_parse_filter` 命令
  - `src-tauri/src/ai/prompt.rs` 新增 `nl_filter_system` prompt 模板
  - 包含列名校验和 null 值运算符规范化

### 智能筛选前端 (NlFilterInput)
- **完成时间**: 2026-08-01
- **变更**:
  - `src/components/ai/NlFilterInput.tsx` — 自然语言筛选输入组件
  - `src/stores/aiStore.ts` — 筛选状态管理（解析、错误、结果）
  - `src/stores/tableDataStore.ts` — 添加 `setFilters` 批量设置筛选
  - `src/windows/connection/TableView.tsx` — 集成筛选组件
  - 修复 BUG-001~008（竞态条件、表切换状态泄漏等）

## Phase 4 详细记录

### MCP Server 基础
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/Cargo.toml` 添加 `rmcp` 2.2.0 + `schemars` + `tokio-util` + `urlencoding`
  - `src-tauri/src/mcp/mod.rs` — MCP 服务器启动逻辑（stdio 传输，优雅关闭）
  - `src-tauri/src/mcp/server.rs` — 完整 MCP 服务器实现
    - 7 个 Tools: `list_connections`, `list_databases`, `list_tables`, `query`, `get_schema`, `explain_query`, `describe_table`
    - 3 个 Resources: `datazen://connections`, `datazen://query-history`, `datazen://schema/{id}/{db}`
    - 3 个 Prompts: `nl2sql`, `diagnose_error`, `explain_plan`
    - `resolve_connection()` 自动连接机制（config ID → runtime connection）
    - URI 编码处理（支持特殊字符的连接 ID）
    - 6 个单元测试
  - `src-tauri/src/commands/mcp.rs` — IPC 命令（`mcp_get_status`, `mcp_start_stdio`, `mcp_stop`）
  - `src-tauri/src/lib.rs` — 注册 MCP 模块和 IPC 命令
  - `src/commands/ai.ts` — 前端 MCP IPC 封装
  - Bug 修复: MCP-001 (tools 注册), MCP-002 (连接 ID 解析), MCP-005 (优雅关闭), MCP-006 (URI 编码)

### MCP 设置页面
- **完成时间**: 2026-08-01
- **变更**:
  - `src/windows/settings/SettingsWindow.tsx` — 添加 MCP Server 管理区域（启停控制 + 配置示例）
  - `src/locales/zh-CN.ts` + `en.ts` — 添加 MCP 翻译键

## Phase 5 详细记录

### Skills 后端
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/mcp/skills.rs` — 完整 Skills 系统实现
    - YAML 格式的 Skill 定义（query + ai 步骤类型）
    - SkillRegistry — 从文件系统加载/保存/删除/列表
    - SkillExecutor — 顺序执行步骤（SQL 查询 + AI 推理）
    - SkillContext — 变量替换（{{var}} + {{steps.id.result}} + 内置变量）
    - 路径遍历防护（ID 校验）
    - 必填变量验证 + 默认值填充
    - 查询结果行数限制（1000）
    - 9 个单元测试
  - `src-tauri/src/commands/ai.rs` — 5 个 IPC 命令（list/execute/save/delete/reload）
  - `src-tauri/src/commands/mod.rs` — AppState 添加 skill_registry
  - `src-tauri/src/lib.rs` — 初始化 SkillRegistry + 注册命令
  - 依赖: serde_yaml, regex

### Skills MCP 集成
- **完成时间**: 2026-08-01
- **变更**:
  - `src-tauri/src/mcp/server.rs` — 新增 `list_skills` + `run_skill` tools + `datazen://skills` resource

### Skills 前端
- **完成时间**: 2026-08-01
- **变更**:
  - `src/types/index.ts` — Skill 相关类型定义
  - `src/commands/ai.ts` — Skill IPC 封装
  - `src/stores/aiStore.ts` — Skill 状态管理
  - `src/components/ai/SkillsPanel.tsx` — Skill 选择、变量输入、执行、结果展示
  - `src/components/ai/AiChatPanel.tsx` — Chat/Skills 标签页切换
  - `src/locales/zh-CN.ts` + `en.ts` — Skills 翻译键

## 提交历史

| 提交 | Phase | 描述 |
|------|-------|------|
| `17465dc` | Phase 0 | AI infrastructure foundation |
| `990a3db` | Phase 1 | NL2SQL and SQL error diagnosis |
| `6b36524` | Phase 2 | EXPLAIN visualization with AI analysis |
| `9c167b8` | Phase 3 | AI Chat sidebar assistant |

---

*此文件随开发进度持续更新。*
