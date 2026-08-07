# 进度：会话级多库 Connection Window（MySQL / PostgreSQL）

> 分支：`feat/multi-database-session-ui`  
> 目标：未指定 database 时在 Connection Window 展示账号可见的全部数据库（类似 Kiwi）；`multiDatabases` 按**实际连接**可见库数量决定（`length > 1` 为多库 UI），而非仅静态类型标志。

## 约定

| 项 | 约定 |
|----|------|
| 能力标志 | `DatabaseTypeMeta.hasMultiDatabase` = 该驱动**支持**多库浏览 |
| 会话标志 | `schemaStore.isMultiDatabase` = `hasMultiDatabase && databases.length > 1` |
| 配置了 database | 仍列出全部可见库；preferred 预选配置库 |
| 未配置 database | 列出全部可见库；多库树 + 懒加载表 |
| 开发循环 | 实现 + 单元测试 → **新 agent** E2E 测试（只出报告）→ 失败则编码 agent 修复 → 通过后提交 |

## 功能拆分

| ID | 功能 | 状态 | 提交 |
|----|------|------|------|
| F1 | MySQL/MariaDB 实现 `use_database` + Rust 单元测试 | ✅ implemented（unit + gated live IT） | — |
| F2 | 前端会话级多库（mysql/mariadb）：registry、schemaStore、SchemaTree、QueryPanel、WorkflowPanel + Vitest | ⬜ pending | — |
| F3 | PostgreSQL：`get_tables` 尊重 database + `use_database` + Rust 单元测试 | ⬜ pending | — |
| F4 | 前端启用 postgresql 多库能力 + Vitest | ⬜ pending | — |
| F5 | 更新必要文档并提交 | ⬜ pending | — |

## 测试记录索引

| ID | 测试 agent | 结果文件 | 结论 |
|----|------------|----------|------|
| F1 | test-agent (fresh) | [progress-multi-database-session-f1-test.md](./progress-multi-database-session-f1-test.md) | **PASS** |
| F2 | — | — | — |
| F3 | — | — | — |
| F4 | — | — | — |

## 变更日志

### 2026-08-07 — F1 MySQL/MariaDB `use_database`

- `MysqlDriver`（mysql + mariadb 共用）实现 `use_database`：校验/引用标识符后执行 `USE \`db\``，并记录 `active_databases`；后续 query/execute/explain/schema 在同一连接上再 `USE`，保证连接池下未限定名落在目标库
- 已在同库时 no-op；空名 / NUL → `InvalidConfig`；`USE` 失败 → `QueryFailed`
- **Live fix：** `USE` 必须走 MySQL text/`COM_QUERY`（`Executor::execute(&str)`）；`sqlx::query` 总会走 prepared protocol，报 `1295 This command is not supported in the prepared statement protocol yet`
- **Live fix：** `current_database` / per-conn check 走 text protocol — MySQL prepared `SELECT DATABASE()` 会固定 PREPARE 时的库名
- **Live fix：** `USE` 后 `clear_cached_statements` — MySQL 在 PREPARE 时解析未限定表名，连接级 statement cache 会在切库后仍命中旧库
- 单元测试（无 live DB）：标识符 quoting、`USE` SQL 构造、trait wiring、同库 no-op
- 命令：`cargo test -p datazen --lib mysql::tests` — 12 passed（含既有 sync mysql adapter tests）
- **Gated live IT**（补 F1-LIVE-006）：`src-tauri/tests/mysql_use_database.rs`
  - 无 `TEST_MYSQL_*`（process env 或仓库根 `.env`）时干净 skip；`cargo test -p datazen` 不依赖本机 MySQL
  - 有凭证且可连时：`connect`（无默认库）→ `use_database(A)` → 未限定名 `users`（含池内多次）→ `use_database(B)` 未限定名应失败 → 切回 A → 非法库 `QueryFailed` / 空名 `InvalidConfig`
  - 为 integration test 可访问驱动，将 `lib.rs` 中 `mod db` 改为 `pub mod db`（与 `pub mod mcp` 同模式）
  - 运行示例：
    ```bash
    TEST_MYSQL_HOST=127.0.0.1 TEST_MYSQL_PORT=3306 TEST_MYSQL_USER=root \
    TEST_MYSQL_PASSWORD= TEST_MYSQL_DATABASE=datazen_test \
    TEST_MYSQL_DATABASE_B=datazen_sync_mysql_tgt \
    cargo test -p datazen --test mysql_use_database -- --nocapture
    ```

### 2026-08-07 — 初始化

- 创建分支与本进度文件
- 明确 F1–F5 范围与测试/提交循环
