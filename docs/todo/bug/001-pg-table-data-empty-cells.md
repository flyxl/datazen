# BUG-003-2：PG 表有数据但单元格全空（显示 42 行，列值全为空）

> 状态：**已修复（待实库回归）** · 关联：[RFC 数据库/Schema 维度契约改造](../../architecture/rfc/schema-dimension-contract.zh-CN.md) · 同源症状见 §6

## 0. 修复摘要

根因是驱动契约把 `database` 维度只给了 `get_tables`，而 `get_columns` / `get_table_schema` /
`get_all_columns` 只能落在**会话当前所在的库**上，于是 host 用 `use database` 去"补"这个维度，
污染共享会话并让空列被缓存 300s。

修复把 `database` + `schema` 变成**所有元数据方法的显式参数**，并彻底删除 `use_database`：
host 不再切换会话，目标随调用传递；驱动自己按显式目标选 pool / 拼限定名。
`SchemaCache` 的键也加上 schema，且空列不缓存。落地状态见 RFC §13。

## 1. 现象

在连接「本地 PostgreSQL」下打开数据库 `winamz_site_medusa` 的某张表（如 `cart_address`）：

| 观察点 | 表现 |
| --- | --- |
| 行数 | **正确**，显示 42 行（与该表真实行数一致） |
| 列头 | **缺失**（没有列名） |
| 单元格 | **全空**，42 行都是空记录 |

即"**行数对、内容全空**"——这个组合很有欺骗性：它看起来像数据读取失败或序列化问题，实际是**列元数据缺失**导致的行映射退化为空对象。

同一连接下的 `datazen_demo` 显示正常；用户反馈 v0.2.1 也"正常"。

## 2. 复现步骤

1. 新建/使用连接「本地 PostgreSQL」（`127.0.0.1:5432`，用户 `postgres`，连接配置的 database 为空）。
2. 连接后**先打开 ER 图**（或直接展开 `winamz_site_medusa` 下的表）。
3. 双击打开 `winamz_site_medusa.public.cart_address`。
4. 观察：42 行、无列头、单元格全空。

**关键点：顺序敏感。** 如果先打开 `datazen_demo` 的表、或先让会话 pin 到 `winamz_site_medusa`，再打开同一张表就正常。这就是"看起来是版本回归 / 某些库坏了"的原因。

## 3. 根因

驱动契约**分裂**（详见 RFC §2.1）：

| 方法 | database 维度 | 说明 |
| --- | --- | --- |
| `get_tables(handle, database)` | **显式参数** | 驱动自己解析（PG 对非当前库临时建 pool）→ 所以表名/表数量**是对的** |
| `get_columns(handle, table)` | **无参数** | 只能落在会话当前连接的那个库上 |

数据网格的取数链路：

```
tableDataStore.getTableData(dbSessionId, table, database)
  → commands/query.rs: get_table_data_impl
      → query_executor.rs:84  schema_cache.get_columns(db_session_id, database, table, driver, handle)
          → cache miss → driver.get_columns(handle, table)          ← 无 database 维度
              → PG: information_schema.columns WHERE table_name = 'cart_address'
                    在会话当前库（如 datazen_demo）里查不到 → 返回 Ok(空列)
          → store_table_schema 以 (conn, 'winamz_site_medusa', 'cart_address') 为键写入空列，TTL 300s
      → build_select_sql：列集为空 → 退化为 `SELECT * FROM ...`   ← 所以行数正确
      → TableDataResult { columns: [], rows: 42 行 }
  → src/lib/loadBatchExportTable.ts:48 rowsToRecords([], rows)
      → 对每行做 `record[col.name] = row[i]`，但 columns 为空 → 每行映射为 {}  ← 所以单元格全空
```

**三个环节叠加才产生这个症状**：列元数据空 → SQL 退化但行数正常 → 行映射产出空对象。

`SchemaCache` 的 TTL 是 300s，且空列条目被写入 `(connection, database, table)` 键，因此一次错误读取会**污染该表后续所有读取**（结构视图、ER 图、数据网格）。

## 4. 证据链

### 4.1 驱动层复现（真实库）

会话连在 `datazen_demo`，直接请求 `winamz_site_medusa`：

```
get_tables(winamz_site_medusa)          = 140            ← 表清单正确（走显式参数）
get_columns("cart_address")  [未 pin]   = Ok, cols=0     ← 静默空列（根因）
use_database("winamz_site_medusa")
get_columns("cart_address")  [已 pin]   = Ok, cols=16
SELECT COUNT(*) FROM cart_address       = 42             ← 与界面显示的 42 行一致
```

### 4.2 应用日志（`{data_dir}/logs/datazen.log.*`）

```
06:42:07.195  get_er_data  database=winamz_site_medusa          ← ER 图先拉取（此时会话还在 datazen_demo）
06:42:12.111  session active database switched  database=winamz_site_medusa
              get_table_schema OK (cache)  table=account_holder  cols=0
              get_table_schema OK (cache)  table=api_key         cols=0
              get_table_data   OK          table=cart_address    rows=42
```

`get_er_data` 的读取发生在会话切换**之前**，且后续 `cols=0` 全部命中缓存（`(cache)` 标记）——完整复现了"污染 + 缓存放大"。

### 4.3 为什么"`datazen_demo` 正常"、"v0.2.1 正常"

两者都是**顺序假象**，不是库差异也不是版本回归：

- `datazen_demo` 正常：读取时会话恰好就在 `datazen_demo` 上；
- v0.2.1"正常"：`git diff v0.2.1..HEAD` 显示 `get_er_data_impl` 与 PG schema 代码**完全一致**（同样缺 pin、同样 `Ok(空)` 分支）。是否正常只取决于"读 schema 时会话当前在哪个库"。

## 5. 影响面

同一根因影响所有"按 database 参数取元数据、但驱动方法无 database 维度"的路径：

| 受影响功能 | 表现 |
| --- | --- |
| 数据网格 | **本问题**：行数对、单元格全空 |
| 表结构视图 | 列不显示（DDL 视图正常，它不走该缓存） |
| ER 图 | 只有表名、无列 |
| 数据导出 / 备份 | `sql_dump` 导出另一个库时产出**空 DDL**（`driver-api/src/sql_dump/dump.rs:202`） |
| AI / NL2SQL | 上下文拿到空列，生成 SQL 质量下降 |
| MCP / Workflow | `get_table_schema` 命令无 database 维度（`schema_catalog_commands.rs:85`） |

## 6. 同源症状（同一次修复覆盖）

| # | 原始描述 | 与本问题的关系 |
| --- | --- | --- |
| 1 | PG 表能看到 DDL，但表结构不显示 columns | 同一空列缓存（DDL 路径不经该缓存，故正常） |
| **2** | **表有数据但显示不出来，42 行、列值全空** | **本文档** |
| 3 | ER 图只显示表名、不显示列 | 同一空列（且 ER 图挂载即拉取，是**最常见的污染触发点**） |

## 7. 修复（已落地）

1. **契约层**：`get_tables` / `get_table_schema` / `get_columns` / `get_all_columns` /
   `dump_*_ddl` 全部增加 `database` + `schema` 显式参数；**删除 `use_database`**，
   `PROTOCOL_VERSION` 3 → 4（硬切）；新增 `has_schema_level()` / `default_schema()` /
   `SchemaScope` / `validate_schema_target()` / `close_database()`。
2. **驱动层**：PG 建立 `(db_session_id, database) → PgPool` 映射（跨库读 = 选 pool，
   每 handle ≤ 8 库 / 每 pool max=2 / LRU 淘汰）；MySQL 全部走 `` `db`.`table` `` 限定名，
   零 `USE`；其余 16 个驱动逐 crate 改造。
3. **缓存层**：`SchemaCache` 的键由 `(conn, database, table)` 变为
   `(conn, database, schema, table)`，且不写入、不命中空列条目（视为 miss）。
4. **驱动守卫**：PG 在 `information_schema` 无行时用 `pg_class` 校验关系是否存在，
   不存在则返回 `DriverError` 而非 `Ok(空)`。
5. **host 兜底**：`commands/schema.rs::resolve_metadata_schema` 为拿不到 schema 的调用方
   （MCP / AI / Workflow）解析"显式 → 表内嵌 → 连接配置 → 驱动约定"。
6. **前端**：所有按表读取都带上该表自己的 schema（来自 `TableInfo.schema`），
   并以运行时能力位 `hasSchemaLevel` 兜底；不再拿 database 当 schema 用。

> 注：修复过程中已先落地两道安全网——PG 的 `pg_relation_exists` 守卫与
> `SchemaCache` 空列不缓存。它们让"静默空"变成"明确报错 + 单次请求失效"，
> 但**根因**仍需上述契约改造才能彻底消除。

## 8. 验收标准

1. `winamz_site_medusa.public.cart_address`：42 行、**16 列**、单元格有值；
2. 同一连接下 `datazen_demo` / `winamz_site_medusa` 交替访问结果稳定，与访问顺序无关；
3. 日志中不再出现 `cols=0 (cache)`，也不再有 `session active database switched`；
4. 跨 schema 同名表（`public.orders` / `other.orders`）列不串；
5. 导出另一个库的 DDL 非空。

参考数据（真实库实测）：`cart_address` = 42 行 / 16 列，
列名 = `id, customer_id, company, first_name, last_name, address_1, address_2, city,
country_code, province, postal_code, phone, metadata, created_at, updated_at, deleted_at`；
`api_key` = 14 列；`winamz_site_medusa` = 140 张表；`datazen_demo` = 7 张表。
