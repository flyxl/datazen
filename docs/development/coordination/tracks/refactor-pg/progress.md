# Track refactor-pg 进度管理

## 1. 功能摘要
- **Track ID**：refactor-pg
- **任务目标**：将 `packages/drivers/postgres/src/postgres.rs` (2,867 行) 拆分为高内聚子模块（driver 门面、连接池、执行与取消、类型解码、系统目录 DDL、单测等）。
- **当前状态**：已完成
- **编码 Commit**：见下方 git log（`refactor(postgres): modularize postgres driver implementation`）
- **测试 Commit**：—（Rust 单元测试同编码 commit 验证）

## 2. E2E 用例表
- 本任务为 Rust 驱动内部纯重构，无 Host UI/IPC 行为变更，依赖 Rust 单元测试和集成测试覆盖。

## 3. 测试结果与覆盖率
- 目标套件：`cargo test -p datazen-driver-postgres --lib`
- 结果：**98 passed; 0 failed**（全绿）

## 4. 模块拆分结果

| 模块 | 职责 | 约行数 |
|------|------|--------|
| `postgres.rs` | `PostgresDriver` 结构体、`new()`、`DatabaseDriver` trait 薄门面 | ~328 |
| `connection.rs` | `open_pool`、`validate_database_name`、`resolve_connect_database`、`build_pg_options`、连接/切换库 | ~406 |
| `execution.rs` | `PgQueryExecution`、流式执行、PID 绑定与取消、query/transaction | ~790 |
| `type_decode.rs` | PgRow → `Value` 解码、`bind_values`、EXPLAIN 指标提取 | ~231 |
| `catalog.rs` | catalog DDL 生成、dump/execute_command 实现 | ~442 |
| `schema.rs` | `get_columns` / `get_table_schema`  introspection | ~219 |
| `sql.rs` | `parse_pg_table_ref`、LIMIT 启发式、DDL 列收集 | ~187 |
| `tests.rs` | 原 `postgres.rs` 底部全部 `#[cfg(test)]` 单测 | ~492 |

## 5. 设计决策 / 遗留注意
- 对外 `pub struct PostgresDriver` 及其 `DatabaseDriver` 实现方法签名维持不变。
- 内部子模块均为 crate 内模块；`PostgresDriver` 字段设为 `pub(crate)` 以支持跨模块 `impl PostgresDriver`。
- 单测经 `postgres.rs` 中 `#[path = "tests.rs"] mod tests;` 引入。
