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
| F2 | 前端会话级多库（mysql/mariadb）：registry、schemaStore、SchemaTree、QueryPanel、WorkflowPanel + Vitest + E2E spec | ✅ implemented + tested | — |
| F3 | PostgreSQL：`get_tables` 尊重 database + `use_database` + Rust 单元测试 | ✅ implemented（unit + gated live IT） | — |
| F4 | 前端启用 postgresql 多库能力 + Vitest + E2E | ✅ implemented + tested | — |
| F5 | 更新必要文档并提交 | ⬜ pending | — |

## 测试记录索引

| ID | 测试 agent | 结果文件 | 结论 |
|----|------------|----------|------|
| F1 | test-agent (fresh) | [progress-multi-database-session-f1-test.md](./progress-multi-database-session-f1-test.md) | **PASS** |
| F2 | test-agent (fresh) | [progress-multi-database-session-f2-test.md](./progress-multi-database-session-f2-test.md) | **PASS** (Vitest 20/20 + E2E 3/3) |
| F3 | test-agent (fresh) | [progress-multi-database-session-f3-test.md](./progress-multi-database-session-f3-test.md) | **PASS** (unit 5/5 + gated IT skip + live IT) |
| F4 | test-agent (fresh) | [progress-multi-database-session-f4-test.md](./progress-multi-database-session-f4-test.md) | **PASS** (Vitest 21/21 + E2E 3/3) |

## 变更日志

### 2026-08-07 — F4 全量测试通过（fresh test-agent）

- Vitest：`databaseTypes` / `schemaStore` / `SchemaTree.test.tsx` — 21/21 PASS
- E2E：`pnpm e2e:skip-build -- --spec e2e/specs/postgres-multi-database.ts` — 3/3 PASS（webdriver debug binary + 本机 PostgreSQL，9 库可见）
- 报告：[progress-multi-database-session-f4-test.md](./progress-multi-database-session-f4-test.md)

### 2026-08-07 — F4 前端启用 postgresql 多库

- `DB_REGISTRY.postgresql`：`hasMultiDatabase: true`（复用 F2 会话公式与 MultiDatabaseSchemaTree）
- Vitest：`databaseTypes` / `schemaStore` / `SchemaTree` 期望改为 postgresql 走多库路由与 expand→`use_database`
- 新增 `e2e/specs/postgres-multi-database.ts`：无默认库连接 → 多库节点；展开加载表；QueryPanel 选择器；无 PG / `E2E_SKIP_PG=1` 时 skip
- `e2e/helpers.ts`：`createAndConnectPostgreSQL`（`database: ''` 可留空）
- `package.json` `e2e:db` 纳入该 spec

### 2026-08-07 — F3 全量测试通过（fresh test-agent）

- 单元测试：`cargo test -p datazen --lib postgres::tests` — 5/5 PASS
- Gated IT 无 env：干净 skip（`⏭  Skipping postgres_use_database: no TEST_PG_*`）
- Gated IT 有 env：本地 PG `goecoride` / `postgres` — live 全场景 PASS（~0.09s）
- 报告：[progress-multi-database-session-f3-test.md](./progress-multi-database-session-f3-test.md)

### 2026-08-07 — F3 PostgreSQL `get_tables` + `use_database`

- `PostgresDriver`：`get_tables(handle, database)` 连接**命名**库的 catalog（active 命中复用池；否则临时 PgPool），不再忽略 `_database`
- `use_database`：Postgres 无 `USE`，按 handle 存 `connect_configs` + `active_databases`，切库时新建 PgPool 并替换旧池；同库 no-op；空名 / NUL → `InvalidConfig`；连不上目标库 → `QueryFailed`；失败不改 active
- 空 `config.database` 仍连默认 `postgres`（列表库）
- 单元测试（无 live DB）：`validate_database_name`、`resolve_connect_database`、trait wiring、同库 no-op
- 命令：`cargo test -p datazen --lib postgres::tests` — 5 passed
- **Gated live IT**：`src-tauri/tests/postgres_use_database.rs`
  - 无 `TEST_PG_*`（process env 或仓库根 `.env`）时干净 skip
  - 有凭证且可连时：`connect`（无默认库 → postgres）→ `get_tables(A/B)` 分 catalog → `use_database(A)` 未限定 `users`（池内多次）→ `get_tables(B)` 不泄漏 A → 切 B 未限定失败 → 切回 A → 非法库 `QueryFailed` / 空名 `InvalidConfig`
  - 运行示例：
    ```bash
    TEST_PG_HOST=127.0.0.1 TEST_PG_PORT=5432 TEST_PG_USER=goecoride \
    TEST_PG_PASSWORD= TEST_PG_DATABASE=goecoride \
    TEST_PG_DATABASE_B=postgres \
    cargo test -p datazen --test postgres_use_database -- --nocapture
    ```

### 2026-08-07 — F2 全量测试通过（fresh test-agent）

- Vitest：20/20 PASS（`schemaStore` + `databaseTypes` + `SchemaTree.test.tsx`）
- E2E：`pnpm e2e:skip-build -- --spec e2e/specs/mysql-multi-database.ts` — 3/3 PASS（webdriver debug binary + 本机 MySQL）
- 报告：[progress-multi-database-session-f2-test.md](./progress-multi-database-session-f2-test.md)

### 2026-08-07 — F2 测试缺口关闭（E2E + SchemaTree Vitest）

- 新增 `e2e/specs/mysql-multi-database.ts`：无默认库连接 → 多库节点；展开加载表；多库时 QueryPanel 选择器；无 MySQL / `E2E_SKIP_MYSQL=1` 时 skip
- `e2e/helpers.ts`：`createAndConnectMySQL` 使用 `E2E_MYSQL_*`；`database: ''` 可留空
- `package.json` `e2e:db` 纳入该 spec
- 新增 `SchemaTree.test.tsx`：mysql length>1 / ===1 / postgresql → 路由与 expand
- Vitest：20/20 PASS
- E2E `--skip-build`：现有 `target/debug/datazen` 无 `--features webdriver`（4445 未开）；需 `pnpm e2e -- --spec e2e/specs/mysql-multi-database.ts` 或先 webdriver debug build

### 2026-08-07 — F2 前端会话级多库（mysql/mariadb）

- `DB_REGISTRY.mysql` / `mariadb`：`hasMultiDatabase: true`（postgresql 仍关闭，留给 F4）
- `schemaStore`：`isMultiDatabase = hasMultiDatabase && databases.length > 1`；`loadForConnection` 接受 `databaseType`；preferred DB 在列表中则预选，否则 `databases[0]`；`loadTables` 先调 `use_database` 再拉表
- `SchemaTree`：能力标志为真时走 `MultiDatabaseSchemaTree`（含 length===1 仍展示单库节点）
- `QueryPanel`：用 store `isMultiDatabase` 控制库选择器（非仅静态 meta）
- `WorkflowPanel`：同公式（capability && length > 1）；workflow 可指向任意连接，按拉取到的库列表计算
- IPC：新增 `use_database` 命令（`commands/schema.rs` + 前端 `databaseCommands.useDatabase`）
- Vitest：`databaseTypes` mysql/mariadb 标志；`schemaStore` isMultiDatabase / preferred / useDatabase 顺序

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
