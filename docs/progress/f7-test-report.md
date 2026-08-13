# F7 QA 报告：Compare → Apply → Recompare = 0

| 项 | 值 |
|---|---|
| 切片 | F7 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/data_sync/apply_loop.rs` |
| 规格 | 选中 INSERT/UPDATE（及开启并勾选的 DELETE）应用后，再比较行差异应为 0。未勾选 DELETE 则再比较仍有 DELETE。PRD V1.2 DoD；方案 `docs/data-synchronization-implementation-plan.zh-CN.md` §1 / §41 项 7 |
| 测试角色 | 独立 QA；未修改任何产品代码；未 commit |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（0 个产品缺陷；E2E 全部 BLOCKED） |

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
| 桌面应用 | **未运行**（`ps` 无 DataZen 进程；computer-use `list_windows` 仅 Edge）。E2E **BLOCKED** |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f7-coverage.txt`。

---

## 2. 本切片测试设计（对照规格）

规格要点（本切片）：

- 内存闭环：Compare → ChangeSet（仅已勾选且 options 允许的变更）→ Apply → Recompare
- 选中 INSERT + UPDATE，且 `options.delete=true` 并勾选 DELETE → 应用后 `remaining_mutating_changes == 0`
- 未勾选 DELETE（默认不选；或 `options.delete=false` 根本不允许）→ 应用后 Recompare 仍有 DELETE
- 本切片是 **in-memory** P0 验收 helper（文件头注释），不经真实 Driver / `execute_data_sync`

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-AL-01 | Rust `apply_loop` | delete 开启并勾选；INSERT+UPDATE+DELETE 全应用后 Recompare=0 | `compare_apply_recompare_is_zero_with_deletes` |
| UT-AL-02 | Rust `apply_loop` | 未选 DELETE（默认 options.delete=false）→ Recompare 仍 1 条 DELETE | `without_selecting_deletes_recompare_still_has_deletes` |
| UT-AL-03 | Rust `apply_loop` | `delete=true` 但 DELETE 未勾选；同时应用 INSERT/UPDATE → 仅剩 DELETE | **缺失**（与 UT-AL-02 同路径：`eligible_for_changeset` 要求 selected；见 §6） |
| UT-AL-04 | Rust `apply_loop` | 取消勾选部分 INSERT → Recompare 仍有这些 INSERT | **缺失** |
| UT-AL-05 | Rust `apply_loop` | INSERT/UPDATE 缺 `source_row` → validation | **缺失**（行 46–47 count=0） |
| UT-AL-06 | Rust `apply_loop` | 传入 Unchanged → 不计入 applied、不改 target | **缺失**（行 56 count=0） |
| E2E-F7-* | E2E | 真库 + Diff UI：Apply 后再比较 = 0 | **BLOCKED**（无桌面应用；无 Diff Workspace；窗仍走旧 `sync_tables`） |

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cargo test -p datazen --lib data_sync::apply_loop -- --nocapture
```

| 项 | 数量 |
|---|---:|
| 通过 | **2** |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 885 |
| 耗时 | 0.02s（已编译） |

失败详情：无。

编译告警（不计入本切片缺陷；**未改代码**）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`（与 F5/F6 相同，无关）
- `src-tauri/src/dashboard/create.rs` unused variable `registry`（无关）
- `data_sync/execute.rs:205` `cancel_mid_run_rolls_back` 中被遮蔽的 `let mut exec`（F6 测试夹具，无关）

### 3.2 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-AL-01 | `compare_apply_recompare_is_zero_with_deletes` | Source `(1,a)(2,b2)(3,c)` / Target `(2,b)(4,d)`；`delete=true`；勾选 DELETE；Compare 含 INSERT+UPDATE+DELETE；Apply 后 `remaining_mutating_changes=0` 且 `unchanged_row_count == source.len()` | PASS |
| UT-AL-02 | `without_selecting_deletes_recompare_still_has_deletes` | Source `(1,a)` / Target `(1,a)(9,gone)`；默认 `delete=false`；ChangeSet 不含 DELETE；Apply 空操作；Recompare `delete_count=1`，insert/update=0 | PASS |

### 3.3 手工对照（不改代码）

夹具 UT-AL-01 的 Compare 结果（按 PK merge）：

| 操作 | key | 说明 |
|---|---|---|
| INSERT | 1 | source only |
| UPDATE | 2 | `b` → `b2` |
| INSERT | 3 | source only |
| DELETE | 4 | target only |

勾选 DELETE 后 `ChangeSet::from_comparison` 收入 4 条；`apply_changeset_to_rows` 写入 1/2/3、删除 4；再比较与 source 对齐，mutating=0。

夹具 UT-AL-02：Compare 为 UNCHANGED(1) + DELETE(9)。`eligible_for_changeset` 因 `selected=false` 且 `options.delete=false` 丢掉 DELETE；`from_comparison` 得到空 tables；Apply 空切片；Target 仍含 key 9；Recompare 仍 1 DELETE。

`apply_changeset_to_rows` **自身不读** `selected` / `options.allows`：过滤发生在 `ChangeSet::from_comparison` → `RowChange::eligible_for_changeset`。测试走的是该契约路径，与 F1 选择规则一致。

`remaining_mutating_changes` = `insert_count + update_count + delete_count`（按 operation 计数，不看 selected）。全量应用后为 0；跳过 DELETE 后 DELETE 仍计入，符合规格。

---

## 4. 覆盖率（仅 `apply_loop.rs`）

### 4.1 测量方法

1. `cargo llvm-cov --version` → 0.8.7。
2. `--ignore-filename-regex` 负向前瞻**不能**把统计限制到单文件（llvm-cov 仍汇总整个 crate）。**不以整 crate 覆盖率为验收标准。**
3. 未过滤的 `cargo llvm-cov -p datazen --lib` 会因无关测试失败中断（`sync::adapter_registry::tests::ensure_type_wire_aliases_succeed`）。不属 F7。
4. 实际采用：

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f7-cov/f7-coverage.json -- data_sync::apply_loop
```

然后按路径 `/data_sync/apply_loop.rs` 过滤。摘要写入 `docs/progress/f7-coverage.txt`（完整 crate JSON 未入库）。

llvm-cov 本次跑 **2 passed / 0 failed**。

### 4.2 行覆盖率

| 文件 | 行 covered | 行 total | 行覆盖率 | 函数 | 区域 |
|---|---:|---:|---:|---:|---|
| `apply_loop.rs` | 116 | 122 | **95.08%** | 87.50%（14/16） | 96.56%（281/291） |

**门槛：`apply_loop.rs` 行覆盖率 ≥ 80% → 满足（95.08%）。**

### 4.3 未覆盖行（不构成 FAIL）

| 位置 | 说明 |
|---|---|
| `apply_loop.rs` 46–47 | INSERT/UPDATE 缺 `source_row` 的 validation 闭包 |
| `apply_loop.rs` 56 | `ChangeOperation::Unchanged => {}`；ChangeSet 不会放入 Unchanged |

测试缺口（建议后续补，不改本切片结论）：

1. `delete=true` + DELETE 保持 `selected=false`，同时有 INSERT/UPDATE 被应用（规格原文「未勾选 DELETE」的最贴近夹具；当前 UT-AL-02 用的是 option 关闭）。
2. 取消勾选部分 INSERT/UPDATE 后 Recompare 只剩未选项。
3. 缺 `source_row` / PK index OOB / Unchanged 传入 apply。
4. 复合 PK、NULL PK 的 key_bytes 往返（INSERT 用 `extract_key(source_row)`，DELETE 用 `change.key`）。
5. 真实库：`generate_table_sql` → `execute_data_sync` → 再 `compare` = 0（属 F6+F8+F9 联调，非本文件）。

---

## 5. E2E 用例表

现状：**BLOCKED**。未新增 `e2e/specs/` 文件（禁止改产品代码；F8 前无 Diff UI 可挂）。

- 本机无 DataZen 桌面进程；computer-use 窗口列表无应用窗。
- `src/windows/data-sync/DataSyncWindow.tsx` 仍调用旧 IPC `sync_tables`（F3 已拒绝覆盖拷贝）；**无** Compare / ChangeSet 勾选 / Apply / Recompare 控件。
- 前端 **无** `executeDataSync` 调用（F6 已记；属 F8）。
- `e2e/specs/` **无** `apply_loop` / Recompare=0 / `executeDataSync` 用例。现有 `e2e/specs/data-sync-real.ts` 面向旧 compare / `sync_tables` refuse，不能当 F7 验收。
- F8（Diff Workspace）/ F9（Host E2E 契约）均为 pending。

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F7-01 全选 INSERT+UPDATE+DELETE → 0 | 同族两库；结构+PK 一致；Target 多 1 行、少 1 行、1 行值不同；开启 DELETE 并勾选全部；Apply；Recompare | mutating 行差异 = 0；Target 行与 Source 一致 | **BLOCKED**（无 Diff UI / 无应用） |
| E2E-F7-02 未勾选 DELETE | 同上但 DELETE 保持未选（或选项关闭） | Apply 后 INSERT/UPDATE 消失；Recompare 仍有 DELETE | **BLOCKED** |
| E2E-F7-03 仅勾选部分 INSERT | 多条 INSERT，只选 1 条 | Recompare 仍有未选 INSERT；已选行已对齐 | **BLOCKED** |
| E2E-F7-04 仅 UPDATE | 同行 PK、非 PK 列不同 | Apply 后 Recompare=0；无多余 INSERT/DELETE | **BLOCKED** |
| E2E-F7-05 空 ChangeSet | 全 UNCHANGED 或全不选 | 拒绝执行；Recompare 与首次 Compare 相同 | **BLOCKED** |
| E2E-F7-06 真库事务后再比较 | 经 `execute_data_sync` 提交后再 Compare | 库内已提交；Recompare=0；不走 `sync_tables` | **BLOCKED**（F6 IPC 有、UI 未接） |
| E2E-F7-07 旧窗不得假闭环 | 打开现有 Data Sync 窗点 Start Sync | 不得 DROP+INSERT 后冒充 Recompare=0；应拒绝覆盖拷贝 | **BLOCKED**（无应用；F3 IPC 已 refuse） |
| E2E-F7-08 复合 PK | 两列 PK 的 INSERT+UPDATE+DELETE | 全选含 DELETE 后 Recompare=0 | **BLOCKED** |

F8/F9 落地建议：E2E-F7-01/02/06 进 Host 契约 journey（PG + MySQL）；断言用 i18n / 差异计数，不写方言 SQL。无 UI 前可用 IPC：`compare` → 构造 ChangeSet → `execute_data_sync` → 再 `compare`，不必点选。

---

## 6. 规格缺口审查（F7 范围内）

审查范围：`key_bytes` / `rows_to_map` / `map_to_sorted_rows` / `apply_changeset_to_rows` / `recompare_table` / `remaining_mutating_changes` 及两条 `#[cfg(test)]`。SQL 生成、事务 Execute、Diff UI 属其它切片。

| 规格 | 实现 | 结论 |
|---|---|---|
| 选中 INSERT/UPDATE 应用后差异减少/清零 | ChangeSet 默认选中 INSERT/UPDATE；apply 按 source_row upsert | 一致；UT-AL-01 含 INSERT+UPDATE，全量含 DELETE 后为 0 |
| 开启并勾选 DELETE → Recompare=0 | UT-AL-01 将 DELETE `selected=true` + `options.delete=true` | 一致 |
| 未勾选 DELETE → Recompare 仍有 DELETE | UT-AL-02：`options.delete=false`（默认不选） | 行为一致；夹具未覆盖「option 开着但未勾选」+ 同时有 INSERT（代码路径相同，见 `eligible_for_changeset`） |
| Compare → Apply → Recompare 编排 | 测试内串联 `compare_sorted_rows` → `ChangeSet::from_comparison` → `apply_changeset_to_rows` → `recompare_table` | 一致（内存） |
| 仅已勾选行进入 Apply | 过滤在 ChangeSet，不在 apply 函数 | 契约一致；直接传入原始 `RowChange` 切片会忽略 `selected`（调用方责任） |

**未记为缺陷**的残留（不构成 FAIL；不改代码）：

1. **UT-AL-02 用 option 关闭代替「未勾选」**：`default_selected(Delete)=false`，option 开启时未勾选同样进不了 ChangeSet。逻辑等价，缺组合夹具。
2. **apply 不二次校验 selected / options**：注释写「selected rows」，实现信任 ChangeSet。误用 API 可能应用未选项（含 DELETE）。
3. **DELETE 的 `applied += 1` 在 key 不存在时仍加一**：`BTreeMap::remove` 返回 None 也计数。本切片夹具 key 均存在。
4. **INSERT/UPDATE 用 `extract_key(source_row)`，DELETE 用 `change.key`**：正常 Compare 产出两者同源；不一致 RowChange 会导致漏删/错插。无脏数据单测。
5. **`key_bytes` 经 `serde_json::to_vec`，失败则 `unwrap_or_default()`**：Value 序列化失败时所有坏 key 撞空字节。未击中。
6. **`map_to_sorted_rows` 对 `extract_key` 用 `unwrap_or_default`**：PK OOB 时按空 key 排序，不报错。
7. **`recompare_table` 表名写死 `"src"`/`"tgt"`**：只影响 TableResult 标签，不影响计数。
8. **无 UI / 无 Host E2E / 未接 `execute_data_sync`**：属 F8/F9；内存闭环已由 UT 覆盖。
9. **`apply_changeset_to_rows` 未被 session / IPC 调用**：仅 re-export + 本文件测试。P0 验收 helper，活路径仍是 F6 SQL+Execute。

---

## 7. Bug 列表

无。无 S1/S2/S3/S4。

未发现「勾选 DELETE 后 Recompare 仍有 DELETE」或「未勾选 DELETE 却被 apply 掉」的证据。两条规格单测均 PASS。

---

## 8. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib data_sync::apply_loop` | **2 passed / 0 failed** |
| `apply_loop.rs` 行覆盖 ≥80% | **95.08%**（116/122） |
| 与 P0 规格冲突的缺陷 | 无 |
| E2E | 全部 **BLOCKED**（无桌面应用；无 Diff UI / 无 F7 spec） |
| 记入缺陷 | **0** |

**总评：PASS**

内存闭环满足：选中 INSERT/UPDATE 且开启并勾选 DELETE 后 Recompare mutating=0；未将 DELETE 纳入 ChangeSet 时 Recompare 仍有 DELETE。`apply_loop.rs` 行覆盖 95.08%。缺 `source_row` / Unchanged 分支与真库 E2E，不挡住本切片结论。E2E 待 F8 Diff Workspace 与 F9 契约 journey。
