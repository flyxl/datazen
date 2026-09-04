# Track refactor-mysql 进度管理

## 1. 功能摘要
- **Track ID**：refactor-mysql
- **任务目标**：将 `packages/drivers/mysql/src/mysql.rs` (2,756 行) 拆分为高内聚子模块（driver 门面、连接池、执行与取消、文本/类型解码、例程与触发器 DDL、单测等）。
- **当前状态**：已完成（Tester 复验通过）
- **编码 Commit**：5d7843c1
- **测试 Commit**：f962a09c

## 2. E2E 用例表
- 本任务为 Rust 驱动内部纯重构，无 Host UI/IPC 行为变更，依赖 Rust 单元测试覆盖。

## 3. 测试结果与覆盖率
- 目标套件：`cargo test -p datazen-driver-mysql --lib`
- 编码代理自报：**83 passed; 0 failed**（`CARGO_TARGET_DIR=/tmp/target-refactor-mysql`）
- Tester 独立复验：**83 passed; 0 failed; 0 ignored**（`CARGO_TARGET_DIR=/tmp/target-test-refactor-mysql`，2026-09-04）
- 编码 vs 复验：数字一致，无差异

## 4. 设计决策 / 遗留注意
- 对外 `pub struct MysqlDriver` 及其 `DatabaseDriver` 实现维持不变。
- 内部拆出的模块全部设为 `pub(crate)`，通过 `mysql.rs` 内 `#[path = "..."]` 挂载同级子模块文件。
- 单测移动到独立 `tests.rs`，由 `mysql.rs` 以 `#[cfg(test)] #[path = "tests.rs"] mod tests;` 引入。
- 拆分后模块：
  - `mysql.rs` (~1,460 行)：结构体、`new()`、`DatabaseDriver` trait 实现及 schema 辅助方法
  - `connection.rs`：连接选项与池化 session `USE` 辅助
  - `execution.rs`：`MysqlQueryExecution`、线程绑定、流式执行与取消
  - `type_decode.rs`：文本列解码与 Row → `Value` 转换
  - `catalog.rs`：routine/trigger DDL dump 与 `SHOW CREATE` 提取
  - `sql.rs`：语句拆分、结果集判定、`SELECT LIMIT`
  - `tests.rs`：原 `mysql.rs` 底部全部单元测试
- `DatabaseDriver` impl 仍集中在 `mysql.rs`（~1,200 行）；进一步拆 trait 方法需额外设计，留待后续 track。
