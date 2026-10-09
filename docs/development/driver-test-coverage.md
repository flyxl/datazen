# 驱动功能测试覆盖矩阵

> 本文盘点各个 path driver crate 中已有的测试资产，用来判断“某项驱动功能是否有测试证据”，不是代码覆盖率报告，也不代表本次已运行全部测试。驱动专属测试应落在 `packages/drivers/<id>/`；Host 通用 UI E2E 和 `driver-api` 的共享实现测试不计作某个驱动 crate 的直接覆盖。本文按 2026-09-30 当前 checkout 中的代码和测试文件整理。

## 功能点范围

| 功能面 | 检查点 |
|---|---|
| 连接与会话 | 配置解析、认证/TLS、连接失败和超时、数据库/会话选择、断线与错误恢复。 |
| 查询与结果 | 原生 SQL/命令、参数绑定、流式/分页结果、多结果集、取消、驱动类型到 DataZen 值类型的转换及错误路径。不同模型驱动按其原生命令语义评估。 |
| Schema Tree 基础目录 | 数据库/namespace、schema（若有）、表/集合/index、列或字段元数据；名称引用、空结果、缺失对象和跨 schema/database。 |
| Schema Tree 对象 | 视图、函数、存储过程、触发器、序列、用户类型的列表与 DDL；对象依赖和权限。只统计该驱动实际暴露的对象种类。 |
| 数据与结构操作 | 行/文档/键浏览、分页、读写、列类型/主键/索引/FK 元数据、表结构 DDL 与边界拒绝。Redis 按其键和值编辑器评估。 |
| 管理与运维命令 | 创建/删除 database、schema、user，权限、进程/状态等驱动命令及异常输入。普通 SQL `EXPLAIN` 不视为管理命令。 |
| Schema Diff | 驱动 migration renderer、DDL 生成/验证、危险变更和重放/回滚约束。 |
| Data Sync | 同族比较、driver source/target adapter、apply/revalidate 与执行后的回查旅程。 |
| Data Transfer | 异构 IR 类型映射、预检、DDL/数据写入、失败回滚/继续及执行后的回查旅程。 |
| 驱动 E2E | `packages/drivers/<id>/e2e/` 中有结果断言的真实用户旅程。仅有占位 smoke 文件不算覆盖；Host contract matrix 单独标注。 |

### 标记

| 标记 | 含义 |
|---|---|
| **●** | crate 内有针对该功能的直接行为断言；可能是单测、内存/HTTP mock、嵌入式或 live 集成测试，不等于每种环境均已运行。 |
| **◐** | 只覆盖部分分支、辅助函数/映射/命令注册，或只有共享契约而缺少该驱动自己的完整旅程。 |
| **○** | 代码暴露或实现了该功能，但在该驱动的测试目录中未找到针对该功能的测试证据。 |
| **—** | 当前驱动不提供该功能，或该功能不适用于其数据模型。 |

## 所有 path driver 覆盖矩阵

`packages/drivers/http-support` 是共享 HTTP 支持 crate，不作为数据库驱动列入。表中 E2E 只表示仓库里存在实质性旅程文件，不表示本次已执行成功。

| 驱动 | 连接 | 查询/结果 | 基础目录 | 对象树 | 数据/结构 | 管理/运维 | Schema Diff | Data Sync | Data Transfer | 驱动 E2E |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ClickHouse | ◐ | ● | ● | — | ◐ | — | — | ◐ | ◐ | ○¹ |
| DuckDB | ◐ | ● | ● | ◐ | ● | — | — | ◐ | ◐ | ○¹ |
| Elasticsearch | ○ | ◐ | ◐ | — | ◐ | — | — | ◐ | ◐ | ○ |
| HBase | ○ | ● | ◐ | — | ◐ | — | — | ◐ | ◐ | ○ |
| InfluxDB | ○ | ● | ◐ | — | ◐ | — | — | ◐ | ◐ | ○ |
| MongoDB | ○ | ● | ● | — | ◐ | — | — | ◐ | ◐ | ○¹ |
| MySQL | ◐ | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| PostgreSQL | ◐ | ● | ● | ● | ● | ● | ● | ● | ● | ● |
| Redis | ● | ● | ●² | — | ● | ● | — | — | — | ● |
| rqlite | ○ | ◐ | ◐ | ○ | ○ | — | — | ◐³ | ◐³ | ○ |
| SQLite | ◐ | ● | ● | ● | ● | ◐ | ● | ◐ | ● | ◐⁴ |
| SQL Server | ● | ● | ● | ◐ | ● | ● | ● | ● | ● | ● |
| Turso | ○ | ◐ | ◐ | ○ | ○ | — | — | ◐³ | ◐³ | ○ |
| Vector | ○ | ◐ | ◐ | — | ◐ | — | — | ◐ | ◐ | ○ |
| VictoriaMetrics | ○ | ◐ | ◐ | — | ◐ | — | — | ◐ | ◐ | ○ |

1. ClickHouse、DuckDB、MongoDB 的 E2E 目录目前含 smoke 占位用例，没有实质旅程断言。
2. Redis 的基础目录指 Redis database/key tree，不是 SQL schema/table 树；其专属 UI 和键操作有大量 crate 内测试。
3. SQLite Sync adapter 声明支持 rqlite、Turso 方言，但这两者没有自己的 adapter journey；这里只把共享 adapter 测试记为部分证据，不等同于 rqlite/Turso 驱动已单独验证。
4. SQLite 没有独立 driver E2E 目录；Host contract matrix 覆盖 SQLite 的连接、查询、表数据等通用 journeys，因此这里只记部分。

## Schema Tree 对象种类覆盖

这张表只看各驱动 crate 对实际支持对象的测试证据。**○** 表示实现/命令已接入但没有该驱动自己的对象操作测试；**—** 表示该方言当前未暴露此类对象。基础表/列目录见上一张表。

| 驱动 | View 列表/DDL | Function 列表/DDL | Procedure 列表/DDL | Trigger 列表/DDL | Sequence 列表/DDL | Type 列表/DDL | 依赖/权限 |
|---|---:|---:|---:|---:|---:|---:|---:|
| MySQL | ● | ● | ● | ● | — | — | ● |
| PostgreSQL | ● | ● | ● | ● | ● | ● | ● |
| SQLite | ● | — | — | ● | — | — | — |
| SQL Server | ● | ● | ● | ◐ | ◐ | ◐ | ◐ |
| DuckDB | — | — | — | ○ | ○ | — | — |
| rqlite | ○ | — | — | ○ | — | — | — |
| Turso | ○ | — | — | ○ | — | — | — |

- MySQL 的序列和独立用户类型在当前对象目录实现中不提供；SQLite 的函数、存储过程、序列、权限目录也不提供。
- PostgreSQL 的对象 journey 在真实 Schema Tree 中覆盖视图、函数、过程、触发器、序列、enum 和 domain；并对视图、序列及自定义类型 DDL、schema 归属、异常查询和临时 schema 清理做断言。驱动级依赖/权限测试仍由目录集成测试覆盖。
- SQL Server live 测试会执行 view、function、procedure、trigger、sequence、type 的对象列表 SQL，并覆盖视图/例程目录与部分 DDL；但每种对象的 `get_object_ddl`、依赖、权限结果尚未形成对称的完整断言，因此标为部分。
- DuckDB、rqlite、Turso 的命令定义/共享 SQL 路径已经接入，但本驱动 crate 没有覆盖各自 view/trigger/sequence 列表、DDL 返回和缺失对象边界的完整测试。

## 代表性测试证据

以下路径列出各驱动现有测试覆盖的典型功能，不是完整测试文件清单。

| 驱动 | 测试证据 |
|---|---|
| ClickHouse | [`src/clickhouse.rs`](../../packages/drivers/clickhouse/src/clickhouse.rs) 覆盖数据库作用域、表/列目录和 schema 参数；[`tests/http_query_wiremock.rs`](../../packages/drivers/clickhouse/tests/http_query_wiremock.rs) 覆盖 HTTP 查询/流；[`src/structure.rs`](../../packages/drivers/clickhouse/src/structure.rs) 覆盖 DDL 计划。E2E smoke 当前是占位。 |
| DuckDB | [`tests/duckdb_embedded_smoke.rs`](../../packages/drivers/duckdb/tests/duckdb_embedded_smoke.rs) 验证内存连接、查询和 schema；[`src/structure.rs`](../../packages/drivers/duckdb/src/structure.rs) 覆盖结构 DDL；Sync adapter 有单测和 smoke。E2E smoke 当前是占位。 |
| Elasticsearch | [`src/elasticsearch.rs`](../../packages/drivers/elasticsearch/src/elasticsearch.rs) 覆盖游标流、类型映射、namespace/schema 行为和命令 dispatch；[`src/sync_adapter.rs`](../../packages/drivers/elasticsearch/src/sync_adapter.rs) 覆盖传输映射。没有 driver E2E。 |
| HBase | [`src/hbase.rs`](../../packages/drivers/hbase/src/hbase.rs) 覆盖 scanner 分页、结果解码、schema 行为和类型映射；[`src/sync_adapter.rs`](../../packages/drivers/hbase/src/sync_adapter.rs) 覆盖 adapter 映射。没有 driver E2E。 |
| InfluxDB | [`src/influxdb.rs`](../../packages/drivers/influxdb/src/influxdb.rs) 覆盖 JSON 结果流、类型映射和 namespace 行为；[`src/sync_adapter.rs`](../../packages/drivers/influxdb/src/sync_adapter.rs) 覆盖 adapter。没有 driver E2E。 |
| MongoDB | [`src/mongodb.rs`](../../packages/drivers/mongodb/src/mongodb.rs) 覆盖 BSON 映射、collection metadata、JSON query/stream；[`ui/__tests__/mongodbFind.test.ts`](../../packages/drivers/mongodb/ui/__tests__/mongodbFind.test.ts) 覆盖 Find UI；Sync adapter 有 smoke。E2E smoke 当前是占位。 |
| MySQL | [`tests/schema_objects_sql.rs`](../../packages/drivers/mysql/tests/schema_objects_sql.rs)、[`tests/schema_dependency_catalog.rs`](../../packages/drivers/mysql/tests/schema_dependency_catalog.rs) 覆盖对象 SQL、view/routine/FK 依赖；[`src/structure.rs`](../../packages/drivers/mysql/src/structure.rs) 与 [`tests/migration_bound_writes.rs`](../../packages/drivers/mysql/tests/migration_bound_writes.rs) 覆盖结构与迁移写入；[`e2e/schema-tree-objects.ts`](../../packages/drivers/mysql/e2e/schema-tree-objects.ts) 覆盖表关系、视图、函数、过程、触发器、视图定义和清理，其他 `e2e/` journeys 覆盖 Sync/Transfer。 |
| PostgreSQL | [`tests/schema_objects_sql.rs`](../../packages/drivers/postgres/tests/schema_objects_sql.rs)、[`tests/schema_dependency_catalog.rs`](../../packages/drivers/postgres/tests/schema_dependency_catalog.rs)、[`tests/schema_foreign_key_introspection.rs`](../../packages/drivers/postgres/tests/schema_foreign_key_introspection.rs) 覆盖对象、依赖和 FK；[`e2e/schema-tree-objects.ts`](../../packages/drivers/postgres/e2e/schema-tree-objects.ts) 覆盖关系表、视图、函数、过程、触发器、序列、enum/domain、定义打开、异常查询和临时 schema teardown；[`tests/migration_bound_writes.rs`](../../packages/drivers/postgres/tests/migration_bound_writes.rs) 与其他 `e2e/` journeys 覆盖迁移/Sync/Transfer。 |
| Redis | [`tests/tree_contract_tester.rs`](../../packages/drivers/redis/tests/tree_contract_tester.rs)、[`tests/tree_scan_budget.rs`](../../packages/drivers/redis/tests/tree_scan_budget.rs) 覆盖 key tree；[`ui/__tests__/keyTreeJourney.test.tsx`](../../packages/drivers/redis/ui/__tests__/keyTreeJourney.test.tsx)、[`ui/__tests__/redisConsole.test.ts`](../../packages/drivers/redis/ui/__tests__/redisConsole.test.ts) 覆盖专属 UI；[`e2e/redis.ts`](../../packages/drivers/redis/e2e/redis.ts) 覆盖 Items、命令行、Monitor 和 Pub/Sub 的真实旅程。 |
| rqlite | [`src/rqlite.rs`](../../packages/drivers/rqlite/src/rqlite.rs) 目前主要覆盖 metadata SQL 选择/schema 参数拒绝和结果解码，未找到对象列表/DDL或数据修改的 crate 内测试。 |
| SQLite | [`tests/schema_object_commands.rs`](../../packages/drivers/sqlite/tests/schema_object_commands.rs)、[`tests/schema_objects_sql.rs`](../../packages/drivers/sqlite/tests/schema_objects_sql.rs) 覆盖 view/trigger；[`tests/schema_rebuild_journey.rs`](../../packages/drivers/sqlite/tests/schema_rebuild_journey.rs)、[`tests/migration_transfer_journey.rs`](../../packages/drivers/sqlite/tests/migration_transfer_journey.rs) 覆盖结构重建和传输；ADB 命令见 [`src/adb.rs`](../../packages/drivers/sqlite/src/adb.rs)。 |
| SQL Server | [`tests/live_connection.rs`](../../packages/drivers/sqlserver/tests/live_connection.rs)、[`tests/live_schema_metadata.rs`](../../packages/drivers/sqlserver/tests/live_schema_metadata.rs)、[`tests/live_admin_and_sync.rs`](../../packages/drivers/sqlserver/tests/live_admin_and_sync.rs)、[`tests/live_write_and_ddl.rs`](../../packages/drivers/sqlserver/tests/live_write_and_ddl.rs) 覆盖连接异常、目录、对象、管理命令、分页、写入和 DDL；[`e2e/sqlserver-live-ui.ts`](../../packages/drivers/sqlserver/e2e/sqlserver-live-ui.ts)、[`e2e/sqlserver-metadata.ts`](../../packages/drivers/sqlserver/e2e/sqlserver-metadata.ts) 以及 `sqlserver-schema-diff.ts`、`sqlserver-data-sync.ts`、`sqlserver-data-transfer.ts` 覆盖应用旅程。 |
| Turso | [`src/turso.rs`](../../packages/drivers/turso/src/turso.rs) 有少量结果解码和 metadata SQL 测试；未找到本驱动的 Schema Tree 对象操作、写入、迁移旅程或 driver E2E。 |
| Vector | [`src/vector.rs`](../../packages/drivers/vector/src/vector.rs) 覆盖向量类型、namespace、结果解码和 literal；[`src/sync_adapter.rs`](../../packages/drivers/vector/src/sync_adapter.rs) 覆盖部分映射。没有 driver E2E。 |
| VictoriaMetrics | [`src/victoriametrics.rs`](../../packages/drivers/victoriametrics/src/victoriametrics.rs) 覆盖指标类型、namespace 和结果解码；[`src/sync_adapter.rs`](../../packages/drivers/victoriametrics/src/sync_adapter.rs) 有映射测试。没有 driver E2E。 |

## 结论与解释

目前不能说“各驱动的测试已经覆盖了各自全部功能”。MySQL、PostgreSQL、SQLite、SQL Server 的关系型目录、结构与对象能力覆盖相对完整，但仍有类型/对象 DDL 等缺口；Redis 的专属 key tree、Workbench、命令台和主要 UI 路径覆盖最深入。多数可选驱动的测试以类型转换、SQL/命令构造、mock 或 adapter helper 为主，缺少连接到真实服务后的目录操作、异常路径和完整用户旅程。

特别是 Schema Tree，以下差异不能被“driver command 已注册”掩盖：

- 通用 Host 测试只能证明宿主树组件或 IPC 编排，不证明每个驱动的目录 SQL 可执行、对象字段可解析或 DDL 正确。
- MySQL、PostgreSQL、SQLite 和 SQL Server 有直接的驱动级对象测试，但对象种类并不对称；PostgreSQL Type、SQL Server trigger/sequence/type 的 DDL/依赖边界仍应补齐。
- DuckDB、rqlite、Turso 暴露了部分对象命令或共享 SQLite 方言查询，但缺少各自 crate 中的对象树操作测试。跨驱动共享 SQL 生成测试不能替代这些驱动的命令执行测试。
- ClickHouse、Elasticsearch、HBase、InfluxDB、MongoDB、Vector、VictoriaMetrics 的目录模型并非完整 SQL routine tree；应测试它们实际提供的 database/namespace、table/index/collection/measurement、字段或列元数据和特殊名称/错误处理，而不是机械补齐 SQL Server 对象种类。

本矩阵统计的是测试是否存在和覆盖到哪里，不表示缺口等于产品实现必然有 bug，也不以测试文件数量推算百分比。新增 driver 能力、命令或对象类型时，应同步更新对应矩阵格及代表性测试证据；具体新增测试仍放在驱动 crate 内。
