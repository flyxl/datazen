# Track: ai-prompt-async (Phase 2.5 — Prompt Async FR-12)

## 状态: IN_PROGRESS

## 目标
1. `prompt_resolver.rs` async 化 — 文件 I/O 改 tokio
2. `{{dialect_notes}}` placeholder 支持
3. `driver-api` `PromptTemplate` 新增 `dialect_notes` 字段（默认空）
4. 各内置驱动补方言小抄（PG/MySQL/SQLite 优先）
5. 启动预热 — 首次请求前预加载 prompt
6. 残留断言清理

## 文件清单
- 后端：`src-tauri/src/ai/prompt_resolver.rs`（async 化）、`packages/ai-api/src/traits.rs`（dialect_notes）
- 驱动：`packages/drivers/postgres/`、`packages/drivers/mysql/`、`packages/drivers/sqlite/`（方言小抄）
- 测试：`prompt_resolver` 单测 + 驱动方言覆盖
