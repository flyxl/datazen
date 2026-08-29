# cr-p1-split-ai — 进度

**轨 ID：** cr-p1-split-ai | **分支：** feature/cr-p1-split-ai | **状态：** 完成

## 范围
拆分 commands/ai.rs；workflow_* IPC 迁出；测试文件迁移。

## 完成项

- [x] `commands/ai.rs`（2893 行）拆为 `commands/ai/{mod,config,chat,generate,prompts,util}.rs`
- [x] `workflow_*` / `workflow_history_*` IPC 迁至 `commands/workflow.rs`
- [x] `ai_integration_tests.rs` → `commands/ai/integration_tests.rs`
- [x] `ai_mock_provider_tests.rs` → `commands/ai/mock_provider_tests.rs`
- [x] 单元测试 / `ipc_contract_guards` 迁至 `commands/ai/tests.rs`、`ipc_contract_guards.rs`
- [x] `commands/mod.rs` 注册 `workflow` 模块并 re-export
- [x] `lib.rs` IPC 注册路径不变（经 `commands::` re-export）

## 模块布局

| 文件 | 职责 |
|------|------|
| `ai/config.rs` | Provider 列表、配置 CRUD、远程模型拉取 |
| `ai/generate.rs` | NL2SQL、诊断、EXPLAIN、filter、schema doc、connection/query 分析 |
| `ai/chat.rs` | Chat、DB/MCP 工具定义与 streaming tool loop |
| `ai/prompts.rs` | Prompt 模板 list/override IPC |
| `ai/util.rs` | resolve_ai、stream callback、JSON 解析、语言 hint |
| `workflow.rs` | Workflow CRUD/execute/history IPC |

## 验收

```bash
CARGO_TARGET_DIR=.../datazen-cr-p1-split-ai/target cargo test -p datazen --lib
```

- AI 相关：`commands::ai` 100 tests 全绿
- 全库：1171 passed；1 失败为既有 `connection::coverage_tests::connect_dedicated_opens_separate_session_from_reuse`（与本轨无关）
