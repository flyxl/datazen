# 原生内置驱动（codex/native-drivers 分支）

> 状态：开发中（已 rebase main；审计 P0/P1 已修，未推送）
> 分支：`codex/native-drivers`（独立 worktree：`.worktrees/native-drivers`）
> 基线：脚手架 + 驱动实现 + 设计文档；本轮补 EXPLAIN / TLS / 多库切换对齐

## 一、为什么做

1. **数据库覆盖是核心差距**。竞品 dbx 支持 70+ 数据库（MongoDB、SQL Server、ClickHouse、达梦等），DataZen 目前只有 PostgreSQL / MySQL / MariaDB / SQLite / Redis + 三个插件（kiwi、OLAP、Superset）。企业用户在选型时"支持的数据库数量"几乎是决定性因素。
2. **dbx 的路线是"agent 进程外置 + 按需下载"**。用户明确要求：只支持 dbx 里 **Rust 原生（进程内）** 的那部分驱动，并且做成 **builtin 驱动而非插件**——即直接编译进主程序，不引入 JRE/外部进程，不依赖插件下载。
3. **目标**：以最低的架构成本把内置数据库类型从 5 个扩到 22 个，同时保持进程内连接的性能与简单性，包体增长控制在可接受范围（见下）。

## 二、做了什么

### 2.1 新增 17 个内置驱动

**全新实现（11 个）**

| 类型 | 实现方式 | 说明 |
|------|---------|------|
| `mongodb` | mongodb crate | JSON 命令式查询（pipeline/find/insert/update/delete），DriverCategory=Document |
| `sqlserver` | tiberius | 原生 TDS 协议，元数据来自 sys.* / INFORMATION_SCHEMA |
| `clickhouse` | reqwest HTTP | HTTP 接口 + `default_format=JSON`，元数据来自 system 表 |
| `duckdb` | duckdb crate（bundled） | 嵌入式，文件 / 内存模式，Arrow schema + ValueRef 解码 |
| `elasticsearch` | reqwest REST | `/_sql` 翻译 SQL，索引即"表"，mapping 解析字段 |
| `rqlite` | reqwest HTTP | `/db/query` + `/db/execute`（SQLite 语法） |
| `turso` | reqwest HTTP | libSQL sqld `/v2/pipeline` |
| `influxdb` | reqwest HTTP | v1 query API（SHOW DATABASES/MEASUREMENTS） |
| `victoriametrics` | reqwest HTTP | PromQL `/api/v1/query`，指标名即"表" |
| `hbase` | reqwest REST | 表/列族浏览 + `scan <table>` 查询 |
| `vector` | reqwest REST | Qdrant 集合浏览 + JSON 检索命令 |

**协议复用（6 个）**——通过 `ReuseDriver` 委托包装现有驱动：

| 类型 | 复用 |
|------|------|
| `doris`、`starrocks`、`manticore`、`ob_oracle` | MySQL 驱动（MySQL 线协议兼容） |
| `questdb`、`cloudberry` | PostgreSQL 驱动（PG wire 协议） |

### 2.2 架构改动

- `src-tauri/src/db/reuse.rs`：`ReuseDriver` 委托包装器（driver_type 覆盖 + 全方法委托）。
- `src-tauri/src/db/http_support.rs`：HTTP 驱动共享的客户端构建（超时、Basic 认证走 default headers）、URL 拼接、JSON→Value 转换。
- `src-tauri/src/db/registry.rs`：`BUILTIN_TYPES` 扩到 22 个；`register_builtin_or_plugin` 增加各类型分支。
- 每个驱动独立模块，实现 `DatabaseDriver` trait（connect / test / schema / query / execute 等）。
- 前端：`src/types/index.ts` 的 `BuiltinDatabaseType` 联合类型、`src/lib/databaseTypes.ts` 的 `DB_REGISTRY` 元数据（端口、表单、方言、能力）、`src/lib/sqlDialects/extra.ts` 新增 6 个方言族（sqlserver / clickhouse / duckdb / elasticsearch / mongodb / generic）。

### 2.3 各驱动查询/浏览能力说明

- **标准 SQL 引擎**（SQL Server、ClickHouse、DuckDB、Doris/StarRocks/Manticore/OceanBase、QuestDB/Cloudberry、RQLite/Turso）：完整查询 + 表/字段浏览。
- **文档/搜索引擎**（MongoDB、Elasticsearch、HBase、Vector）：提供表（集合/索引/表/集合）浏览与简化查询；MongoDB 用 JSON 命令，ES 用 SQL 翻译 API，HBase 用 `scan`，Vector 用 JSON 检索命令。
- **时序/指标**（InfluxDB、VictoriaMetrics）：测量点/指标名浏览 + 各自查询语法。

### 2.4 验证结果

- `cargo check -p datazen --lib`：0 错误（仅 sqlx 未来兼容性警告）。
- `cargo test -p datazen --lib`：**277 passed**。
- 前端 `tsc --noEmit`：0 错误。
- Vitest：**13 passed**（databaseTypes / dialects 等）。
- 按 CI 参数出 release 包（`plugins=none`，aarch64-apple-darwin）：DMG **19MB**（v0.0.8 基线 9.4MB），DataZen.app **48MB**，主二进制 **47MB**。

## 三、还没做（已知边界）

### 3.1 功能边界（v1 有意为之）

1. **冒烟/集成测试**：DuckDB 已有进程内冒烟（query/explain/PK）；MongoDB / SQL Server / ClickHouse 仍需真实实例。
2. **事务**：HTTP 类驱动无事务；SQL Server 驱动也**未接通** `begin_transaction/commit/rollback`（tiberius 单连接，留待后续）。
3. **EXPLAIN**：ClickHouse / DuckDB / RQLite / Turso / SQL Server（`SHOWPLAN_TEXT`）已接通。
4. **MongoDB 没有专属 document 视图**：前端目前只有 `sql` / `keyvalue` 两种 connectionView，MongoDB 暂用 SQL 视图 + JSON 命令，交互不如原生 document 浏览器。
5. **Schema 浏览精度**：
   - Elasticsearch mapping 只解析顶层字段；
   - InfluxDB / VictoriaMetrics / HBase / Vector 的"表结构"是简化视图（DDL 无意义，返回空）；
   - SQL Server 主键已填充；索引/外键仍空。
6. **连接表单高级字段**：部分新类型前端表单仍走 `standard`。MongoDB / ClickHouse / SQL Server 的 SSL 已按 `ssl_mode` 处理；其余 HTTP 驱动走默认 http/https 判断。SQL Server / ClickHouse 已接通 `use_database` 多库切换。
7. **驱动分发**：全部编译进包（用户明确要求 builtin），**没有** dbx 那种"按需下载驱动 + JRE 管理"机制；代价是包体翻倍（9.4MB → 19MB DMG）。如果后续要控制体积，可以考虑把 HTTP 类驱动保留内置、把 mongodb/tiberius/duckdb 这类重依赖做成可选 feature。
8. **未做 E2E**：没有 WebdriverIO 用例覆盖新类型的连接/浏览流程。

### 3.2 工程遗留

- 未推送远程分支（`codex/native-drivers` 目前只在本地 worktree）。
- 各 HTTP 驱动存在重复样板（连接池 map、query_multi 包装），后续可抽公共 `HttpSqlDriver` 基类。
- DuckDB / SQL Server 已填主键，索引/外键仍空。
- `docs/competitive-comparison-dbx.md`（竞品分析）已入库，但未链接到文档索引。

### 3.3 建议的下一步

1. 为 MongoDB / SQL Server / ClickHouse 补真实实例冒烟（DuckDB 已有）。
2. 实现 SQL Server 事务。
3. MongoDB document 视图（前端新增 `connectionView: 'document'`）。
4. 抽公共 HTTP 驱动基类，消除样板。
5. 评估把重依赖拆成可选 feature，控制包体。
6. 推送分支并开 PR（已 rebase main；本地 `cargo test --lib` 282 / vitest 通过）。
