# Track refactor-pg 进度管理

## 1. 功能摘要
- **Track ID**：refactor-pg
- **任务目标**：将 `packages/drivers/postgres/src/postgres.rs` (2,867 行) 拆分为高内聚子模块（driver 门面、连接池、执行与取消、类型解码、系统目录 DDL、单测等）。
- **当前状态**：未开始
- **编码 Commit**：—
- **测试 Commit**：—

## 2. E2E 用例表
- 本任务为 Rust 驱动内部纯重构，无 Host UI/IPC 行为变更，依赖 Rust 单元测试和集成测试覆盖。

## 3. 测试结果与覆盖率
- 目标套件：`cargo test -p datazen-driver-postgres --lib`
- 基线测试数：全绿通过

## 4. 设计决策 / 遗留注意
- 对外 `pub struct PostgresDriver` 及其 `DatabaseDriver` 实现维持不变。
- 内部拆出的模块全部设为 `pub(crate)`。
- 单测移动到独立 `tests.rs` 或同级测试模块中。
