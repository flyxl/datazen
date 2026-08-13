# F3 QA 报告：停用旧 DROP+INSERT 覆盖拷贝

| 项 | 值 |
|---|---|
| 切片 | F3 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `data_sync/legacy.rs`、`data_sync/pairing.rs`（classify）、`commands/sync/table_sync.rs`（`sync_table_impl` / `sync_tables_impl` 立即拒绝）、`commands/sync/mod.rs`（`classify_sync_pair`）、`src/lib/syncPairing.ts`、`DataSyncWindow.tsx`（横幅 + Start Sync 禁用）、`e2e/specs/data-sync-real.ts`、10 语言 locales |
| 规格 | PRD V1.2「拆除或停用旧 DROP+INSERT」；异构 pair 是 Transfer 不是 Sync |
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
| Node | v22.20.0 |
| vitest | 4.1.10 |
| crate | `datazen` 0.0.9（`src-tauri`） |
| 桌面应用 | **未运行**（`list_running_apps` 无 DataZen；无 `com.datazen` / Tauri 窗口） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f3-coverage.txt`。

---

## 2. 本切片测试设计（对照规格）

规格要点：

- PRD V1.2：增量 Diff Sync，不是克隆/覆盖拷贝，也不是 Transfer。
- 旧引擎：删除/**停用** DROP/CREATE/INSERT 与跨库 IR 拷贝（留给 Transfer）。
- P0：产品内不可再走覆盖拷贝；`sync_table` / `sync_tables` 不得真正 DROP+INSERT。
- 异构 pair（如 PG → MySQL）是 Transfer，Data Sync 不可选。

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-LEG-01 | Rust | `refuse_overwrite_copy` 文案稳定且可检测，含 Diff Sync / Transfer | `data_sync::legacy::tests::refuse_message_is_stable_and_detectable` |
| UT-IPC-01 | Rust | `sync_table_impl` 立即拒绝，不碰连接/DROP | `commands::sync::tests::sync_table_impl_refuses_overwrite_copy` |
| UT-IPC-02 | Rust | `DataSyncError` → `CommandError` 仍带 retired 文案 | `commands::error::tests::from_conversions_work` |
| UT-PAIR-01..07 | Rust | Direct V1 / IR=Transfer / SQLite 非 V1 / Redis·Kiwi 拒绝 / classify 视图 | `data_sync::pairing::tests::*` |
| UT-CLS-01 | Rust IPC | `classify_sync_pair` mysql↔mariadb Direct；PG→MySQL `path=ir, supported=false` | `classify_sync_pair_rejects_ir_and_allows_mysql_family` |
| UT-TS-01..08 | TS | `resolveSyncPairing` 与后端对齐：IR 不可选；仅 mysql/postgresql V1 | `src/lib/__tests__/syncPairing.test.ts` |
| UT-I18N-01 | TS | 10 语言 host key 与 en 对齐（含 `sync.overwriteRetiredBanner` / `sync.applyUnavailable`） | `src/locales/locales.test.ts` |
| E2E-IPC-* | E2E IPC | `sync_table` / `sync_tables` 拒绝；`classify_sync_pair` IR 不可选 | `e2e/specs/data-sync-real.ts`（需运行中的 webdriver 应用） |
| E2E-UI-* | E2E UI | 横幅可见；Start Sync 禁用；异构 Target 不可选 | 规格用例见 §4；**无正在运行的桌面应用 → BLOCKED** |

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cargo test -p datazen --lib data_sync
cargo test -p datazen --lib commands::sync::tests
cargo test -p datazen --lib commands::error::tests
npx vitest run src/lib/__tests__/syncPairing.test.ts src/locales/locales.test.ts
```

| 命令 | 通过 | 失败 | 忽略 | 过滤 | 结果 |
|---|---:|---:|---:|---:|---|
| `data_sync` | **55** | 0 | 0 | 810 | PASS |
| `commands::sync::tests` | **21** | 0 | 0 | 844 | PASS |
| `commands::error::tests` | **3** | 0 | 0 | 862 | PASS |
| vitest（2 files） | **22** | 0 | 0 | — | PASS（测试本身） |

失败详情：无。

编译告警（与 F3 无关，不计入本切片）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`
- `src-tauri/src/dashboard/create.rs` unused variable `registry`

vitest `--coverage` 在默认 `src/lib/**` **statements ≥80%** 门槛上 exit 1（本文件 statements 79.48%）。测试 22/22 已通过；切片门槛按 F1/F2 惯例取 **行覆盖率**，见 §4。

### 3.2 F3 相关单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-LEG-01 | `refuse_message_is_stable_and_detectable` | 停用文案稳定；含 Diff Sync / Transfer | PASS |
| UT-IPC-01 | `sync_table_impl_refuses_overwrite_copy` | `sync_table` 立即拒绝，无 DROP | PASS |
| UT-IPC-02 | `from_conversions_work` | retired 错误可映射为 `CommandError` | PASS |
| UT-PAIR-01 | `mysql_mariadb_is_direct_mysql_family` | 同族 Direct / mysql | PASS |
| UT-PAIR-02 | `postgres_aliases_are_direct` | PG 别名 Direct | PASS |
| UT-PAIR-03 | `pg_to_mysql_is_transfer_not_sync` | 异构 IR → Transfer 文案 | PASS |
| UT-PAIR-04 | `sqlite_same_family_not_v1` | SQLite 非 V1 Sync | PASS |
| UT-PAIR-05 | `redis_and_kiwi_rejected` | 非 SQL / 跨类拒绝 | PASS |
| UT-PAIR-06 | `v1_family_helpers` | V1 仅 mysql + postgresql | PASS |
| UT-PAIR-07 | `classify_view_marks_ir_and_sqlite_unsupported` | classify：IR `supported=false` | PASS |
| UT-CLS-01 | `classify_sync_pair_rejects_ir_and_allows_mysql_family` | IPC `classify_sync_pair` | PASS |
| UT-TS-01 | `same postgresql family is direct` | 前端 Direct | PASS |
| UT-TS-02 | `mysql and mariadb are the same V1 family` | 前端 mysql family | PASS |
| UT-TS-03 | `cross sql dialect is Transfer (IR path, not selectable)` | 前端 IR `supported: false` + Transfer | PASS |
| UT-TS-04 | `cross category is unsupported` | PG↔Mongo / Mongo↔Redis | PASS |
| UT-TS-05 | `same kv/document types are not V1 Data Sync` | Redis / Mongo 不可选 | PASS |
| UT-TS-06 | `sqlite same-family is not V1` | SQLite 不可选 | PASS |
| UT-TS-07 | `normalizeSyncFamily maps wire aliases` | mariadb→mysql，questdb→postgresql | PASS |
| UT-TS-08 | `isSyncTargetSupported only allows V1 families` | 选择器门闸 | PASS |
| UT-I18N-01 | locales 10 语种 + host key parity | `overwriteRetiredBanner` / `applyUnavailable` 十语均有 | PASS |

`data_sync` 其余 47 个用例（F1/F2 模块）全部 PASS，不计入本切片覆盖率门槛。

缺口（非产品缺陷）：**没有** `sync_tables_impl` 的 Rust 单测；拒绝语义由 E2E `SYNC-BATCH-001` / `SYNC-RESUME-003` 覆盖（本次未跑）。源码审查确认 `sync_tables_impl` 与 `sync_table_impl` 同样立即 `refuse_overwrite_copy()`。

---

## 4. 覆盖率（仅本切片核心）

不以整个 `datazen` crate / 整个 `src` 为门槛。

### 4.1 Rust：`legacy.rs` + `pairing.rs`

测量：`cargo llvm-cov -p datazen --lib --json -- data_sync`，再按路径过滤。

| 文件 | 行覆盖 | 行总数 | 行% | 函数% | 区域% |
|---|---:|---:|---:|---:|---:|
| `legacy.rs` | 14 | 14 | **100.00%** | 100.00% | 100.00% |
| `pairing.rs` | 101 | 101 | **100.00%** | 100.00% | 99.36% |
| **合计** | **115** | **115** | **100.00%** | **100.00%** | **99.44%** |

**门槛：合计行覆盖率 ≥ 80% → 满足（100.00%）。**

`pairing.rs` 区域未覆盖：测试模块 L116 `assert!(..., "{msg}")` 格式化臂，不影响产品路径。

### 4.2 TS：`src/lib/syncPairing.ts`

| 指标 | 覆盖 | 总数 | % |
|---|---:|---:|---:|
| **Lines（门槛）** | 31 | 36 | **86.11%** |
| Statements | 31 | 39 | 79.48% |
| Functions | 6 | 6 | 100.00% |
| Branches | 29 | 41 | 70.73% |

**门槛：行覆盖率 ≥ 80% → 满足（86.11%）。**

未覆盖行均为 family/category 回退，不是 overwrite 停用逻辑：kiwi/superset `other`、sqlserver/trino 别名、非 SQL 跨族 `unsupported`。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。本机无正在运行的 DataZen 桌面应用，未启动/构建 webdriver 二进制（禁止改产品代码，且任务要求无运行中应用则标 BLOCKED）。下列步骤与期望仍列出，便于后续实跑。

`e2e/specs/data-sync-real.ts` **已经写好** overwrite retired 断言（`expectOverwriteRetired` / `SYNC-BATCH-001` 等），但依赖运行中的 Tauri + 真实 PG/MySQL。

### 5.1 UI（Data Sync 窗口）

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F3-UI-01 | 打开 Data Synchronization 窗口 | 顶部横幅 `data-testid=data-sync-overwrite-retired` 可见；文案为当前语言 `sync.overwriteRetiredBanner`（提及 DROP + INSERT 已停用） | **BLOCKED**（无运行中应用） |
| E2E-F3-UI-02 | 切 10 种语言各看一次横幅 | 十语种均非空、非缺 key；zh-CN/en 与 locale 文件一致 | **BLOCKED**（key parity 已由单测覆盖） |
| E2E-F3-UI-03 | Source=MySQL，Target 列表含 MariaDB 与 PostgreSQL | MariaDB 可选；PostgreSQL 禁用并带 unsupported hint（IR=Transfer，不可选） | **BLOCKED** |
| E2E-F3-UI-04 | Source=PostgreSQL，选 Target=MySQL | 选择被拒/自动清空；不得显示可 Sync 的 IR 路径 | **BLOCKED** |
| E2E-F3-UI-05 | Source=Target=SQLite 或 Redis | Target 禁用 | **BLOCKED** |
| E2E-F3-UI-06 | 同族两库 Compare 成功后看 footer | Start Sync / Re-sync 按钮 `disabled`；`data-testid=data-sync-start-disabled`；`title=sync.applyUnavailable` | **BLOCKED** |
| E2E-F3-UI-07 | 尝试点击 Start Sync（键盘/脚本） | 不得发出成功的覆盖拷贝；即使误调 IPC 也应 retired 错误 | **BLOCKED** |
| E2E-F3-UI-08 | 若存在旧 `sync_tasks.json` 未完成任务 | 产品不得用 Resume 真正 DROP+INSERT（当前 `SavedTasksBanner` 未挂到窗口；Resume handler 仍会 `invoke('sync_tables')`，后端会拒绝） | **BLOCKED** |

### 5.2 IPC（`e2e/specs/data-sync-real.ts`）

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F3-IPC-01 | `SYNC-REAL-002`：`sync_table` PG→PG | 抛错，消息含 `overwrite copy is no longer Data Synchronization`；目标表不被 DROP | **BLOCKED** |
| E2E-F3-IPC-02 | `SYNC-REAL-003`：拒绝后再 `compare_databases` | 仍 `source_only`（证明未拷贝） | **BLOCKED** |
| E2E-F3-IPC-03 | `SYNC-REAL-007`：结构不一致表 `sync_table` | 同样 retired，不得改写目标 | **BLOCKED** |
| E2E-F3-IPC-04 | `SYNC-REAL-010/011`：只读目标 | 在权限错误之前即 retired | **BLOCKED** |
| E2E-F3-IPC-05 | `SYNC-REAL-021..023`：PG→MySQL `sync_table` | 全部 retired（不再「跨库成功拷贝」） | **BLOCKED** |
| E2E-F3-IPC-06 | `SYNC-BATCH-001`：`sync_tables` 三表 full | retired；目标无新表 | **BLOCKED** |
| E2E-F3-IPC-07 | `SYNC-BATCH-002`：`classify_sync_pair` PG→MySQL | `{ path: 'ir', supported: false }` | **BLOCKED** |
| E2E-F3-IPC-08 | `SYNC-BATCH-003`：批量场景下 `sync_table` | retired | **BLOCKED** |
| E2E-F3-IPC-09 | `SYNC-RESUME-003`：`strategy=continue` 的 `sync_tables` | retired（续传也不能走覆盖拷贝） | **BLOCKED** |
| E2E-F3-IPC-10 | `SYNC-BATCH-004`：task CRUD | 仍可用；与 overwrite 解耦 | **BLOCKED** |

源码对照：上述 E2E 期望与当前 `data-sync-real.ts` 断言一致。

---

## 6. 规格缺口审查（能否真正 DROP+INSERT）

对照 PRD V1.2「拆除或停用」与实施方案「P0 结束前产品内不可达」。

### 6.1 产品路径（可达）

| 入口 | 行为 | 能否真正 DROP+INSERT |
|---|---|---|
| Tauri `sync_table` → `sync_table_impl` | 立刻 `Err(refuse_overwrite_copy())`，忽略连接/表名 | **否** |
| Tauri `sync_tables` → `sync_tables_impl` | 立刻拒绝，不写 task、不 emit progress、不执行 SQL | **否** |
| 前端 Start Sync 按钮 | 恒 `disabled` + `sync.applyUnavailable` | **否**（UI 不可点） |
| 前端 `resolveSyncPairing` / Target 选择 | 异构 IR `supported: false`，选择器禁用 | 不可选 IR 拷贝 |
| `classify_sync_pair` IPC | 与后端 `classify_data_sync_pair` 一致 | IR 不可选 |
| MCP | 未暴露 `sync_table` / `sync_tables` | 无 |

结论：**`sync_table` / `sync_tables` 不能真正执行 DROP+INSERT。** 不记 FAIL。

复现（若回归）：

1. 任意已连接的同族或异构 pair，IPC `invoke('sync_table', { sourceConnectionId, targetConnectionId, tableName })`。
2. 期望：错误字符串包含 `overwrite copy is no longer Data Synchronization`。
3. 再 `compare_databases` 或直接查目标库：表结构/行数不变。
4. 对 `sync_tables`（含 `strategy: 'continue'`）重复 1–3。

### 6.2 残留（不停用失败，不记 Bug）

1. **死代码仍含 DROP+INSERT**：`sync_one_table` / `sync_table_impl_legacy` / `sync_tables_impl_legacy`（`#[allow(dead_code)]`）。未 export 到 IPC，`mod.rs` 只 re-export 拒绝版。属「停用未拆除」，符合规格「拆除**或**停用」。
2. **`DataSyncWindow` 仍有 `startSync` / `handleResumeConfirm` 调用 `sync_tables`**：Start Sync 已禁用；`SavedTasksBanner` **未挂载**，Resume 对话框默认关。若将来重新挂上 Resume，后端仍会拒绝，不会真正覆盖。属后续 UX 清理。
3. **`src/commands/sync.ts` 仍导出 `syncTable` / `syncTables` 包装**：调用即 IPC 拒绝。
4. **无 `sync_tables_impl` 单测**：实现与 `sync_table_impl` 对称拒绝；E2E 有断言但本次 BLOCKED。

以上 **不构成**「仍能真正 DROP+INSERT」。

---

## 7. Bug 列表

无。

---

## 8. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib data_sync` | 55 passed / 0 failed |
| `cargo test -p datazen --lib commands::sync::tests` | 21 passed / 0 failed |
| `cargo test -p datazen --lib commands::error::tests` | 3 passed / 0 failed |
| vitest `syncPairing` + `locales` | 22 passed / 0 failed |
| Rust `legacy.rs`+`pairing.rs` 行覆盖 ≥80% | **100.00%** |
| TS `syncPairing.ts` 行覆盖 ≥80% | **86.11%** |
| `sync_table` / `sync_tables` 仍能真正 DROP+INSERT | **否**（立即拒绝） |
| 10 语言 `overwriteRetiredBanner` / `applyUnavailable` | 均存在；locales parity PASS |
| UI/IPC E2E | 全部 **BLOCKED**（无运行中桌面应用） |
| P0 规格冲突缺陷 | 无 |

**总评：PASS**
