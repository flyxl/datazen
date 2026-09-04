# Track refactor-sqldump 进度管理

## 1. 功能摘要
- **Track ID**：refactor-sqldump
- **任务目标**：将 `packages/driver-api/src/sql_dump.rs` (1,359 行) 拆分为 `packages/driver-api/src/sql_dump/` 目录模块（`mod.rs`, `dump.rs`, `restore.rs`, `parser.rs`, `tests.rs`）。
- **当前状态**：已完成（PASSED）
- **编码 Commit**：23aa507f — refactor(driver-api): modularize sql_dump module
- **测试 Commit**：test(driver-api): verify modularization of sql_dump module（见 `git log feature/test-refactor-sqldump -1`）

## 2. E2E 用例表
- 纯 driver-api 内部算法与工具重构，依赖单元测试覆盖。

## 3. 测试结果与覆盖率
- 目标套件：`cargo test -p datazen-driver-api --lib`
- 编码自报：108 passed; 0 failed
- **Tester 独立复验**（2026-09-04，`CARGO_TARGET_DIR=/tmp/target-test-refactor-sqldump`）：**108 passed; 0 failed; 0 ignored** — 与编码自报一致

## 4. 设计决策 / 遗留注意
- `packages/driver-api/src/sql_dump/mod.rs` 重新 `pub use` 所有导出函数与结构体，完全兼容原有调用。
- 547 行单测独立移至 `tests.rs`。
- 模块划分：
  - `dump.rs` — DDL/INSERT 批处理、全库转储管线
  - `restore.rs` — `RestoreSession`、恢复语句执行与错误恢复
  - `parser.rs` — SQL 标识符/字面量解析辅助（`created_relation_ident` 等）
  - `tests.rs` — 原 `#[cfg(test)]` 单测

## 5. Tester 范围完整性审查
| 子模块 | 行数 | 职责 | 状态 |
|--------|------|------|------|
| `mod.rs` | 21 | Facade + `sql_split` 重导出 | ✓ |
| `dump.rs` | 432 | 转储管线（DDL/INSERT/全库） | ✓ |
| `restore.rs` | 278 | 还原会话与错误恢复 | ✓ |
| `parser.rs` | 101 | 标识符/字面量解析 | ✓ |
| `tests.rs` | 550 | 12 单测（9 `#[test]` + 3 `#[tokio::test]`） | ✓ |

- 原 `sql_dump.rs` 已删除；生产代码合计 1,382 行（含 mod 声明），与原 1,359 行 + 模块边界一致。
- `mod.rs` `pub use` 覆盖原 22 项公共 API + 5 项 `sql_split` 重导出，100% 对齐。
- 内部 helper（`strip_kw`、`relation_already_exists` 等）迁至 `parser.rs` 为 `pub(crate)`，跨模块引用正确。

## 6. Tester 逻辑正确性审查（commit 23aa507f）
- 生产代码 diff 为纯模块搬迁，转储/还原管线逻辑无行为变更。
- `dump_sql_database` → `dump_one_object` → `append_batched_inserts` 链路完整保留于 `dump.rs`。
- `RestoreSession::feed/finish` + `recover_restore_statement_default` 链路完整保留于 `restore.rs`。
- `created_relation_ident` / `extract_nextval_sequence_names` 解析逻辑完整保留。
