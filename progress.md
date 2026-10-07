# P5 修复轨 `p5-endpoint-overlap` 进度台账

> 分支 `feature/p5-endpoint-overlap`，基线 `codex/p5-integration @ 4b782750ddd1c51f6f75f8275f63dccf4902e8b0`。
> 本文件是交付前临时台账，**验收合并时必须删除**。

## 交付状态（截至 `8d1c65b8f6f72978dad5969a8b9fa4097de0af6b`）

| 项 | 状态 | 证据所在 |
| --- | --- | --- |
| D1 / D2 两处阻塞缺陷 | 已修，两处不可分割 | 「修复」一节 |
| A–E 非回归矩阵（B / C 各有独立用例） | 全绿 | 「A–E 非回归矩阵」一节 |
| 变异验证 7 条（含控制变异 M1） | 全部 KILLED | 「变异验证」一节 |
| 变异结论对当前 HEAD 的有效性 | 前提已证明：4 文件、生产代码 0 改动 | 「变异结论的有效性前提」一节 |
| 门禁（拆分后在 `8d1c65b8f` 重跑） | 12 项 EXIT 逐字记录，首尾取证逐字相等 | 「最终门禁」一节 |
| 告警增减 | **不声称**，只交集合；两侧清单逐字节相同 | 「告警：只比集合」一节 |
| 驱动集 `--drivers=all` | 17 个 crate，断言式枚举 | 「门禁」一节 |
| `job_kernel.rs` 超 800 行 | **已拆**，623 / 134 / 289 | 「job_kernel.rs 拆分」一节 |
| `traits.rs` / `sqlserver.rs` 超 800 行 | **未拆，技术债**（上游既违规，本轨只标注并给拆分计划） | 「行数纪律：违规文件」一节 |
| 唯一红门禁 `postgres_cross_database` | 基线既有失败，非本轨引入 | 「唯一红门禁」一节 |

提交链（老 → 新）：
`653f2347c` → `24f5a8955` → `b00a1b0c2` → `051a228a4` → **`2940e2530`（修复代码）**
→ `e998366a6` → `5824392f0` → **`fc8196128`（`job_kernel.rs` 拆分）** → `8d1c65b8f`（本台账）。

## 本轮：独立验收 TEST_FAILED 的两处阻塞缺陷修复

上一轮（`651a228`→`051a228`）被验收打回，两条阻塞缺陷：

| 编号 | 缺陷 | 性质 |
| --- | --- | --- |
| D1 | 检测键并入了 `Connection(connection_id)`，把**一条保存连接上的两个数据库**误判为端点重叠 | 阻断（合法跨库复制被拒） |
| D2 | 摘要只吃 `config.host`，而 `normalized()` **不解析驱动默认值**（默认值在驱动 crate 连接时解析），「省略 host」与「显式写驱动默认 host」得到两个摘要 | 阻断（真实重叠漏检，可致自覆盖） |

根因判定：`ledger.rs` 的 `services: BTreeMap<ConnectionId, ServiceState>` 回答的是**配额**，
端点重叠检测回答的是**安全**，一个字段兼两职是范畴错误。**缺陷 100% 在消费端**，
生产端 `identify` 的设计是对的，予以保留。

### 修复（两处不可分割）

1. `packages/runtime/src/job/budget.rs`：删除检测键集里的 `Connection` 分量。
   `identity_keys() -> Vec<String>` 收敛为 `identity_key() -> Option<String>`（只取 `service_key`）。
2. `packages/driver-api/src/traits.rs` 新增两个**带默认实现**的 provided 方法
   `default_host() -> Option<&'static str>` / `default_port() -> Option<u16>`；
   `endpoint_identity.rs` 的 `ResolvedLocation::of` 在 `config` 缺省时向驱动查询后填入摘要。

**没有为了让 D1 通过而削弱检测器**：A/C 仍由 `service_key` 相等单独拒绝，
`cm31_same_service_key_on_one_connection_is_rejected` 是专门的反弱化护栏。

### `default_host() == None` 是刻意的 fail-open 选择

`None` 的含义是「**该驱动不施加隐式 host**」，不是「未知」。驱动 crate 才是自身默认值的
权威持有者，宿主若维护一张 per-driver 默认值表就违反 AGENTS.md「宿主不得硬编码驱动行为」。

- 退路（fail-closed 出口）：解析后既无 host 又无 database ⇒ `service_key` 为空串 ⇒
  `identity_key()` 返回 `None` ⇒ `any_unprovable && has_writer` ⇒ 直接拒。
  该分支在 `budget.rs:108-112` **原样保留**。
- 已评估并否决的替代：(i) 三态 `HostDefault` 枚举——要再改 13 个驱动 crate，
  且会让 SQLite/DuckDB/MongoDB/ClickHouse 传输一律 fail-close；(ii) 连接时把解析后的地址回写——
  `connect` 收 `&ConnectionConfig`，`ConnectionHandle` / `ServerInfo` 都不带 host，做不到；
  (iii) 「无 host 且无默认 ⇒ 不可证明」的粗暴规则——会打死所有文件型驱动。

### 机制选择：向驱动查询，而不是在 `establish_connection` 里改配置

驱动默认值只在驱动 crate 连接时才知道，宿主无从推导。在**准入期**查询驱动是外科手术式的：
只有传输 Job 的端点身份看到它，`get_session_config` 的返回、`reviewed::identity` 的输入、
驱动收到的配置对象都不受影响。

### 每个驱动的默认值来自与 `connect` 共用的同一个常量（反漂移）

`default_host()` 的返回值与 `connect` 实际拨号的地址由**同一常量**产生，
并在驱动 crate 内用测试钉死，宿主不参与。

| 驱动 | `default_host()` | `default_port()` | 与 `connect` 共享的常量 | 驱动内测试 |
| --- | --- | --- | --- | --- |
| postgres | `Some("localhost")` | `Some(5432)` | `connection.rs` `DEFAULT_HOST` / `DEFAULT_PORT` | `the_declared_defaults_are_exactly_what_connect_dials` |
| mysql | `Some("localhost")` | `Some(3306)` | `connection.rs` `DEFAULT_HOST` / `DEFAULT_PORT` | `the_declared_defaults_are_exactly_what_connect_dials` |
| redis | `Some("127.0.0.1")` | `Some(6379)` | `connect/mod.rs` `DEFAULT_HOST` / `DEFAULT_PORT` | `the_declared_defaults_are_exactly_what_connect_dials` |
| sqlserver | `None`（host 必填，缺失即报错） | `Some(1433)` | `sqlserver.rs` `DEFAULT_PORT` | `the_declared_default_port_is_exactly_what_connect_dials` |
| clickhouse / duckdb / elasticsearch / hbase / http-support / influxdb / mongodb / rqlite / sqlite / turso / vector / victoriametrics | `None` | `None` | —（沿用 trait 默认 `None`） | — |

sqlite 证据：`packages/drivers/sqlite/src/sqlite.rs` 零 `.host` 引用，
`db_path(config)` 只看 `config.database`；因此 `host || database` 的定位锚定规则必须保留。

## A–E 非回归矩阵

| 场景 | 期望 | 用例 |
| --- | --- | --- |
| A 两个不同保存连接 → 同一物理服务器/库/对象 | REJECT | `cm31_alias_configs_sharing_one_service_are_still_rejected` |
| B 一条保存连接、两个数据库、同名对象 | ACCEPT | `cm31_one_connection_id_across_two_databases_is_allowed` + 宿主 `one_saved_connection_two_databases_is_not_a_self_overlap` |
| C 省略 host vs 显式写驱动默认 host | REJECT | `an_omitted_host_digests_as_the_drivers_declared_default` |
| D 两个不同保存连接 → 同一物理服务器、不同数据库 | ACCEPT | `cm31_same_object_on_two_different_endpoints_is_allowed` |
| E SQL 文件目标无写端点 | ACCEPT（行为不变） | 既有 SQL-file 用例 |

D1 与 D2 之间**没有被迫取舍**：A/C 靠 `service_key` 相等单独拒绝，B/D 靠摘要里的 `database` 区分。

## 已实现的事实（台账删除后仍可从代码与测试读出）

- 端点身份来自 `ConnectionConfig`：`connection_id = config.id`（持久化连接配置 id，喂 §6.2 配额账本），
  `service_key = "data-transfer:" + sha256(物理位置摘要)`（**只**喂安全检测）。
- 物理位置摘要字段集与 `datazen_schema_diff::reviewed::same_endpoint` 对齐
  （driver/host/port/database/schema/options/tunnel，剔除 `user`），`id` 刻意不参与摘要。
- `host`/`port` 进摘要前先经 `ResolvedLocation::of` 解析：config 优先，缺失时向驱动查询。
- `detect_endpoint_overlap` 签名未变（`&[EndpointRef] -> Result<(), JobError>`），无调用方需要改。
- §6.2 一次性全有或全无预算预留保持不变：`ensure_service` 幂等、每个端点按物理端点独立预留、整体全有或全无。
- `sql_file == true` 的目标侧没有写端点（`target_identity: Option::None`），该路径行为不变。

## 变更规模（每行都带状态标识）

| 口径 | 命令 | 结果 |
| --- | --- | --- |
| **提交对提交**（权威） | `git diff --shortstat 4b782750ddd1c 2940e25307746` | **21 files, +2037, −152** |
| 提交 → 工作区（含未提交台账，**无效口径**） | `git diff --shortstat 4b782750ddd1c` | 21 files, +2109, −152 |
| 两者之差 = 未提交的 `progress.md` | `git diff --shortstat 2940e253` | 1 file, +121, −49 |

**曾用错口径并已更正**：早先汇报的 `+2109` 取自单 ref（提交 → **脏工作区**），
把未提交的台账也算进去了。权威值是提交对提交的 **+2037**。
后续所有数字一律标注取数状态。

## 门禁（在干净树重跑；数字见本节末提交）

驱动集 `--drivers=all`（`shasum -a256 drivers-registry.json` =
`8531e125fa9bc0c9fe4249c6403b479b1aae61fdd7932c5eaadb0971b96f7cb8`，
`cargo metadata --no-deps --format-version 1 > /tmp/dz-metadata.json`（EXIT=0）后 node 解析，
计得 **17** 个 `datazen-driver-*` 包：
api / clickhouse / duckdb / elasticsearch / hbase / http-support / influxdb / mongodb /
mysql / postgres / redis / rqlite / sqlite / sqlserver / turso / vector / **victoriametrics**；
`drivers-registry.json` 另有 3 个 git 驱动 `kiwi` / `olap` / `superset`，本机未克隆，
故不是 workspace member。16 个驱动目录 + `packages/driver-api` = 17。

这个「17」不是数出来的，是**断言出来的**。逐字输出：

```
$ HEAD=8d1c65b8f6f72978dad5969a8b9fa4097de0af6b
$ REGISTRY_SHA=8531e125fa9bc0c9fe4249c6403b479b1aae61fdd7932c5eaadb0971b96f7cb8
$ cargo metadata --no-deps --format-version 1 > /tmp/dz-metadata.json ; echo METADATA_EXIT=$?
METADATA_EXIT=0
METADATA_BYTES=132779
DRIVER_CRATE_COUNT=17
IDS=api, clickhouse, duckdb, elasticsearch, hbase, http-support, influxdb, mongodb, mysql, postgres, redis, rqlite, sqlite, sqlserver, turso, vector, victoriametrics
ASSERT_OK contains: victoriametrics, api, sqlserver  missing=none
ASSERT_EXIT=0
```

断言脚本不是 `grep | wc -l`，而是 `node` 读 metadata 后**逐个 id 做成员判定**，
缺任一 id 即 `exit 1`。取样断言 `victoriametrics`（最容易被漏的尾部驱动）、
`api`（`datazen-driver-api`，计数里唯一的非驱动 crate）、
`sqlserver`（本轨 `default_port()` 所在 crate），三条全中。
`REGISTRY_SHA` 与本轨其它记录一致，说明枚举所依据的注册表未被本轨改动。
全量 `--drivers=all` 宿主编译另见「格式与规模纪律」一节（`cargo test -p datazen --lib --no-run` EXIT=0）。

门禁重跑的取证方案（取代此前只记 `HEAD` 的不足）：
`HEAD` 一致**不足以**证明无人改动——此前一轮就出现过 `HEAD` 恒定、
但工作区 sha 与 dirty 文件数在门禁期间变化（20 → 18）的情况。
现采用 **`SOURCE_SHA`**：`git ls-files` 全量 tracked 文件内容求 sha256，
**排除** `src-tauri/Cargo.toml` 与 `Cargo.lock`（这两份是 codegen 注入产物，见下节）。
规则是**注入发生在记录窗口之前**，还原发生在窗口之后；
窗口首尾各记一次 `HEAD` / `TREE` / `SOURCE_SHA`，**必须逐字相等**，否则作废重跑。
不注入驱动的门禁全程 `git status --porcelain` 保持 0。

### 门禁结论（干净树重跑，被门禁提交 `e998366a6fdbd699f908f5f1b79e41fec3bc7932`）

取证首尾（逐字，必须相等）：

```
BEFORE_HEAD=e998366a6fdbd699f908f5f1b79e41fec3bc7932
BEFORE_TREE=46f7daede64e0a661a1ccd46e212dcf85229a7f0
BEFORE_DIRTY=0
BEFORE_SOURCE_SHA=86837cc0460293ee8ef0fe93cb92d240d984fecec6f76e7eabdb21644cbe2f0a
AFTER_HEAD=e998366a6fdbd699f908f5f1b79e41fec3bc7932
AFTER_TREE=46f7daede64e0a661a1ccd46e212dcf85229a7f0
AFTER_SOURCE_SHA=86837cc0460293ee8ef0fe93cb92d240d984fecec6f76e7eabdb21644cbe2f0a
POST_RESTORE_DIRTY=0
```

`HEAD` / `TREE` / `SOURCE_SHA` 三项**逐字相等**。8 次 `PHASE1_DIRTY_AFTER_*` 全为 `0`
（每个非注入门禁跑完立即复查工作区）。注入后 `git status --porcelain` 只有
` M src-tauri/Cargo.toml` 一行，`AFTER_INJECT_*` 三项仍与 BEFORE 相同。

| 门禁命令 | 退出码 | 逐字结论行 |
| --- | --- | --- |
| `cargo metadata --no-deps --format-version 1`（**第一个跑**） | 0 | 见上节 |
| `cargo test -p datazen-runtime` | 0 | lib 412 + 33 个集成二进制，全 ok |
| `cargo test -p datazen-schema-diff` | 0 | `test result: ok. 225 passed; 0 failed; ...` |
| `cargo test -p datazen-data-sync` | 0 | `test result: ok. 176 passed; 0 failed; ...` |
| `cargo test -p datazen-data-transfer` | 0 | `test result: ok. 213 passed; 0 failed; ...` |
| `cargo test -p datazen-driver-postgres` | **101** | lib `ok. 201 passed; 0 failed`；**集成二进制 `postgres_cross_database` 1 failed（见下，已证为基线既失败）** |
| `cargo test -p datazen-driver-mysql` | 0 | lib `ok. 178 passed; 0 failed` + 8 个集成二进制全 ok |
| `cargo test -p datazen-driver-redis` | 0 | lib `ok. 397 passed; 0 failed; 3 ignored` + 5 个集成二进制全 ok |
| `cargo test -p datazen-driver-sqlserver` | 0 | 8 个集成二进制全 ok（lib 见下节计数） |
| `cargo test -p datazen --lib` | 0 | `test result: ok. 1762 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 23.72s` |
| `cargo check -p datazen` | 0 | `warning: \`datazen\` (lib) generated 33 warnings`；`warn_lines=46  warn_files=17`（**仅供对照，判定见「告警」一节**） |
### 最终门禁（`@8d1c65b8f`，拆分提交 `fc8196128` **之后**重跑）

**旧读数不可平移。** `5824392f0` 里的认证门禁对应 `2940e2530` 的树，不含拆分；
拆分动了 `packages/runtime/tests/` 下三个会被编译进测试二进制的文件，
因此在拆分后**整份门禁重跑**，不是只补一个包。

取证首尾（逐字，必须相等）：

```
BEFORE_HEAD=8d1c65b8f6f72978dad5969a8b9fa4097de0af6b
BEFORE_TREE=0e8636c811cb03e6dcd9a5e69e0ec8c8d22e08e2
BEFORE_DIRTY=0
BEFORE_SOURCE_SHA=13876a4e2a246a9ed5dc2e33a124d00610115b119109eba11e29bbd47184593e
AFTER_INJECT_HEAD=8d1c65b8f6f72978dad5969a8b9fa4097de0af6b
AFTER_INJECT_TREE=0e8636c811cb03e6dcd9a5e69e0ec8c8d22e08e2
AFTER_INJECT_SOURCE_SHA=13876a4e2a246a9ed5dc2e33a124d00610115b119109eba11e29bbd47184593e
AFTER_HEAD=8d1c65b8f6f72978dad5969a8b9fa4097de0af6b
AFTER_TREE=0e8636c811cb03e6dcd9a5e69e0ec8c8d22e08e2
AFTER_SOURCE_SHA=13876a4e2a246a9ed5dc2e33a124d00610115b119109eba11e29bbd47184593e
POST_RESTORE_DIRTY=0
```

`HEAD` / `TREE` / `SOURCE_SHA` 三项**逐字相等**，首尾 `DIRTY=0`。
8 次 `PHASE1_DIRTY_AFTER_*` 全为 `0`。注入后 `git status --porcelain` 只有
` M src-tauri/Cargo.toml` 一行。

| 门禁命令 | 退出码 |
| --- | --- |
| `cargo metadata --no-deps --format-version 1`（第一个跑） | 0 |
| `cargo test -p datazen-runtime` | 0 |
| `cargo test -p datazen-schema-diff` | 0 |
| `cargo test -p datazen-data-sync` | 0 |
| `cargo test -p datazen-data-transfer` | 0 |
| `cargo test -p datazen-driver-postgres` | **101**（见下，唯一红项） |
| `cargo test -p datazen-driver-mysql` | 0 |
| `cargo test -p datazen-driver-redis` | 0 |
| `cargo test -p datazen-driver-sqlserver` | 0 |
| `node scripts/resolve-drivers.mjs --drivers=all`（注入） | 0 |
| `cargo test -p datazen --lib` | 0 |
| `cargo check -p datazen` | 0 |

逐字结论行：

```
GATE datazen-runtime EXIT=0
GATE datazen-schema-diff EXIT=0
GATE datazen-data-sync EXIT=0
GATE datazen-data-transfer EXIT=0
GATE datazen-driver-postgres EXIT=101
GATE datazen-driver-mysql EXIT=0
GATE datazen-driver-redis EXIT=0
GATE datazen-driver-sqlserver EXIT=0
inject_EXIT=0
GATE datazen-lib EXIT=0
GATE datazen-check EXIT=0
POST_RESTORE_DIRTY=0
```

- `cargo test -p datazen-runtime`：**34 条 `test result:` 行，全部 `ok.`**，
  与拆分前（`5824392f0` 门禁日志）**逐行相同**，`job_kernel` 那条仍是
  `test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`。
- `cargo test -p datazen --lib`：
  `test result: ok. 1762 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 23.89s`
- `cargo check -p datazen`：`warning: \`datazen\` (lib) generated 33 warnings`；
  `warn_lines=46  warn_files=17`（**计数仅供对照，判定见「告警」一节，按集合裁定**）。
- 全门禁唯一的 `test result: FAILED` 在
  `packages/drivers/postgres/tests/postgres_cross_database.rs`（下一节证明为基线既有失败）。


### 唯一红门禁：`postgres_cross_database` 是基线既有失败，非本轨引入

`cargo test -p datazen-driver-postgres` 退出 101，唯一失败用例：

```
---- cross_database_reads_never_move_the_session stdout ----
thread 'cross_database_reads_never_move_the_session' panicked at packages/drivers/postgres/tests/postgres_cross_database.rs:305:29:
seed probe table dz_cross_db_probe_<uuid> in dz_fixture_pg_a: Query failed: error returned from database: permission denied for schema public
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
error: test failed, to rerun pass `-p datazen-driver-postgres --test postgres_cross_database`
```

这是**连真实库的 gated live 测试**（凭据只从进程环境 `TEST_PG_*` 取）。
本机 postgres 可连但该 fixture 角色对 schema `public` 无 CREATE 权限，属本机夹具授权问题。

**证明它与本轨无关，两条独立证据：**

1. `git diff --stat 4b782750ddd1c e998366a6 -- packages/drivers/postgres/tests/`
   **输出为空**——本轨从未碰过该文件。
2. 在**基线 `4b782750ddd1c` 的独立 detached worktree**上跑同一条命令，**同样失败**：

```
BASE_PG_XDB_EXIT=101
thread 'cross_database_reads_never_move_the_session' panicked at packages/drivers/postgres/tests/postgres_cross_database.rs:305:29:
seed probe table dz_cross_db_probe_<uuid> in dz_fixture_pg_a: Query failed: error returned from database: permission denied for schema public
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

同一条用例、同一个 panic 行号、同一个错误串。**基线红、本轨同样红 → 未回归。**
两次的探针表 UUID 不同（该测试每次生成新 UUID），确认两次都是真跑而非命中残留。

**顺带纠正此前一个口径错误**：此前汇报的「postgres 基线 200 / HEAD 201」
只统计了 `--lib`，**整包跑时 lib 之外的集成二进制从未被计入**。
本节的驱动计数已改为整包口径。`--lib` 与整包数字不可比，不可混用。

### 驱动「连接行为未变」的证明（基线 vs 本 HEAD，各跑一遍整包）

基线由独立 detached worktree + 独立 `CARGO_TARGET_DIR` 在 `4b782750ddd1` 测得，
起止 `HEAD` / `tree` / `git status` 均核对未变，工作树与 target 事后已删除。

| 驱动 | 基线 | 本 HEAD | delta |
| --- | --- | --- | --- |
| postgres | 200 | 201 | +1（新增默认值一致性测试） |
| mysql | 177 | 178 | +1 |
| redis | 396（+3 ignored） | 397（+3 ignored） | +1 |
| sqlserver | 120 | 121 | +1 |
| 合计 | 893 / 0 failed | 897 / 0 failed | 每 crate 恰好 +1，**无任何测试被删、无任何计数低于基线** |

## 变异验证（7 条，全部 KILLED）

方法：每条变异在**独立 detached worktree**（`.worktrees/dz-mut-<名>`，
起点恒为 `2940e25307746`）+ **独立空 target 目录** 上执行，
逐条记录「改了几个文件 / 冷编译首行 / 退出码 / 失败用例名 / panic 行」。
所有变异脚本先 `assert s.count(old) == 1`，锚点数量不对直接失败。

| 变异 | 改什么 | 冷编译首行 | 退出码 | 被谁杀死 |
| --- | --- | --- | --- | --- |
| **M1（对照）** | 摘要恒定化：`service_key` 退回常量 `"data-transfer"` | `Compiling proc-macro2` | 101 | **13 failed**，含 `service_key_is_not_a_constant_and_never_holds_config_text`、`connection_id_alone_never_decides_the_service_key` |
| M2 | `identity_key` 重新并回 `Connection` 分量（**D1 复现**） | 同上 | 101 | `cm31_alias_configs_sharing_one_service_are_still_rejected`，panic `job_endpoint_identity.rs:57:51` |
| M3 | `ResolvedLocation::of` 去掉驱动兜底（**D2 复现**） | 同上 | 101 | `an_omitted_host_digests_as_the_drivers_declared_default`（panic `endpoint_identity.rs:411:9`）与 `an_omitted_port_…`（`:429:9`） |
| M4 | 摘要里 `host`/`port` 钉成 `None` | 同上 | 101 | **8 failed**，含 A–E 矩阵的 A、B、C 三条 |
| M5 | postgres `default_host()` 漂移成 `"127.0.0.1"` | 同上 | 101 | `the_declared_defaults_are_exactly_what_connect_dials`，panic `postgres/src/connection.rs:618:9` |
| M6 | postgres 删掉 `default_host` override，退回 trait 默认 | 同上 | 101 | 同一测试，panic `:612:14`（`.expect("postgres has an implicit host")`）——**响亮失败，不是静默通过** |
| M7 | 关掉 `any_unprovable && has_writer` 的 fail-closed | 同上 | 101 | `cm31_unprovable_identity_with_writer_is_rejected`，panic `job_endpoint_identity.rs:141:51` |

**逐字结论行（原文）**

```
m1: test result: FAILED. 15 passed; 13 failed; 0 ignored; 0 measured; 1740 filtered out; finished in 0.04s
m2: test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
m3: test result: FAILED. 26 passed; 2 failed; 0 ignored; 0 measured; 1740 filtered out; finished in 0.09s
m4: test result: FAILED. 20 passed; 8 failed; 0 ignored; 0 measured; 1740 filtered out; finished in 0.04s
m5: test result: FAILED. 200 passed; 1 failed; …
m6: test result: FAILED. 200 passed; 1 failed; …
m7: test result: FAILED. 4 passed; 1 failed; …
```

每条的 `HEAD_AFTER` 均为 `2940e25307746c72614f5286aace5c52fbbeae70`，
且 `target_dir_entries_before_build=0`（空目录起点，**排除"复用旧产物"伪通过**），
变异 worktree 与 target 事后均已删除。

**M1 与 M2 是两条不同的变异，各自独立跑、各自记证据**：M1 杀的是「身份退化成常量」，
M2 杀的是「D1 的 Connection 键复活」。M1 的 13 条失败里包含
`same_object_on_two_physical_endpoints_is_accepted`——
若身份恒定化，A 与 D 会被混为一谈，正是 M1 存在的意义。
### 变异结论的有效性前提（**这是前提，不是「无影响」四个字**）

七条变异都跑在 `2940e25307746` 这棵树上，而本轨 HEAD 现在是 `8d1c65b8f6f7`。
「变异结论继续有效」**只在下述文件清单为真的前提下成立**——清单逐字如下：

```
$ git diff --stat 2940e2530 8d1c65b8f
 packages/runtime/tests/job_kernel.rs               | 672 +++++----------------
 .../runtime/tests/job_kernel/endpoint_budget.rs    | 134 ++++
 .../runtime/tests/job_kernel/runtime_journeys.rs   | 289 +++++++++
 progress.md                                        | 449 ++++++++++++--
 4 files changed, 958 insertions(+), 586 deletions(-)
```

**4 个文件，3 个在 `packages/runtime/tests/**`，1 个是 `progress.md`。
生产代码（`src-tauri/src/**` 与 `packages/**/src/**`）改动数 = 0。**
因此 M1/M3/M4（M5/M6/M7/M2 同理）所变异的那几行，在 `8d1c65b8f` 上逐字未变，
KILLED 判定可平移。**若清单里出现任何 `src/**` 文件，本结论立即失效，须整批重跑。**

拆分本身另有独立证据：`cargo test -p datazen-runtime` 在拆分前后
34 条 `test result:` 行**逐行相同**（`diff` 为空），`job_kernel` 二进制
hash 与 `test result: ok. 16 passed` 均未变——拆分没有增删任何测试。

**M1 是控制变异，不可被 M2 替代。** M2 杀的是「D1 的 `Connection` 键复活」，
是**本轮修复内容**的反弱化；M1 杀的是「身份退化成常量 `"data-transfer"`」，
是**原始缺陷本身**（缺陷成因就是旧实现把身份写成了常量）。
两者变异点不同、失败集合不同（M1 死 13 条，M2 死 1 条），不可互相顶替。


## 告警：只比集合，不比计数

**不声称任何告警增减。** 计数口径随 `-->` 抽取规则变化，「46 行 / 17 文件」这类数字
**不可比**；验收方抽得 44/16 属抽取规则差异，**不构成事实分歧**。本轨只提交集合。

生成命令（两侧完全相同）：
`grep -oE '\-\-> [^:]+' <check 日志> | sort -u > <清单>`

两侧清单的输入日志与采集时刻（可复现性的全部依据）：

| 侧 | 输入日志 | 采集时刻 | 行数 |
| --- | --- | --- | --- |
| 基线 | `/tmp/dz-base-check.log`（脱离工作树，HEAD = `4b782750ddd1`） | 08:10 | 17 |
| 本轨 | `/tmp/dz-gate-clean-check.log`（本轨工作树，HEAD = `8d1c65b8f6f7`，最终门禁） | 09:03 | 17 |

**基线（`/tmp/dz-warn-files-BASE.txt`，17 行，逐字）：**

```
--> packages/data-sync/src/compare.rs
--> packages/data-transfer/src/sql_file.rs
--> packages/data-transfer/src/writer.rs
--> packages/drivers/elasticsearch/src/resource_provider.rs
--> packages/drivers/mongodb/src/resource.rs
--> src-tauri/src/commands/ai/util.rs
--> src-tauri/src/commands/data_transfer/plans.rs
--> src-tauri/src/commands/schema.rs
--> src-tauri/src/commands/schema_diff/job.rs
--> src-tauri/src/commands/sync/comparison_store.rs
--> src-tauri/src/commands/sync/comparison_store/disk.rs
--> src-tauri/src/commands/sync/plans.rs
--> src-tauri/src/store/app_db.rs
--> src-tauri/src/store/app_db/dashboards.rs
--> src-tauri/src/store/app_db/runs.rs
--> src-tauri/src/store/app_db/workflows.rs
--> src-tauri/src/store/platform_vault.rs
```

**本轨 `@8d1c65b8f6f72978dad5969a8b9fa4097de0af6b`（`/tmp/dz-warn-files-MINE.txt`，17 行，逐字）：**

```
--> packages/data-sync/src/compare.rs
--> packages/data-transfer/src/sql_file.rs
--> packages/data-transfer/src/writer.rs
--> packages/drivers/elasticsearch/src/resource_provider.rs
--> packages/drivers/mongodb/src/resource.rs
--> src-tauri/src/commands/ai/util.rs
--> src-tauri/src/commands/data_transfer/plans.rs
--> src-tauri/src/commands/schema.rs
--> src-tauri/src/commands/schema_diff/job.rs
--> src-tauri/src/commands/sync/comparison_store.rs
--> src-tauri/src/commands/sync/comparison_store/disk.rs
--> src-tauri/src/commands/sync/plans.rs
--> src-tauri/src/store/app_db.rs
--> src-tauri/src/store/app_db/dashboards.rs
--> src-tauri/src/store/app_db/runs.rs
--> src-tauri/src/store/app_db/workflows.rs
--> src-tauri/src/store/platform_vault.rs
```

**集合裁定（本轨侧实测）：**

| 比较 | 命令 | 结果 |
| --- | --- | --- |
| 我**引入**告警的文件 | `comm -13 <BASE> <MINE>` | **0 条** |
| 我**消除**告警的文件 | `comm -13 <MINE> <BASE>` | **0 条** |
| 交集 | `comm -12 <BASE> <MINE>` | **17** |
| 逐字节比对 | `diff <BASE> <MINE>` | **IDENTICAL（无差异行）** |

两份清单逐字节相同。注意 `src-tauri/src/commands/schema_diff/job.rs`
出现在**两侧**同一位置——它本就在告警清单里，与本轨无关，本轨也未改它（D3 属另一轨）。

**自查更正**：核对本节时先用错了输入日志名（`/tmp/dz-gate-clean-datazen-check.log`
并不存在），MINE 被抽成空清单，`comm` 一度出现假的「BASE 独有 17 条」。
已按门禁脚本第 47/50 行的真实文件名 `/tmp/dz-gate-clean-check.log` 重抽并复核，
上表是更正后的结果。**空清单导致的假差异已在读取时识别，未进入结论。**


## `src-tauri/Cargo.toml` 与 `Cargo.lock`：刻意排除，不是漏掉

`git diff --stat 4b782750ddd1c 2940e25307746 -- src-tauri/Cargo.toml Cargo.lock`
**输出为空**——两份文件与基线逐字节相同，本轨一个字节都没改。

之所以要专门写这一节：`cargo check/test -p datazen` 必须先跑
`node scripts/resolve-drivers.mjs --drivers=all` 注入驱动依赖，
而注入会改写 `src-tauri/Cargo.toml`（实测 `31 insertions(+), 1 deletion(-)`），
cargo 在注入状态下做依赖解析时又会改写 `Cargo.lock`。
这两个副作用发生在门禁记录窗口内，是 `SOURCE_SHA` 必须排除它们的原因。
入库版本保留 `# <<driver-dependencies>>` 占位段（`src-tauri/Cargo.toml:18`），
其中只有 `datazen-driver-api`——这正是 AGENTS.md「Cargo.toml 中的插件占位段在 git 中应保持为空」的要求。

## 格式与规模纪律（如实披露）

- 逐文件 `rustfmt --edition 2021 --check`：**18 个改动 .rs 文件中 16 个 EXIT=0**。
  余下 2 个的偏差**是基线就有的、与本次改动无关**，逐字核对过：
  `packages/runtime/src/job/budget.rs`（`release` 的 `.map` 1 处）、
  `packages/driver-api/src/mock_driver.rs`（`:7` 与 `:19` 两处）。**刻意未修**——修它就是改动
  与本轨无关的上游代码，扩大爆炸半径。
  （注：对 crate root 跑 rustfmt 会经 `mod` 递归进子模块并把子模块的 hunk 一并报出，
  计数会重复。）
- `mock_driver.rs` 不是 `cfg(test)` 模块：新增字段均为可加字段且默认 `None`，
  等于改动前的行为，无破坏性。
### 行数纪律：违规文件，性质各不相同

下表「修复侧」列取自 `@fc8196128`（拆分提交，即当前 HEAD）；`traits.rs` / `sqlserver.rs`
两行未随拆分变动，其数值同时成立 `@2940e253`。基线列一律 `@4b782750`。

| 文件 | 基线 | 修复后（`@fc8196128`） | delta | 性质 |
| --- | --- | --- | --- | --- |
| `packages/runtime/tests/job_kernel.rs` | 740 | 623 | **−117** | 已在 `fc8196128` 拆分，**回到 ≤800 行纪律**（见下） |
| `packages/runtime/tests/job_kernel/endpoint_budget.rs` | — | 134 | 新增 | 本次拆分新增 |
| `packages/runtime/tests/job_kernel/runtime_journeys.rs` | — | 289 | 新增 | 本次拆分新增 |
| `packages/driver-api/src/traits.rs` | 1772 | 1795 | +23 | 上游既有违规，本轨只标注 |
| `packages/drivers/sqlserver/src/sqlserver.rs` | 2566 | 2643 | +77 | 上游既有违规，本轨只标注 |
| `src-tauri/src/commands/data_transfer/job_api/assembly.rs` | 484 | 517 | +33 | 未超限；两参数 `identify` 调用点迁移 |
| `src-tauri/src/commands/data_transfer/job_api/runtime.rs` | 418 | 430 | +12 | 未超限；同上 |
| `src-tauri/src/commands/data_transfer/exec.rs` | 728 | 728 | **+0** | 未超限且**零增长** |

**`job_kernel.rs` 曾出现的 +283（峰值 1023 `@2940e253`）是本轨引入的，不是上游遗留**，
已按裁定拆分消解，如实交代来龙去脉：

740（基线）→ 841（`24f5a8955`）→ 742（`b00a1b0c2`）→ 742（`051a228a4`）→ 1023（`2940e253`）。

`051a228a4 → 2940e253` 这一段是 **377 insertions / 96 deletions 的纯重排**：
D4 要求把该文件的 import 排序按 rustfmt 规范化，我用的是整文件
`rustfmt --edition 2021`，它把约 60 条本来就超宽的单行 `assert!` 展开了。
**`#[test]` 数量 4 → 4，没加测试也没删测试**，净效果是纯格式噪声把文件撑到 1023 行。

**明确拒绝的回退路径**：把 rustfmt 展开改回去，计数就好看了。
那是**刷指标**——它会退回一份 rustfmt 不干净的代码。
裁定是**拆分**（与 `b00a1b0c2` 当时 841 → 742 同一手法），且必须是独立提交、不得 `--amend`。

### `job_kernel.rs` 拆分：提交 `fc8196128`（已完成，非计划）

**拆分提交：`fc8196128`**，独立提交，**没有 `--amend`**。

采用**二进制内 `#[path]` 子模块**拆分，理由：`b00a1b0c2` 那次是切到新的测试二进制，
但本文件有 7 个自用夹具（`ctx` / `owner_job` / `apply_payload` / `prepare_payload` /
`definition` / `repo_clock` / `runtime_with`）跨块复用；切新二进制要复制夹具或改公开面。
`#[path]` 手法取自仓库既有先例 `cm70_idempotency_replay.rs:37-47`。

| 文件 | 拆分前 | 拆分后（`@fc8196128`） | 职责 |
| --- | --- | --- | --- |
| `packages/runtime/tests/job_kernel.rs` | 1023（单文件） | **623** | 共享夹具 + CM-54 幂等 + fencing + 取消 + 持久化白名单 + 恢复旅程 |
| `packages/runtime/tests/job_kernel/endpoint_budget.rs` | — | **134** | 预算与重叠门禁旅程（CM-31 / CM-65） |
| `packages/runtime/tests/job_kernel/runtime_journeys.rs` | — | **289** | JobRuntime 生命周期旅程 |

三文件均 `rustfmt --check` 0 hunk，**均远低于 800 行上限**。
1046 行总量比拆分前 1023 多 23 行，全部是文档头（6 行）、两处 `use super::*;`、
7 行 `mod` 声明和 4 行拆分理由注释——**没有新增或删除任何一行测试逻辑**。

- `CountingHandler` / `Mode` / `impl JobHandler` **必须留在根模块**：
  持久化白名单与恢复故障旅程（根模块 L390/L457）同样要用它们，
  放子模块会造成反向依赖。第一次拆分尝试放错了位置，编译报
  `E0422` / `E0433`，已按编译器指正搬回根模块。
- **测试数量前后都是 16**（根 8 + `endpoint_budget` 3 + `runtime_journeys` 5），
  `#[test]` / `#[tokio::test]` 分布 4+12 → 4+12，无增无减。
- **仍是一个测试二进制**（`job_kernel`，二进制名 `job_kernel-4f4c1b6e8f172788` 不变），
  子模块不是独立 target，`test result:` 行数不变。
- 拆分后 `cargo test -p datazen-runtime` **EXIT=0**，34 条 `test result:` 行与拆分前
  （`5824392f0` 干净树门禁日志 `/tmp/dz-gate-clean-datazen-runtime.log`）**逐行相同**；
  `job_kernel` 那条仍是 `test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`。
- **未引入新 warning**：`job_kernel` 仍是 4 条（`OrganizationId`、`BudgetClaim`+`BudgetConfig`、
  `JobClock`+`JobResult`+`SUPPORTED_PLAN_MAJOR`、`variant CancelStage is never constructed`），
  四条与 `5824392f0` 门禁日志里逐字相同，行号 14/19/22 未变，`CancelStage` 由 421 变 103
  纯属行号位移。**刻意未修**：它们是上游既有告警，不属本轨。

**`traits.rs` / `sqlserver.rs` 的拆分计划（技术债，非本轨引入）**

- `traits.rs`（1795 行）：按 trait 职责切为
  `traits/ddl.rs`（DDL/对象）、`traits/query.rs`（查询/流式）、`traits/admin.rs`（管理命令）、
  `traits/meta.rs`（能力探测/方言），`traits.rs` 保留 trait 骨架与 re-export。
  风险是下游 17 个驱动 crate 全部要跟改，**必须独立立项、单独回归**。
- `sqlserver.rs`（2643 行）：连接与传输已在 `sqlserver/` 下有模块，
  应把「连接建立 / T-SQL 生成 / 元数据查询 / DDL 渲染」四段各自成文件，
  `sqlserver.rs` 只留 `SqlServerDriver` 的 trait 实现。

本轨**只标注这两处、不顺手拆**：AGENTS.md 的爆炸半径纪律要求改动限于本轨，
把 17 个驱动的 trait 改动混进来会让本轨的 diff 失去可审性。
「本次 +23」**不构成**对上限的通过。

**点名澄清一处外部约束**：`datazen-p5-integration` 轨要求
`handler.rs`（781 行）与 `host/mod.rs`（721 行）不增长——
这两条路径在本轨工作树与基线中**都不存在**，
`job_api/` 下实际内容为 `admission.rs / assembly.rs / cancel.rs / endpoint_identity.rs /
mod.rs / runtime.rs / scope.rs / tests`。请集成轨以本轨实际文件为准。

- `progress.md`（本文件）**合并时必须删除**，不得存活到 `main`。

## 遗留与待裁定

- **D2 修复的已知残余边界（如实记录）**：
  (a) `host: "LOCALHOST"` 与省略 host 仍不同——**刻意**不做 DNS 大小写折叠，
      因为 `normalized()` 不能对 `database` 做小写化（库名加引号时大小写敏感）；
  (b) `localhost` 与 `127.0.0.1` 仍不同；
  (c) 将来某个驱动新增默认 host 却忘了 override 新方法，缺口会静默重开——
      防线是「per-driver 单一真相常量 + 驱动 crate 内测试」，不是宿主。
- **`schema_diff` Job 路径未动**：`commands/schema_diff/job.rs` 的 D3 属另一轨，本次**超范围，未改**。
- 验收方的探针全部跑在 `MockDriver` / `TestAppState` 上，**从未跑过真实驱动**；真实驱动侧的
  兜底是上面那张「驱动内测试」表，不是宿主测试。
- **D5 已复核三次**：`grep -rn "connection_id.*trim()" packages/runtime/src packages/runtime/tests
  src-tauri/src/commands/data_transfer` 无任何命中；`identity_key` 只 trim `service_key`。
  修复前四个提交里 `identity_keys` 对 `connection_id` 与 `reserve`/
  `ensure_endpoint_services`（`runtime.rs:189`）的裸值不一致**确实存在**，故该提交是必需的。
- 遗留未做清单（明写，不掩饰）：
  1. ~~`job_kernel.rs` 拆分~~ —— **已完成**，提交 `fc8196128`，独立提交未 `--amend`，
     623 / 134 / 289 三文件均 ≤800；**本轨已无单文件越线项**；
  2. `traits.rs` / `sqlserver.rs` 上限 —— 技术债，需独立立项；
  3. 两处基线既有 rustfmt 偏差（`budget.rs:182`、`mock_driver.rs:7/:19`）—— 刻意未修；
  4. `.env` 内容全程未读；临时日志只落系统 temp，仓库内无残留。
  5. M6 的宿主侧行为**不由宿主测试钉死**（这是设计使然：宿主不该硬编码驱动行为），
     它的证据是驱动 crate 内那条响亮失败的 `.expect`。
