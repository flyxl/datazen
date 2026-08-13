# F2 QA 报告：Data Synchronization 硬门闸

| 项 | 值 |
|---|---|
| 切片 | F2 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/data_sync/{pairing,types_eq,gate,mapping}.rs`（及 `mod.rs` 导出） |
| 规格 | PRD V1.2 `docs/data-synchronization-prd.zh-CN.md` §9 / §10 / §23；实施方案 `docs/data-synchronization-implementation-plan.zh-CN.md` 硬门闸与 §9 Table Mapping |
| 测试角色 | 独立 QA；未修改任何产品代码；未 commit |
| 日期 | 2026-08-13 |
| **总评** | **PASS** |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| toolchain | stable-aarch64-apple-darwin |
| cargo-llvm-cov | 0.8.7 |
| crate | `datazen` 0.0.9（`src-tauri`） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f2-coverage.txt`。

---

## 2. 单元测试

### 2.1 命令

```bash
cargo test -p datazen --lib data_sync -- --nocapture
```

（按要求跑全 `data_sync` 测试集；F1 模块用例一并执行，不计入 F2 覆盖率门槛。）

### 2.2 结果

| 项 | 数量 |
|---|---:|
| 通过 | **53** |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 809 |
| 耗时 | 0.00s（增量编译约 0.79s） |

失败详情：无。

编译告警（与 `data_sync` 无关，不计入本切片）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`
- `src-tauri/src/dashboard/create.rs` unused variable `registry`

### 2.3 F2 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-PAIR-01 | `mysql_mariadb_is_direct_mysql_family` | §9.1 / §23：mysql↔mariadb Direct，family=`mysql` | PASS |
| UT-PAIR-02 | `postgres_aliases_are_direct` | §9.1 / §23：`postgresql`/`postgres`/`cloudberry` 同族 | PASS |
| UT-PAIR-03 | `pg_to_mysql_is_transfer_not_sync` | §9.1：异构 IR 拒绝，文案含 Transfer | PASS |
| UT-PAIR-04 | `sqlite_same_family_not_v1` | §23：SQLite 同族也非 V1 | PASS |
| UT-PAIR-05 | `redis_and_kiwi_rejected` | §23：Redis / Kiwi / mysql→mongodb 拒绝 | PASS |
| UT-PAIR-06 | `v1_family_helpers` | V1 仅 `mysql` + `postgresql` | PASS |
| UT-TEQ-01 | `mysql_int_aliases_and_display_width` | §9.3：INT=INTEGER、INT(11)=INT；INT≠BIGINT；TEXT≠VARCHAR；UNSIGNED 保留 | PASS |
| UT-TEQ-02 | `postgres_int_and_varchar_aliases` | §9.3：int/int4/int8/int2、varchar/character varying、timestamptz、float4/8 | PASS |
| UT-TEQ-03 | `empty_and_unknown_family_are_literal` | 未知 family 不做跨类型别名 | PASS |
| UT-GATE-01 | `identical_mysql_tables_pass_even_if_column_order_differs` | §9.3：列名集合 + 类型等价 + 可空；物理顺序可不同；异名表也可 Compatible | PASS |
| UT-GATE-02 | `missing_pk_is_incompatible` | §9.2：无 PK → INCOMPATIBLE | PASS |
| UT-GATE-03 | `pk_from_column_flags_when_list_empty` | PK 可从 `is_primary_key` 回退 | PASS |
| UT-GATE-04 | `composite_pk_order_matters` | §9.2：复合 PK 列集合+**顺序**必须相同 | PASS |
| UT-GATE-05 | `extra_and_missing_columns_listed` | §9.3：缺列/多列都列出；禁止只同步交集 | PASS |
| UT-GATE-06 | `type_and_nullability_mismatch` | §9.3：类型 + 可空性不一致 → INCOMPATIBLE | PASS |
| UT-GATE-07 | `view_is_not_a_base_table` | §9 / §10：View / MaterializedView / SystemTable 非基表 | PASS |
| UT-GATE-08 | `indexes_do_not_affect_gate` | §9.3：非 PK 索引差异不阻断 | PASS |
| UT-MAP-01 | `auto_maps_same_name_and_marks_unmapped` | §10：同名自动映射；未配对 UNMAPPED_*；视图不进列表 | PASS |
| UT-MAP-02 | `disabled_mapping_skips_gate` | §10：DISABLED 不跑门闸 | PASS |
| UT-MAP-03 | `renamed_mapping_still_requires_identical_structure` | §10：不同名 mapping 通过门闸 → MATCHED | PASS |
| UT-MAP-04 | `missing_table_or_schema_or_view_is_incompatible` | 缺表 / 目标为 View → INCOMPATIBLE | PASS |
| UT-MAP-05 | `structure_mismatch_after_rename_is_incompatible` | 不同名 mapping 结构不一致仍 INCOMPATIBLE | PASS |
| UT-MAP-06 | `missing_schema_payload_is_incompatible` | schema 未加载 → INCOMPATIBLE | PASS |

F1 同命令下另外 30 个用例（options / task / changeset / state / session / error）全部 PASS，不在本切片门槛内。

---

## 3. 覆盖率（仅 F2 四文件）

### 3.1 测量方法

1. `cargo llvm-cov --version` → 0.8.7。
2. 不以整个 `datazen` crate 覆盖率为验收标准（全 crate 行覆盖约 2% 量级，会被未跑测试稀释）。
3. 不把 F1 文件（`model.rs` / `state.rs` / `changeset.rs` / `session.rs` / `error.rs`）计入本切片门槛。
4. 实际采用：只跑 `data_sync` 测试，从 JSON 过滤四文件：

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f2-cov/f2-coverage.json -- data_sync
```

然后按路径 `/data_sync/` + basename ∈ `{pairing,types_eq,gate,mapping}.rs` 过滤。摘要写入 `docs/progress/f2-coverage.txt`（完整 crate JSON ~6.4MB，未入库）。

### 3.2 各文件行覆盖率

| 文件 | 行覆盖 | 行总数 | 行% | 函数% | 区域% |
|---|---:|---:|---:|---:|---:|
| `pairing.rs` | 54 | 54 | **100.00%** | 100.00% | 98.94% |
| `types_eq.rs` | 151 | 151 | **100.00%** | 100.00% | 97.32% |
| `gate.rs` | 263 | 266 | **98.87%** | 100.00% | 98.07% |
| `mapping.rs` | 305 | 306 | **99.67%** | 100.00% | 99.04% |
| **合计** | **773** | **777** | **99.49%** | **100.00%** | **98.31%** |

**门槛：合计行覆盖率 ≥ 80% → 满足（99.49%）。**

未覆盖行（不影响门槛）：

| 位置 | 说明 |
|---|---|
| `types_eq.rs:45` | `canonical_type` 空参数分支（如 `INT()`）无夹具 |
| `gate.rs:269,298,330` | 测试 `panic!` 臂（期望 Incompatible） |
| `mapping.rs:42` | `target_table` 为空时不记 mapped_targets |
| `mapping.rs:79-80` | **source** 侧非基表；现有用例只测 target View |

---

## 4. E2E 用例表（未来 UI）

现状：**BLOCKED**。`src/windows/data-sync/DataSyncWindow.tsx` 仍是旧「选表 → 覆盖拷贝」窗口；前端 `resolveSyncPairing` 仍把跨族 SQL 标为 `path: 'ir', supported: true`（例如 PG↔MySQL 可选）。F2 的 `require_data_sync_family` / `classify_tables` / `check_table_gate` **尚未接到 IPC 或 Diff Workspace**。无 MATCHED / INCOMPATIBLE / Transfer 引导 UI。现有 `e2e/specs/data-sync-real.ts` 测的是旧 IR/拷贝路径，不能当作本切片验收。

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F2-01 | 打开 Data Sync；Source=MySQL，Target=MariaDB | 允许配对（Direct / mysql family），可进入 Mapping | **BLOCKED**（无新门闸 UI） |
| E2E-F2-02 | Source=PostgreSQL，Target=postgres/cloudberry 别名连接 | 允许配对（postgresql family） | **BLOCKED** |
| E2E-F2-03 | Source=PostgreSQL，Target=MySQL | 拒绝进入 Compare；提示改用 Data Transfer；不得走 IR 拷贝 | **BLOCKED**（旧 UI 仍允许 IR） |
| E2E-F2-04 | Source=Target=SQLite | 拒绝；说明 V1 不支持该 family | **BLOCKED** |
| E2E-F2-05 | Source=Target=Redis | 拒绝；非 V1 SQL Sync | **BLOCKED** |
| E2E-F2-06 | Source=Kiwi，Target=PostgreSQL | 拒绝（unsupported / 非 Sync） | **BLOCKED** |
| E2E-F2-07 | 同族两库，同名基表且结构+PK 一致；点 Compare 前看 Mapping | 同名自动 MATCHED | **BLOCKED** |
| E2E-F2-08 | 手工映射 `customers`→`clients`（结构+PK 一致） | MATCHED；可 Compare | **BLOCKED**（手工映射 UI 为 P1，门闸逻辑已有单测） |
| E2E-F2-09 | 同名映射但 Target 缺列/多列 | INCOMPATIBLE，列出具体列；禁止只同步交集列 | **BLOCKED** |
| E2E-F2-10 | 列物理顺序不同，列名集合/类型/可空/PK 相同 | MATCHED，允许 Compare | **BLOCKED** |
| E2E-F2-11 | `INT` vs `BIGINT` 或可空性不同 | INCOMPATIBLE + 列级原因 | **BLOCKED** |
| E2E-F2-12 | 双方或任一方无 PRIMARY KEY | INCOMPATIBLE；提示 Structure Sync 或 Transfer | **BLOCKED** |
| E2E-F2-13 | 复合 PK `(tenant_id, user_id)` vs `(user_id, tenant_id)` | INCOMPATIBLE（顺序不同） | **BLOCKED** |
| E2E-F2-14 | Source/Target 为 View 或物化视图 | 不进可同步列表；若被映射则为 INCOMPATIBLE | **BLOCKED** |
| E2E-F2-15 | 仅非 PK 索引 / 外键不同，列+PK 一致 | 可 Sync（可警告 Structure Sync，但不阻断） | **BLOCKED** |
| E2E-F2-16 | INCOMPATIBLE 表点 Compare | 不得比较、不得生成 Change Set | **BLOCKED** |

---

## 5. 规格缺口审查（P0）

对照 PRD §9 / §10 / §23 与实施方案硬门闸 + §9 Mapping。下列为**已实现且与 P0 一致**的行为：

| 规格 | 实现 | 结论 |
|---|---|---|
| 同族 Direct（mysql↔mariadb，PG 别名） | `require_data_sync_family` → `SyncPairing::Direct` + V1 family | 一致 |
| 异构 IR → 拒绝并提示 Transfer | `SyncPairing::Ir` 错误含 `Data Transfer` | 一致 |
| SQLite / Redis / Kiwi 非 V1 | sqlite 同族报 V1；redis Direct 但非 V1；kiwi Unsupported | 一致 |
| 列名集合、类型等价、可空；物理顺序可不同 | `check_table_gate` 按列名对齐 | 一致 |
| 相同 PRIMARY KEY（含复合且顺序相同）；无 PK → INCOMPATIBLE | `MissingPrimaryKey` / `PrimaryKeyMismatch` | 一致 |
| 视图/物化视图非基表 | `check_base_table`；自动映射跳过非 `Table` | 一致 |
| 同名自动映射；不同名 mapping 仍过门闸 | `classify_tables` | 一致 |
| 非 PK 索引不阻止 Sync | gate 不读 `indexes`（FK 同样忽略） | 一致 |
| 禁止只同步交集列 | 缺列+多列同时列出并 INCOMPATIBLE | 一致 |

**未记为 P0 冲突**的残留（不构成 FAIL；不改代码）：

1. **索引/外键/触发器「警告即可」**：实现完全忽略，没有 warning 载荷。P0 要求是「不阻断」，已满足；警告适合 UI 切片。
2. **PG `DECIMAL` vs `NUMERIC`**：`types_eq` 未互为别名（MySQL 已互为 DECIMAL）。现场 PG driver 从 `information_schema.columns.data_type` 读取，DECIMAL/NUMERIC 均返回 `numeric`，误伤风险低。
3. **`classify_tables` 不调用 `require_data_sync_family`**：family 由调用方传入。本切片无 IPC 编排；漏接时可能用错误 family 比类型。属后续组合切片。
4. **显式 mappings 非空时不产出 `UNMAPPED_*`**：P0 主路径是空 mappings 自动匹配（已测）；手工映射 UI 为 P1。
5. **临时表**：`TableType` 无 Temporary 变体；依赖驱动不要把临时表报成 `Table`。
6. **自同步「同连接+同库+映射到自身」**：在 F1 `SyncTask::validate`，不在本四文件。
7. **无 PK / 结构失败文案**未内嵌「去 Structure Sync / Transfer」；有 `CompatCode` 可供 UI 映射。IR 配对文案已含 Transfer。
8. **产品窗仍允许 IR**：旧 `syncPairing.ts` `supported: true`。F2 后端会拒绝，但尚未接线。记 E2E BLOCKED，不是本四文件逻辑错误。

**P0 规格冲突缺陷：无。**

---

## 6. Bug 列表

无

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib data_sync` | 53 passed / 0 failed |
| F2 四文件合计行覆盖率 ≥ 80% | **99.49%**（各文件均 ≥ 98.87%） |
| P0 规格冲突缺陷 | 无 |
| UI E2E | 全部 BLOCKED（无 F2 门闸 UI / IPC） |

**总评：PASS**
