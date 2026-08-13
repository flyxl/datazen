# F4 QA 报告：Host 流式 PK merge-compare

| 项 | 值 |
|---|---|
| 切片 | F4 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/data_sync/compare.rs` |
| 规格 | PRD V1.2 §7 / §9 / §10（可取消、NULL/`''`/`0`/`"0"`）；方案 `docs/data-synchronization-implementation-plan.zh-CN.md` §7（keyset + 流式比较，禁止整表 `query()`） |
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
| 桌面应用 | **未要求**（本切片用 `RowPageSource`，不接真实 DB） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f4-coverage.txt`。

---

## 2. 本切片测试设计（对照规格）

规格要点（本切片）：

- Source 有 Target 无 → INSERT；Target 有 Source 无 → DELETE；同 PK 比非 PK 列 → UPDATE 或 UNCHANGED
- 复合 PK 按列顺序比较（有序 tuple）
- `NULL ≠ ''`，`0 ≠ "0"`（跨 `Value` 变体不相等；`false ≠ 0` 同理）
- 分页 keyset（`batch_size`）结果须与全量 merge 一致
- 可取消（`AtomicBool` → `DataSyncError::Cancelled`）
- 禁止整表一次性 `query()`；本切片编排面是 `RowPageSource` + `compare_table_pages`，不要求真实 DB
- 未来接 `query_stream` 的 E2E：无 UI/IPC 则 **BLOCKED**

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-MRG-01 | Rust | 四操作：UNCHANGED / UPDATE / DELETE / INSERT；DELETE 默认不选 | `merge_insert_update_delete_unchanged` |
| UT-PK-01 | Rust | 复合 PK + 非 PK 列 `0` vs `"0"` → UPDATE | `composite_pk_and_type_strictness` |
| UT-VAL-01 | Rust | 同 PK 下 `NULL` vs `''` → UPDATE | `null_not_equal_empty_string` |
| UT-CMP-01 | Rust | `cmp_values` / `cmp_keys` 同型序 + 跨型 rank（Integer < String） | `cmp_values_orders_and_cross_type_rank` |
| UT-PAGE-01 | Rust | `batch_size=2` keyset 分页计数 = 全量 merge 计数 | `paged_source_matches_full_merge` |
| UT-CAN-01 | Rust | 进入循环前已取消 → `DataSyncError::Cancelled` | `cancel_stops_compare` |
| UT-ERR-01 | Rust | PK 下标越界 → validation | `pk_index_out_of_bounds` |
| UT-EMP-01 | Rust | 双侧空表 → MATCHED、0 行、无差异 | `empty_tables_are_matched_with_no_rows` |
| E2E-F4-* | E2E | 真实 `query_stream` + keyset + Cancel 查询 | **无 IPC/UI 接线 → BLOCKED**（用例见 §5） |

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cargo test -p datazen --lib data_sync::compare -- --nocapture
```

| 项 | 数量 |
|---|---:|
| 通过 | **8** |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 865 |
| 耗时 | 0.01s（编译约 1s） |

失败详情：无。

编译告警（与 `data_sync::compare` 无关，不计入本切片）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`
- `src-tauri/src/dashboard/create.rs` unused variable `registry`

### 3.2 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-MRG-01 | `merge_insert_update_delete_unchanged` | PK=1 同值 UNCHANGED；PK=2 非 PK 变 UPDATE（`age`）；Target-only PK=3 DELETE 且 `selected=false`；Source-only PK=4 INSERT 且默认勾选 | PASS |
| UT-PK-01 | `composite_pk_and_type_strictness` | PK `(tenant, region)`；同 key 下 Integer `0` vs String `"1"` 不跨型相等 → UPDATE `n` | PASS |
| UT-VAL-01 | `null_not_equal_empty_string` | `None`/`NULL` ≠ `''` → UPDATE | PASS |
| UT-CMP-01 | `cmp_values_orders_and_cross_type_rank` | Null<Int；同型 Bool/Int/Float/Bytes/Timestamp/Json/String 序；短 key < 长 key；Int(1) < String("1") | PASS |
| UT-PAGE-01 | `paged_source_matches_full_merge` | `batch_size=2`：insert=2、update=1、delete=1、unchanged=1；表名透传 | PASS（见 §4.3 关于未直接调用全量 merge 的备注） |
| UT-CAN-01 | `cancel_stops_compare` | `cancelled=true` → `DataSyncError::Cancelled` | PASS |
| UT-ERR-01 | `pk_index_out_of_bounds` | `pk_indexes=[3]` 越界，文案含 `out of row bounds` | PASS |
| UT-EMP-01 | `empty_tables_are_matched_with_no_rows` | 空 vs 空：`!has_row_differences()` 且 `rows` 空 | PASS |

### 3.3 手工对照（不改代码）

对 UT-PAGE-01 夹具手工跑全量 merge，与分页路径一致：

| 侧 | 行（id / name） |
|---|---|
| Source | `(1,a) (2,b) (3,c) (5,e)` |
| Target | `(1,a) (2,B) (4,d)` |

全量 merge 序列：`UNCHANGED(1)` → `UPDATE(2,name)` → `INSERT(3)` → `DELETE(4)` → `INSERT(5)`。

`batch_size=2` keyset：page1 比完 `(1,2)` 后 `after_key=2`；source 下一页 `(3,5)`，target 下一页 `(4)`；随后 `3<4` INSERT、`5>4` DELETE、target 耗尽 INSERT `5`。计数与操作集合与全量一致。

---

## 4. 覆盖率（仅 `src-tauri/src/data_sync/compare.rs`）

### 4.1 测量方法

1. `cargo llvm-cov --version` → 0.8.7。
2. `--ignore-filename-regex` 负向前瞻**不能**把统计限制到单文件（llvm-cov 仍汇总整个 crate）。**不以整 crate 覆盖率为验收标准。**
3. 未过滤的 `cargo llvm-cov -p datazen --lib` 会因无关测试失败中断（`sync::adapter_registry::tests::ensure_type_wire_aliases_succeed`，期望 sqlserver adapter）。不属 F4。
4. 实际采用：

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f4-cov/f4-coverage.json -- data_sync::compare
```

然后按路径 `/data_sync/compare.rs` 过滤。摘要写入 `docs/progress/f4-coverage.txt`（完整 crate JSON 未入库）。

### 4.2 行覆盖率

| 文件 | 行 covered | 行 total | 行覆盖率 | 函数 | 区域 |
|---|---:|---:|---:|---:|---|
| `compare.rs` | 357 | 384 | **92.97%** | 96.43%（27/28） | 92.30%（731/792） |

**门槛：行覆盖率 ≥ 80% → 满足（92.97%）。**

### 4.3 未覆盖行（不构成 FAIL）

| 位置 | 说明 |
|---|---|
| `24` / `26` | `Null==Null`、右值 `Null`；现有序关系夹具只打了左 `Null` vs `Integer` |
| `35–36` | 跨型 `then_with(format Debug)`；各变体 `value_rank` 互不相同，ranks 不等时闭包永不执行（死代码） |
| `42,43,45,47–49` | `value_rank` 的 Null/Bool/Float/Bytes/Timestamp/Json；跨型夹具只有 Integer vs String |
| `105–108` | `compare_sorted_rows` 在 **source 耗尽后** 排空 target DELETE；现夹具走 `Ordering::Greater` 删除 |
| `119–122` | `compare_sorted_rows` 的 `Ordering::Less` INSERT；现夹具走「target 耗尽尾巴」INSERT |
| `206–209` | `compare_table_pages` 在 **src_page 已空** 时排空 target DELETE；分页夹具出现 target-only 时 source 仍有更大 key |

测试缺口（建议后续补，不改本切片结论）：

1. 单侧非空：source=`[]` / target=`[(1,…)]`（及相反），同时打 `compare_sorted_rows` 与 `compare_table_pages`。
2. UT-PAGE-01 应直接 `assert_eq` 分页 `TableResult.rows` 的 operation/key 序列与 `compare_sorted_rows` 输出，而不仅比计数。
3. `cancelled=false` 跑完后再置位、以及取消发生在第二页之后。
4. `false` vs `0`、`-0.0` vs `0.0` 作为非 PK 列。

---

## 5. E2E 用例表（未来接 `query_stream`）

现状：**BLOCKED**。`compare_table_pages` / `RowPageSource` **未接到任何 IPC 或 UI**（`src/commands/`、`commands/sync/`、`DataSyncWindow` 均无引用）。现有 `e2e/specs/data-sync-real.ts` 走的是旧 `compare_databases` / `compare_table_data`（抽样路径），不能当 F4 验收。`SyncSession::start_compare` 只改相位，不调用本比较器。

无 `sync_start_compare` / `sync_cancel_compare` / `data-sync:compare-progress`。Driver `query_stream` 存在于各 SQL crate，但 Host Data Sync 尚未用它拉 PK 有序页。

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F4-01 Source-only → INSERT | 同族两库、结构+PK 一致；Source 多一行 | Compare 结果该 PK 为 INSERT，默认勾选 | **BLOCKED**（无 Compare IPC） |
| E2E-F4-02 Target-only → DELETE | Target 多一行；options.delete 默认关 | 该 PK 为 DELETE 且默认不选；ChangeSet 不含它 | **BLOCKED** |
| E2E-F4-03 同 PK 非 PK 变 → UPDATE | 同 id，name 不同 | UPDATE + `changed_columns` 含该列 | **BLOCKED** |
| E2E-F4-04 全相同 → UNCHANGED | 两表行完全一致 | 0 insert/update/delete；行标 UNCHANGED | **BLOCKED** |
| E2E-F4-05 复合 PK 顺序 | PK `(tenant_id, user_id)`；`(1,2)` vs `(2,1)` 为不同 key | 不得当成同一行 UPDATE；应为 INSERT+DELETE | **BLOCKED** |
| E2E-F4-06 NULL ≠ `''` | 可空 TEXT：Source NULL，Target `''` | UPDATE，不得 UNCHANGED | **BLOCKED** |
| E2E-F4-07 `0` ≠ `"0"` | 仅当 Driver 对同列给出不同 `Value` 变体时（门闸后同物理类型时可能无法构造） | 不得判 UNCHANGED | **BLOCKED**（真实 DB 上更可能只在单测可构造） |
| E2E-F4-08 keyset 分页一致性 | 同一对表，`batch_size=2` 与 `batch_size=1000` | ChangeSet 行集合（operation+key+changed_columns）完全一致 | **BLOCKED** |
| E2E-F4-09 禁止整表 `query()` | Compare 过程中 spy / 日志 / 驱动调用 | 只走 `query_stream` 或参数化 keyset 页；不得一次 `query()` 整表 | **BLOCKED** |
| E2E-F4-10 可取消 | Compare 中途 Cancel | 相位 CANCELLED；**必须** `cancel_query`（不只停 UI）；Target 无部分写入（Compare 只读） | **BLOCKED** |
| E2E-F4-11 空表 | 两侧 0 行 | MATCHED、0 差异 | **BLOCKED** |
| E2E-F4-12 Source 空 / Target 有 | Source 0 行，Target N 行 | N 条 DELETE | **BLOCKED** |
| E2E-F4-13 Target 空 / Source 有 | 相反 | N 条 INSERT | **BLOCKED** |
| E2E-F4-14 大表不下整表 | 行数 ≫ `batch_size`（例如 1e5，batch=1000） | 过程有进度/分页；内存不按整表物化两份（Host 只保留当前页 + 已产出 Change） | **BLOCKED** |
| E2E-F4-15 旧抽样 API 不得当 Sync 依据 | 若仍暴露 `compare_table_data` 1000 行抽样 | 产品 Compare 不得用它生成 ChangeSet（属 F10 拆除；本切片未接线故未误用） | **BLOCKED**（旧 IPC 仍在，未接到新引擎） |

F9 落地建议：E2E-F4-01–04/06/08/10 进 Host 契约 journey（MySQL 夹具优先，方案 §41 第 4 项）；E2E-F4-09 用驱动侧调用计数或 Host mock `RowPageSource` 集成测即可，不必 UI。

---

## 6. 规格缺口审查（F4 范围内）

审查范围：Host 流式 PK merge-compare（`cmp_*` / `compare_sorted_rows` / `compare_table_pages` / `SliceRowSource`）。`query_stream` 真实 DB 编排、Execute SQL、Diff UI 属后续切片，标 BLOCKED / 切片外，不记本切片 P0 缺陷。

| 规格 | 实现 | 结论 |
|---|---|---|
| Source − Target → INSERT | `Ordering::Less` 与 source 尾巴 | 一致 |
| Target − Source → DELETE | `Ordering::Greater` 与 target 尾巴 | 一致 |
| 同 PK 非 PK 列 → UPDATE / UNCHANGED | `diff_changed_columns` + `optional_values_equal` | 一致 |
| 复合 PK 按 `pk_indexes` 列顺序 | `extract_key` 按索引序；`cmp_keys` zip | 一致（夹具未单独证明 `(1,2)≠(2,1)`，实现按序比较） |
| `NULL ≠ ''` | `optional_values_equal`；UT-VAL-01 | 一致 |
| `0 ≠ "0"`（跨变体） | `values_equal` 只比同变体；UT-PK-01 | 一致 |
| keyset `batch_size` 与全量 merge 一致 | 同 merge 循环 + `SliceRowSource` `>` after_key | 一致（手工+计数；见 §3.3） |
| 可取消 | 循环头读 `AtomicBool` → `Cancelled` | 一致（本切片无 Driver `cancel_query`） |
| 禁止整表 `query()` | 本文件无 SQL；只通过 `RowPageSource::next_page` | 一致 |

**未记为 P0 冲突**的残留（不构成 FAIL；不改代码）：

1. **取消检查在首次 `next_page` 之后**：`compare_table_pages` 先拉源/目标第一页，再进 loop 看 `cancelled`。预取消仍会打两次 page fetch，然后才 `Cancelled`。未来 `query_stream` 适配器必须把 flag 传进 `next_page` / `cancel_query`，否则不满足 PRD「必须取消后端查询」。
2. **`compare_table_pages` 不调用 `SyncOptions::validate()`**：`batch_size=0` 时 `SliceRowSource` 用 `limit.max(1)`；真实适配器若把 `0` 当 `LIMIT 0`，会误判空表。F1 已拒绝 `batch_size=0`；调用方须先 validate。
3. **重复 PK**：`SliceRowSource` 的 after_key 为严格 `Greater`，跨页会丢掉与 after_key 相等的重复行；`compare_sorted_rows` 则会逐行 zip。F2 假定真实 PK 唯一；本切片 mock 未测。
4. **`LargeValueMode::Hash`**：比较器始终 Full 比 `Value`。PRD 写明 Hash 为 P1。
5. **无进度回调**：PRD 要整体/单表进度；本切片 API 无 progress sink（属 F8 / IPC 切片）。
6. **`compare_sorted_rows` 不排序**：调用方须已按 PK 有序（keyset `ORDER BY`）。`SliceRowSource::new` 会排序，二者契约不同。
7. **旧 `compare_table_data` 抽样 1000 行 IPC 仍在**：未调用新引擎；F10 拆除。本切片未把它接成 Sync 依据。

---

## 7. Bug 列表

无。

---

## 8. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib data_sync::compare` | **8 passed / 0 failed** |
| `compare.rs` 行覆盖 ≥80% | **92.97%**（357/384） |
| 与 P0 规格冲突的缺陷 | 无 |
| `query_stream` / UI / IPC E2E | 全部 **BLOCKED**（本切片明确不要求真实 DB） |

**总评：PASS**

Host 流式 PK merge-compare（INSERT/UPDATE/DELETE/UNCHANGED、复合 PK 序、`NULL≠''`、跨型 `0≠"0"`、keyset 分页与全量计数一致、可取消、越界校验、空表）与 PRD V1.2 / 方案 §7 在 `RowPageSource` 边界内一致。真实 `query_stream` 接线、`cancel_query`、以及分页结果与 `compare_sorted_rows` 的逐行 `assert_eq` 留给后续切片。
