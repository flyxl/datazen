# F1 QA 报告：Navicat 风格 Data Sync 领域模型 + 状态机 + ChangeSet

| 项 | 值 |
|---|---|
| 切片 | F1 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/data_sync/`（`error.rs` / `model.rs` / `state.rs` / `changeset.rs` / `session.rs` / `mod.rs`） |
| 规格 | PRD V1.2 `docs/data-synchronization-prd.zh-CN.md` §9 / §24 / §25 / §28；实施方案 `docs/data-synchronization-implementation-plan.zh-CN.md` §6 |
| 测试角色 | 独立 QA；未修改任何产品代码；未 commit |
| 日期 | 2026-08-13 |
| **总评** | **PASS** |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (aarch64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| toolchain | stable-aarch64-apple-darwin |
| cargo-llvm-cov | 0.8.7 |
| llvm-tools | llvm-tools-aarch64-apple-darwin |
| crate | `datazen` 0.0.9（`src-tauri`） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f1-coverage.txt`。

---

## 2. 单元测试

### 2.1 命令

```bash
cargo test -p datazen --lib data_sync -- --nocapture
```

### 2.2 结果

| 项 | 数量 |
|---|---:|
| 通过 | **30** |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 809 |
| 耗时 | 0.01s（编译约 7.6s） |

失败详情：无。

编译告警（与 `data_sync` 无关，不计入本切片）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`
- `src-tauri/src/dashboard/create.rs` unused variable `registry`

### 2.3 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-OPT-01 | `default_options_match_prd` | §8 / 方案 §6.3：insert/update 开、delete 关、batch=1000、PK、Full | PASS |
| UT-OPT-02 | `options_reject_empty_ops_and_zero_batch` | batchSize>0；至少一种操作 | PASS |
| UT-TASK-01 | `self_sync_same_database_is_rejected` | PRD §28.8；schema 空白等价 | PASS |
| UT-TASK-02 | `same_connection_different_database_is_allowed` | 同连接不同库允许 | PASS |
| UT-TASK-03 | `empty_schema_normalizes_like_none` | schema `""` ≡ `None` | PASS |
| UT-TASK-04 | `task_requires_ids_and_databases` | 任务 id / connection / database 必填 | PASS |
| UT-ROW-01 | `delete_not_selected_by_default_even_when_enabled` | §8.3 / §28.3：DELETE 默认不选；勾选后才可进 ChangeSet | PASS |
| UT-ROW-02 | `insert_not_eligible_when_option_disabled` | options 关闭则即使 selected 也不进 ChangeSet | PASS |
| UT-CMP-01 | `comparison_summary_separates_tables_and_rows` | §11 / §24：行计数 vs 未变表 vs INCOMPATIBLE | PASS |
| UT-VAL-01 | `values_do_not_cross_types` | 方案 §7.5：`0≠"0"`、`NULL≠''`、同变体比较 | PASS |
| UT-SER-01 | `serde_roundtrip_task_and_mapping_status` | camelCase 任务；状态 `UNMAPPED_SOURCE`；操作 `INSERT` | PASS |
| UT-SER-02 | `row_change_serde_keeps_operation` | RowChange 序列化保操作类型 | PASS |
| UT-CS-01 | `changeset_excludes_unselected_deletes_and_unchanged` | 方案 §10：未选 DELETE / UNCHANGED / INCOMPATIBLE 不进 | PASS |
| UT-CS-02 | `selected_delete_enters_changeset_only_when_option_on` | delete 开 + 用户勾选 → 进入并 `requires_delete_confirmation` | PASS |
| UT-CS-03 | `empty_changeset_is_not_executable` | 空 ChangeSet 不可执行 | PASS |
| UT-CS-04 | `unselected_insert_is_dropped` | §28.4 未选中行不得执行 | PASS |
| UT-SM-01 | `happy_path_to_completed` | §25 主路径到 COMPLETED | PASS |
| UT-SM-02 | `comparing_cannot_jump_to_execute_pipeline` | §25 禁止 COMPARING → 执行管线 | PASS |
| UT-SM-03 | `draft_cannot_compare_or_execute` | DRAFT 不能直接 COMPARE/EXECUTE | PASS |
| UT-SM-04 | `compared_cannot_skip_review` | COMPARED 不能跳过 REVIEW | PASS |
| UT-SM-05 | `cancel_and_retry_paths` | 取消 / 失败重试 / 回滚 | PASS |
| UT-SM-06 | `same_phase_is_not_a_transition` | 同相不是合法转移 | PASS |
| UT-SM-07 | `serde_and_helpers` | 相位 serde `READY_TO_EXECUTE`；辅助谓词 | PASS |
| UT-SES-01 | `compare_review_execute_happy_path` | Session 编排主路径；DRAFT 不能 start_compare | PASS |
| UT-SES-02 | `cannot_review_without_comparison` | 无 Comparison 不能 Review | PASS |
| UT-SES-03 | `cannot_revalidate_empty_changeset` | 空 ChangeSet 不能进入 REVALIDATING | PASS |
| UT-SES-04 | `comparing_cannot_execute` | Session 在 COMPARING 拒绝 execute/revalidate；cancel→draft | PASS |
| UT-SES-05 | `failure_and_rollback_paths` | COMPARE_FAILED / VALIDATION_FAILED / EXECUTION_FAILED / ROLLED_BACK | PASS |
| UT-ERR-01 | `helpers_and_display` | Validation / Incompatible / Cancelled / IllegalTransition 文案 | PASS |
| UT-ERR-02 | `clones_and_eq` | Error Clone + Eq | PASS |

---

## 3. 覆盖率（仅 `src-tauri/src/data_sync/`）

### 3.1 测量方法

1. `cargo llvm-cov --version` → 已安装 0.8.7。
2. 示例中的 `--ignore-filename-regex '(^|/)(?!data_sync/)'` **不能**把统计范围限制到本模块：llvm-cov 仍对整个 crate 汇总（约 **2.45%** 行覆盖）。**不以整 crate 覆盖率为验收标准。**
3. 未过滤的 `cargo llvm-cov -p datazen --lib` 因**无关**测试失败而中断：
   - `sync::adapter_registry::tests::ensure_type_wire_aliases_succeed`
   - panic：`expected sync adapter for sqlserver`
   - **不属 F1**，未当作本切片缺陷。
4. 实际采用（只跑 `data_sync` 测试，再从 JSON 过滤路径含 `/data_sync/` 的文件）：

```bash
cargo llvm-cov -p datazen --lib --json --output-path docs/progress/f1-coverage.json -- data_sync
```

然后用脚本按文件名过滤。过滤后的数字写入 `docs/progress/f1-coverage.txt`（完整 crate JSON 约 5.9MB，未保留）。

`data_sync` 测试是该模块的唯一覆盖来源（Host 其它测试未引用新域）；过滤测试不影响本模块行覆盖率结论。

### 3.2 各文件行覆盖率

| 文件 | 行 covered | 行 total | 行覆盖率 | 函数 | 区域 |
|---|---:|---:|---:|---:|---:|
| `changeset.rs` | 138 | 138 | **100.00%** | 100.00% | 100.00% |
| `error.rs` | 25 | 25 | **100.00%** | 100.00% | 100.00% |
| `model.rs` | 534 | 536 | **99.63%** | 100.00% | 99.58% |
| `session.rs` | 162 | 166 | **97.59%** | 96.15% | 97.52% |
| `state.rs` | 146 | 147 | **99.32%** | 100.00% | 98.54% |
| **合计** | **1005** | **1012** | **99.31%** | **99.24%** | **99.08%** |

`mod.rs` 仅为模块声明与 re-export，llvm-cov 未单独成文件统计（逻辑在各子模块）。

**是否 ≥ 80%：是（99.31%）。**

### 3.3 未覆盖行（不构成 FAIL）

| 位置 | 说明 |
|---|---|
| `model.rs:211` | `SyncTask::validate` 里 `options.validate()?` 失败分支；非法 options 已由 `SyncOptions::validate` 单测覆盖 |
| `model.rs:234` | `default_selected` 的 `Insert \| Update` 或模式残留 |
| `model.rs:480` | `optional_values_equal(None, None)`；现有测试走 `None` vs `Some(Null)` |
| `session.rs:35,40` | `configure` / `start_compare` 在 `task.validate()` 失败时的 `?` |
| `session.rs:65–68,71` | `generate_change_set` 在无 comparison 时的错误路径 |
| `state.rs:188` | 测试内 `panic!` 兜底臂（非产品路径） |

---

## 4. 规格缺口审查（F1 范围内）

审查范围：领域模型、状态机、ChangeSet 选择规则。§9 的 dialect/PK/结构**运行时**硬门闸、专用 Execute IPC、SQL 生成、UI 属 F2+ / F5+ / F8，标为「切片外」而非本切片 P0 缺陷。

| 规格点 | 期望 | 实际 | 结论 |
|---|---|---|---|
| 方案 §6.1–6.6 类型 | SyncTask / Endpoint / SyncOptions / MatchingStrategy::PrimaryKey / TableMapping / RowChange | 字段与 PRD §24 一致；Endpoint 仅 `connection_id`（无密码） | PASS |
| 默认 options | insert=true, update=true, delete=false, batch_size=1000 | `SyncOptions::default()` 一致；`MatchingStrategy::PrimaryKey`；`LargeValueMode::Full` | PASS |
| §25 主路径 | DRAFT→…→COMPLETED | `can_transition_to` + Session 主路径覆盖 | PASS |
| §25 禁止 COMPARING→EXECUTE | 比较中不得进入执行 | `Comparing` 不能转到 GeneratingSql / ReadyToExecute / Revalidating / Executing / Completed；Session `start_execute`/`start_revalidate` 亦拒绝 | PASS |
| 必须 Review | COMPARED 不能直跳执行 | Compared→Executing / ReadyToExecute 非法；须 Reviewing | PASS |
| DRAFT 不能直接 Compare | 须 CONFIGURED | Draft→Comparing 非法 | PASS |
| DELETE 默认不进 ChangeSet | §8.3 / §28.3 | `default_selected(Delete)=false`（即使 `options.delete=true`）；未勾选不进 set | PASS |
| 未选中行不得执行 | §28.4 | `eligible_for_changeset` 要求 selected ∧ 非 Unchanged ∧ options.allows | PASS |
| INCOMPATIBLE / DISABLED / UNMAPPED 不进 ChangeSet | 方案 §10 | `from_comparison` 只取 `Matched` | PASS |
| 空 ChangeSet 不可执行 | 执行层只接受非空 set | `validate_executable` 拒绝 empty | PASS |
| 含 DELETE 须确认钩子 | §8.3 / §28.3 | `requires_delete_confirmation()` 在 delete_count>0 时为 true（UI 确认属 F8） | PASS |
| 自同步禁止 | §28.8；方案 Safety #8 | 同 connection + database + normalized schema 拒绝；trim 空白 schema 视为相同 | PASS |
| 同连接不同库 | 允许 | `same_connection_different_database_is_allowed` | PASS |
| 值比较不跨类型 | 方案 §7.5 | `values_equal` 同变体；`0≠"0"`、`NULL≠""` | PASS |
| MatchingStrategy V1 仅 PK | 方案 §6.4 | 枚举仅 `PrimaryKey` | PASS |
| 任务不存密码 | §28.5 | Endpoint 无密码字段 | PASS |
| §9 同族 / 相同 PK / 结构一致 | Compare 前硬门闸 | F1 仅有 `TableMappingStatus::Incompatible` 模型；校验在 F2 | 切片外 |
| §28.1 Target read_only 禁 Execute | 安全 | 无 Session/IPC 字段；属 F6 | 切片外 |
| §28.2 专用 Execute IPC | 不走普通 execute_query | F1 无 IPC | 切片外 |
| §28.6 参数化 SQL / quote_ident | 执行安全 | 属 F5 | 切片外 |
| §28.7 CASCADE 警告 | V1 警告即可 | 无模型字段；属后续 | 切片外 |

### 4.1 观察（非 P0 FAIL）

1. **自同步粒度**：PRD §7 写「同一连接 + 同一库 + **映射到自身的同一张表**」；§28.8 与方案 Safety #8 写「同一库自同步禁止」。实现按后者：同库即拒绝（即使表名不同）。与 F1 进度备注一致，**从严且符合 P0 安全条**。同连接 + 同库 + **不同 schema** 仍允许。
2. **`generate_change_set` 错误路径**：若 Reviewing 后 `comparison` 被清空，会先 `go(GeneratingSql)` 再因缺 comparison 返回 Err，会话停在 `GENERATING_SQL`。`begin_review` 已要求有 comparison，正常 API 用不到。建议 F6 补失败回退（不改本切片）。
3. **`fail_compare(_reason)` 丢弃原因**：F1 Session 无 last_error 字段；属后续切片。
4. **现有 `DataSyncWindow` 仍接旧 `syncCommands` / DROP+INSERT 路径**：F1 领域未接线。符合「F1 无新窗口/IPC」；旧引擎停用是 F3，新 UI 是 F8。
5. **`optional_values_equal(None, Some(Null)) == true`**：把缺失单元格与 SQL NULL 视为相等。与 §7.5「NULL≠empty string」不冲突（empty 是 `String("")`）。`None`/`None` 臂当前单测未直接打到，不影响语义。

---

## 5. E2E 测试用例表（供 F8 / F9）

当前**无** Navicat 风格 Data Sync 窗口 / IPC（现有 `data-sync` 窗口走旧引擎）。下列用例全部 **BLOCKED**，步骤按未来 Diff Workspace 编写，便于 F8/F9 直接落地 Host E2E（`e2e/specs/` 契约 journey，不写驱动方言）。

阻塞原因（共性）：**F1 无新窗口 / 无 `data_sync` IPC**；旧 `e2e/specs/data-sync-real.ts` 测的是旧 Transfer 式同步，不能当作本切片验收。

| ID | 步骤 | 期望 | 实际结果 |
|---|---|---|---|
| F1-E2E-01 主路径 Compare→Review→Apply | 1. 打开 Data Sync 窗口 2. 选同源同族 Source/Target（不同库）3. 默认 options 4. 自动映射表 5. Compare 6. Review 行 diff 7. 生成 ChangeSet / SQL Preview 8. 确认后 Execute 9. 再 Compare | 相位不得跳过 Review；Preview 只读；Execute 后差异为 0；INSERT/UPDATE 默认入选，DELETE 不入选 | **BLOCKED**（F1 无窗口/IPC） |
| F1-E2E-02 禁止比较中执行 | 1. 开始 Compare 2. 比较进行中点击 Sync/Execute | 按钮禁用或报非法相位；不得进入 EXECUTING；不得写 Target | **BLOCKED** |
| F1-E2E-03 禁止跳过 Review | Compare 完成后直接点 Execute（不经 Review/ChangeSet） | 拒绝；须先 Reviewing | **BLOCKED** |
| F1-E2E-04 DRAFT 不能 Compare | 未选齐 Source/Target/库 即点 Compare | 停留 DRAFT/校验错误；不进入 COMPARING | **BLOCKED** |
| F1-E2E-05 DELETE 默认关闭 | 打开 Options；Target-only 行存在 | `delete=false`；DELETE 行未勾选；ChangeSet.deleteCount=0；无删行确认 | **BLOCKED** |
| F1-E2E-06 开启 DELETE 两道确认 | 1. 打开 delete 2. 风险文案 3. Compare 4. 勾选若干 DELETE 5. 生成 ChangeSet 6. Execute | 开启时提示 *Records that exist only in the target database will be deleted.*；ChangeSet 含 DELETE 时执行前再确认；未勾选 DELETE 不执行 | **BLOCKED** |
| F1-E2E-07 取消勾选 INSERT 不执行 | Compare 后取消部分 INSERT，再 Apply | 未选行不出现在 SQL Preview / 不写入 Target | **BLOCKED** |
| F1-E2E-08 关闭 insert option | Options.insert=false 后 Compare | INSERT 行默认不选且不可进入 ChangeSet；至少保留 update 或 delete | **BLOCKED** |
| F1-E2E-09 自同步拒绝 | Source=Target（同连接+同库+同 schema）点 Compare | 校验失败「self-sync…not allowed」；不进入 COMPARING | **BLOCKED** |
| F1-E2E-10 同连接不同库允许 | 同一连接、不同 database | 允许进入 Compare（同族前提下） | **BLOCKED** |
| F1-E2E-11 空 ChangeSet 不能 Execute | 全 UNCHANGED 或全部取消选择后点 Sync | 提示 change set is empty；不进入 REVALIDATING/EXECUTING | **BLOCKED** |
| F1-E2E-12 INCOMPATIBLE 表不进 ChangeSet | 映射表无 PK 或结构不一致（F2 门闸就位后） | 摘要 incompatibleCount≥1；该表 0 行进入 ChangeSet；不得只同步列交集 | **BLOCKED**（依赖 F2） |
| F1-E2E-13 摘要口径分离 | Compare 完成看 Summary | 「tables unchanged」与「row inserts」分开；数字与 TableResult 一致 | **BLOCKED** |
| F1-E2E-14 不同表名映射 | `customers`→`clients` 且结构+PK 一致 | MATCHED；ChangeSet.target_table=clients | **BLOCKED**（映射 UI 属 F8，门闸属 F2） |
| F1-E2E-15 Swap 作废结果 | Compare 后 Swap 两端 | Comparison / ChangeSet / SQL 全部作废；需重新 Compare 才能 Execute | **BLOCKED** |
| F1-E2E-16 Cancel Compare | Compare 中点取消 | 进入 CANCELLED；后端查询取消（F4）；可回 DRAFT/CONFIGURED | **BLOCKED**（Cancel 查询属 F4） |
| F1-E2E-17 失败后重试 | Compare 失败 → 再 Compare；Execute 失败 → Rollback → 回 Review | 相位符合 §25 异常边；不残留半执行 ChangeSet | **BLOCKED** |
| F1-E2E-18 Apply 后再 Compare=0 | 主路径成功后立刻再 Compare | 0 insert/update/delete（验收标准） | **BLOCKED**（依赖 F4–F7） |
| F1-E2E-19 Target read_only | Target 连接 read_only：Compare 后 Execute | 允许 Compare；禁止 Execute（§28.1） | **BLOCKED**（属 F6） |
| F1-E2E-20 任务不含密码 | 保存/导出任务 JSON（P1 或调试） | 仅 connection id；无 password | **BLOCKED**（保存任务属 P1；F1 模型已无密码字段） |
| F1-E2E-21 默认 options UI | 新任务 Options 面板 | Insert/Update 开，Delete 关，batch 1000，匹配策略仅 Primary Key | **BLOCKED** |
| F1-E2E-22 值比较展示 | 源 `0` 目标 `"0"`；源 NULL 目标 `''` | 显示为 UPDATE（不同类型/NULL≠空串）；不误判 UNCHANGED | **BLOCKED**（比较引擎属 F4） |

F9 落地建议：将 F1-E2E-01/02/05/06/07/09/11 纳入 Host 契约 journey（PG/MySQL 夹具）；F1-E2E-12/18/19 分别等 F2/F7/F6。

---

## 6. Bug 列表

无。

（环境备注，非 F1 bug：全 crate `cargo llvm-cov -p datazen --lib` 被 `sync::adapter_registry::tests::ensure_type_wire_aliases_succeed` 打断，期望存在 sqlserver sync adapter。与本切片无关。）

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| 单测失败 | 否（30 passed / 0 failed） |
| `data_sync` 行覆盖率 < 80% | 否（**99.31%**） |
| 与 P0 规格冲突的缺陷 | 无 |

**总评：PASS**

F1 领域模型、状态机（含禁止 COMPARING→执行管线）、ChangeSet 选择规则（DELETE 默认不选、未选/options 禁止/非 Matched 不进 set）、options 默认值、自同步拒绝，均与 PRD V1.2 §24/§25/§28 及方案 §6 一致。E2E 待 F8/F9 窗口与 IPC；§9 运行时硬门闸待 F2。
