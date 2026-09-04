# Track refactor-sqldump 进度管理

## 1. 功能摘要
- **Track ID**：refactor-sqldump
- **任务目标**：将 `packages/driver-api/src/sql_dump.rs` (1,359 行) 拆分为 `packages/driver-api/src/sql_dump/` 目录模块（`mod.rs`, `dump.rs`, `restore.rs`, `parser.rs`, `tests.rs`）。
- **当前状态**：未开始
- **编码 Commit**：—
- **测试 Commit**：—

## 2. E2E 用例表
- 纯 driver-api 内部算法与工具重构，依赖单元测试覆盖。

## 3. 测试结果与覆盖率
- 目标套件：`cargo test -p datazen-driver-api --lib`
- 基线测试数：全绿通过

## 4. 设计决策 / 遗留注意
- `packages/driver-api/src/sql_dump/mod.rs` 重新 `pub use` 所有导出函数与结构体，完全兼容原有调用。
- 547 行单测独立移至 `tests.rs`。
