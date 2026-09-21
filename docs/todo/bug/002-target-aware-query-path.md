# BUG-004：多库场景下"只能看到第一个库的数据" / MySQL 报 `1046 No database selected`

> 状态：**已修复（实库回归通过）** · 关联：[RFC 数据库/Schema 维度契约改造](../../architecture/rfc/schema-dimension-contract.zh-CN.md) §13.4 · 同源缺陷：[001](001-pg-table-data-empty-cells.md)

## 0. 修复摘要

第一轮改造（[001](001-pg-table-data-empty-cells.md) / RFC ①–④）把**元数据**读取改成了显式
`(database, schema)` 参数，但**数据**路径仍然调用 `driver.query(handle, sql)` 这类**不带目标**的签名：

- **PostgreSQL** 的 `query_impl` 里是 `Self::get_pool(&pools, handle)` —— 永远取句柄自己那个库的池，
  于是浏览第二个库时读到的仍是第一个库（或读不到表）；
- **MySQL** 的 `qualify_sql_target` 本来就会把库名内联成 `` `db`.`t` ``，但数据路径**根本没调用它**，
  于是未限定表名直达引擎 —— 连接没选默认库时直接 `1046 (3D000): No database selected`。

两者**同根**：目标没有到达驱动的查询路径。修复不是逐点打补丁，而是在驱动契约上补一组
**目标感知方法**，让"目标"成为查询调用的第一等参数。

## 1. 现象

用户在连接「本地 PostgreSQL」下浏览 `winamz_site_medusa`：

| # | 现象 | 复现路径 |
| --- | --- | --- |
| 1 | PG 只能看到第一个库的数据 | 连接配置指向库 A，在连接树里点开库 B 的表 → 数据网格显示的是库 A 的内容（或空） |
| 2 | MySQL 直接报 `Query failed: error returned from database: 1046 (3D000): No database selected` | 连接未指定默认库时打开任意表 → 数据网格直接报错 |
| 3 | 连接树上的 database 节点不显示打开状态 | 无法判断哪些库当前持有连接、哪些可以被"关闭数据库连接"释放 |

现象 1 与 2 是**同一个缺陷的两个方言表现**：目标丢失后，PG 静默读错库，MySQL 则硬失败。

## 2. 根因

### 2.1 元数据路径已修，数据路径未修

第一轮只改了元数据方法签名：

```rust
async fn get_tables(&self, handle, database: &str, schema: Option<&str>) -> ...;
async fn get_columns(&self, handle, table, database: &str, schema: Option<&str>) -> ...;
```

而数据方法**保持原样**：

```rust
async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError>;
async fn query_multi(&self, handle, sql, limit) -> ...;
async fn execute(&self, handle, sql) -> ...;
```

于是 `src-tauri/src/services/query_executor.rs::get_table_data` 明明已经拿到了
`database` + `schema`（`commands/schema.rs::get_table_data_impl` 解析后传下来的），
却在这一行把目标丢掉：

```rust
// 修复前
let (count_res, data_res) = tokio::try_join!(
    driver.query(handle, &count_sql),   // ← 目标在这里丢失
    driver.query(handle, &data_sql),
)?;
```

### 2.2 为什么两个方言表现不同

- **PostgreSQL**：引擎**不支持**跨库引用（`ERROR: cross-database references are not implemented`），
  所以 `qualify_sql_target` 故意**不内联**库名，库维度只能靠**选池**。而 `query` 无从知道目标，
  只能回落到 `Self::get_pool(&pools, handle)` —— 句柄自己的库。
- **MySQL**：引擎**支持** `` `db`.`t` ``，`qualify_sql_target` 也实现正确，
  但 `query` 不会去调用它。未限定表名 + 连接无默认库 = `1046`。

**结论**：这不是某个调用点写错了，而是契约缺了一个维度。逐点修 `get_table_data` 只能治
数据网格，导出 / 同步 / 传输 / 备份 / 结构比对会陆续复现同样的缺陷。

## 3. 方案

### 3.1 驱动契约：目标感知方法（默认实现 = 改写后委托）

```rust
/// 应用驱动自己的目标改写；无目标时原样返回。
fn qualified_sql(&self, sql: &str, target: SqlTarget<'_>) -> String;

/// 带显式目标执行。默认实现改写后委托给无目标版本 ——
/// 因此"连接不绑定单库、只靠内联限定名"的驱动（MySQL 家族）无需任何改动即被修好。
async fn query_at(&self, handle, sql, target) -> Result<QueryResult, DriverError>;
async fn query_multi_at(&self, handle, sql, limit, target) -> ...;
async fn execute_at(&self, handle, sql, target) -> ...;
async fn query_with_params_at(&self, handle, sql, params, target) -> ...;
async fn query_stream_at(&self, handle, sql, limit, target, on_event) -> ...;
```

选**新增方法 + 默认实现**而不是改现有签名：宿主有 79 处 `.query(`/`.execute(` 调用点、
18 个驱动实现，改签名是纯机械的大面积重写；新增方法让**只有真正知道目标的调用点**需要更新，
其余驱动零改动。

### 3.2 PostgreSQL 覆写：按库选池

```rust
async fn query_at(&self, handle, sql, target) -> ... {
    self.ensure_transaction_reaches(handle, target).await?;   // 事务一致性
    let sql = self.qualified_sql(sql, target);                 // 内联 schema
    let pool = self.resolve_statement_pool(handle, target).await?;  // 库 → 池
    self.query_on(handle, &sql, &pool).await
}
```

### 3.3 事务与目标的一致性（新增语义）

PG 事务绑定**单条连接**（即句柄自己的库）。在事务内请求**其他库**：

- 返回 `DriverError::TransactionError`，而不是静默读到错误的 catalog；
- **没有**事务时，同一条语句走目标库自己的池，**不报错**。

> 这一点在实库测试里被真实抓到过：初版守卫漏了"是否真的有事务"判断，
> 导致无事务的跨库读取也被拒绝。实库回归是必要的，纯单测不会暴露。

### 3.4 宿主调用点透传目标

| 路径 | 改动 |
| --- | --- |
| 数据网格 | `services/query_executor.rs::get_table_data` → `query_at`（count + data 两路） |
| SQL 编辑器 / 流式 | `driver-api::execute_standard_sql_command` → `query_multi_at` / `execute_at`（目标来自请求的 `database`/`schema`） |
| 表导出 | `commands/export.rs`：`ExportTablesRequest` 新增 `database` / `schema`，`TableExportInput` 新增**逐表** `schema` → `query_stream_at` |
| 数据同步 | `commands/sync/{compare,exec,keyset_source}.rs` → `query_at` / `query_with_params_at`；IPC 新增 `target_schema` |
| 数据传输 | `data_transfer/{execute,structure}.rs` → `query_at` / `execute_at`（源/目标各自的目标） |
| 备份恢复 | `commands/backup.rs` → `execute_at` |
| 结构比对部署 | `schema_diff/deploy.rs` + `TransactionScope::begin_at` → `execute_at`；IPC 新增 `target_database` / `target_schema` |

### 3.5 未知库必须报错（顺带修正）

MySQL 的 `get_tables` 对不存在的库此前返回**空列表**（`information_schema.TABLES` 查不到），
与 PG 的报错行为不一致 —— "库不存在"和"库是空的"长得一模一样。现在 `TABLES` 为空时回查
`information_schema.SCHEMATA`，未知库报 `Unknown database '{db}'`。

### 3.6 连接树显示库打开状态（现象 3）

- 驱动契约新增 `open_databases(handle) -> Vec<String>`（默认 `Ok(vec![])`）：
  PG = 句柄自己的库 + 缓存的外库池；MySQL = `active_databases`；其余默认空。
- 新增 IPC `get_open_databases(dbSessionId)`。
- 前端 `useNavigatorDbState` 维护 `openDbs`，在**读表之后**、**连接刷新之后**重新拉取；
  "关闭数据库连接" / "删除数据库" 走 `clearDbLocalCache`，**立即**把该库从打开集合移除。
- 数据库节点渲染 `[data-db-open="true|false"]` 小圆点（实心=打开，空心=已关闭）。
- **驱动不上报（`null`）时不渲染任何标记**，而不是谎称"已关闭" —— 否则等于替驱动宣称
  一个它从未提供的事实。

## 4. 验证

### 4.1 实库门控测试

| 测试 | 覆盖 |
| --- | --- |
| `packages/drivers/postgres/tests/postgres_cross_database.rs` | 带目标读外库成功；**同一条语句无目标读失败**；事务内跨库报 `TransactionError`；`open_databases` 随关闭收敛且不影响句柄自己的库 |
| `packages/drivers/mysql/tests/mysql_cross_database.rs::connection_without_a_default_database_needs_an_explicit_target` | **无默认库**连接下未限定语句复现 `1046`，带目标语句成功 |
| `packages/drivers/mysql/tests/mysql_cross_database.rs::cross_database_reads_use_qualified_names_not_use` | 限定名跨库读；未知库报错；`qualified_sql` 确实内联了库名 |

两个测试都改成**运行时发现 fixture**（"A 有、B 没有的任意一张表"），
不再依赖 `users` 这张本地不存在的表，因此在本机可真实执行而不是静默 skip。

本机实跑（PG 17.9 @ `127.0.0.1:5432`、MySQL 8 @ `127.0.0.1:3306`）均通过。

### 4.2 单测 / 类型检查

| 范围 | 结果 |
| --- | --- |
| `cargo test -p datazen --lib` | 1463 passed / 0 failed / 3 ignored |
| `datazen-driver-api` | 126 passed |
| `datazen-driver-postgres` | 105 passed |
| `datazen-driver-mysql` | 86 passed |
| `npx vitest run` | 427 files / 4398 tests passed |
| `npx tsc --noEmit` | clean |
| `cargo fmt --all -- --check` / `prettier --check` | clean（仅 gitignored 的 `driver_init.rs` 例外） |

## 5. 教训

1. **契约缺维度时，症状会按方言分裂**：PG 静默读错库、MySQL 硬报错。
   从症状反推会得出两个不相干的 bug；从"目标在哪里丢失"反推才是同一个根因。
2. **"元数据路径修好了"不等于"数据路径修好了"**：第一轮验收只覆盖了表结构 / 列，
   数据读取从未进入验收范围，于是缺陷完整地活到了第二轮。
3. **默认实现决定修复半径**：把 `*_at` 做成"改写后委托"的默认方法，
   让 MySQL 家族 18 个驱动零改动即被修好；只有真正需要选池的 PG 需要覆写。
4. **实库测试必须真的会跑**：门控测试如果依赖本地不存在的 fixture，会长期静默 skip，
   等于没有测试 —— 这也是本节两个测试改成运行时发现 fixture 的原因。

## 6. E2E 定位到的真正根因：**前端在读取路径上丢掉了 schema**

§1–§5 修的是"目标没有到达**驱动**"。跑 E2E 后发现还有一层更靠前的问题：
**目标没有到达 host** —— 前端在三个读取路径上把 `schema` 整个丢掉了，
于是 host 只能回落到**连接配置里的 schema**，而表和这个 schema 根本不在一个命名空间。

### 6.1 复现（E2E `table-data.ts`，`db` suite）

E2E 的连接由 `e2e/run.mjs` 播种，带 `E2E_WORKER_SCHEMA=e2e_worker_0`，
即连接配置里写着 `schema = e2e_worker_0`；而 spec 用**未限定名**建表，
表实际落在 `public`。加日志后一目了然：

```
get_table_schema target … requested_schema=None  resolved_schema=Some("e2e_worker_0")   ← 找不到
get_table_data   target …                        resolved_schema=Some("public")          ← 找得到
```

同一张表、同一个库，两条路径解析出**不同的 schema** —— 因为其中一条根本没把
schema 传下来。这正是用户报告的三个现象：

| 用户现象 | 对应路径 | 丢在哪 |
| --- | --- | --- |
| 表有数据但列值全空 / 显示不出来 | 数据网格 `get_table_data` | `tableDataStore` 调用时没传 `schema` |
| 表结构里没有 columns | 表结构 tab | `metadataCache` 持有 `session.schema` 却从不往下传 |
| ER 图只有表名没有列 | ER 图 | `get_er_data` 调用时没传 `schema` |

### 6.2 修复

| 位置 | 改动 |
| --- | --- |
| `src/stores/tableDataStore.ts` | `getTableData({ …, schema: context.schema ?? null })` |
| `src/lib/relationMetadata/metadataCache.ts` | `loadTableSchema(dbSessionId, table, database, session.schema)`（并放宽 deps 签名） |
| `src-tauri/src/commands/schema.rs` | `get_table_data` 也识别表名内嵌 schema（`sales.orders`），与其余元数据路径一致 |

**核心原则**：schema 是**关系的身份**，不是"可选的提示"。
凡是"读某张表"的调用，schema 必须随关系一起传；一旦省略，host 只能用连接级
默认值去猜，而猜错时**报错是静默的**（表被当成不存在）。

### 6.3 验证

- `table-data.ts`：**11 个失败 → 16 个全部通过**（`multi-database` 6 passed、`mysql` 21 passed 不变）。
- 新增单测：`tableDataStore` 断言 schema 随分页一起保持；`metadataCache` 4 处断言改为
  校验第 4 个参数（schema）确实被传递。
- `er-diagram.ts` 仍失败，但原因**无关 schema**：面板本身打不开
  （`get_er_data` IPC 返回 7 张表正常，ER-001 通过）。该失败在本轮改动**之前**
  的提交 `7528e620` 上可原样复现，属于既有缺陷。

### 6.4 教训

1. **"默认值"会把错误变成静默**：`config.schema` 作为兜底看似友好，实际让"读错命名空间"
   和"表不存在"长得一样。兜底应当只用于**确实没有更好信息**时，而不是替代调用方已知的事实。
2. **A/B 是唯一能证明"不是我弄坏的"的方式**：`table-data.ts` 的失败在改动前后**逐条一致**
   （相同 spec、相同断言、相同元素超时），在 `7528e620` 上复现后即可确认为既有缺陷，
   避免把时间花在错误的方向上。
3. **日志缺失本身就是缺陷**：`get_table_data` 原先不记录解析后的 `(database, schema)`，
   导致"表不存在"这条报错无法区分"目标错了"和"真的没有"。补上后 10 分钟定位根因。
4. **E2E 的价值在"环境恰好和用户一样"**：连接带 schema、表在 public —— 这个组合单测不会构造，
   而它正是用户真实环境的样子。
