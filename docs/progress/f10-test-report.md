# F10 QA 报告：拆除旧 DROP+INSERT 执行残留 + 架构/AGENTS 文档

| 项 | 值 |
|---|---|
| 切片 | F10 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 分支 | `feat/data-sync-navicat` |
| 被测代码 | `src-tauri/src/commands/sync/table_sync.rs`、`compare.rs`、`types.rs`、`tests.rs`（`table_sync_module_has_no_drop_insert_body` / `sync_table_impl_refuses_overwrite_copy`）、`src-tauri/src/data_sync/legacy.rs`；文档 `docs/architecture/backend/data-sync.md`、`docs/architecture/README.md`、`docs/architecture/backend/commands.md`、`docs/architecture/backend/drivers.md`、`docs/architecture/testing.md`、`docs/architecture/windows.md`、`AGENTS.md` |
| 规格 | 拆除旧 DROP+INSERT 执行体；`table_sync.rs` 仅 `refuse_overwrite_copy`；无 `sync_one_table` / legacy / `DROP TABLE`；`compare.rs` 无 `fetch_table_options` / `resolve_adapters`；`types.rs` 无 `SyncProgressEvent` / `BATCH_SIZE`；架构文档区分 Sync / Transfer / Structure Sync；`AGENTS.md` 含 `data_sync/` 与「Data Synchronization ≠ Transfer」 |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f10-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **FAIL**（单测 25/25；静态审查 PASS；**table_sync.rs Lines 35.71% < 80%**；E2E 全部 **BLOCKED**；产品 DROP+INSERT 执行体 0） |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| toolchain | stable-aarch64-apple-darwin |
| cargo-llvm-cov | 0.8.7 |
| cargo-tarpaulin | **未安装**（未使用；llvm-cov 可用） |
| crate | `datazen` 0.0.9（`src-tauri`） |
| 桌面应用 | **未运行** |
| webdriver 二进制 | **不存在** |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f10-coverage.txt`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Microsoft Edge。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程
- `target/debug/datazen`：**不存在**
- `target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen`：**不存在**
- `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**（`lsof` exit 1）
- `e2e/.env`：**不存在**（仅有 `.env.example`）

尝试执行（按任务要求）：

```bash
pnpm e2e:skip-build -- --spec e2e/specs/data-sync-real.ts
```

结果：**exit 1**（runner 在 `assertBinaryReady` 失败；未进入 WDIO / 未跑 SYNC-BATCH-001/003）。

```text
[e2e-runner] Skipping build (--skip-build). Binary MUST come from a prior Tauri webdriver build.
[e2e-runner] WARNING: e2e/setup-e2e-env.sh failed. DB specs may fail; UI-only specs can still run.
[e2e-runner] E2E binary not found: .../data-sync-navicat/target/debug/datazen
psql: FATAL: role "postgres" does not exist
```

**结论：`data-sync-real` SYNC-BATCH-001/003 均 BLOCKED，不假装 PASS，也不单独记 FAIL。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围（对照规格）

- `table_sync.rs` 仅 `sync_table_impl` / `sync_tables_impl` → `refuse_overwrite_copy()`；无 `DROP TABLE` / `sync_one_table` / `sync_table_impl_legacy`
- `compare.rs` 删除 `fetch_table_options` / `resolve_adapters`
- `types.rs` 删除 `SyncProgressEvent` / `BATCH_SIZE`（现仅 compare sample/mismatch 常量）
- 单测：`sync_table_impl_refuses_overwrite_copy`、`table_sync_module_has_no_drop_insert_body`；另跑 `commands::sync::tests` 全模块 + `data_sync::legacy`
- 文档：`data-sync.md` 区分 Sync / Transfer / Structure Sync；`AGENTS.md` 含 `data_sync/` 与「Data Synchronization ≠ Transfer」
- Host E2E：`e2e/specs/data-sync-real.ts` 的 SYNC-BATCH-001 / SYNC-BATCH-003（refuse overwrite）
- 覆盖率门槛：`table_sync.rs` llvm-cov **Lines ≥ 80%**

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-TS-01 | Rust `sync_table_impl` | 单表覆盖拷贝 IPC 立即拒绝 | `sync_table_impl_refuses_overwrite_copy` |
| UT-TS-02 | Rust 源码断言 | `table_sync.rs` 无 `DROP TABLE` / `sync_one_table` / `sync_table_impl_legacy`，且含 `refuse_overwrite_copy` | `table_sync_module_has_no_drop_insert_body` |
| UT-TS-03 | Rust `sync_tables_impl` | 批量覆盖拷贝 IPC 立即拒绝 | **缺失**（L16/29–32 count=0；llvm-cov 未击中） |
| UT-LEG-01 | Rust `data_sync::legacy` | 退役文案稳定且可检测 | `refuse_message_is_stable_and_detectable` |
| ST-01 | 静态 | `table_sync.rs` 无执行体残留 | 本报告 §3.3 / §6 |
| ST-02 | 静态 | `DataSyncWindow.tsx` 不 `invoke('sync_tables'` | 本报告 §3.3 |
| ST-03 | 静态 | `data-sync.md` 区分三类产品 | 本报告 §3.3 |
| ST-04 | 静态 | `AGENTS.md` `data_sync/` + ≠ Transfer | 本报告 §3.3 |
| E2E-F10-01 | E2E | `sync_tables` refuse | **BLOCKED**（SYNC-BATCH-001） |
| E2E-F10-02 | E2E | `sync_table` refuse | **BLOCKED**（SYNC-BATCH-003） |

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有二进制则 BLOCKED，不新建构建）
- 行 Diff / ChangeSet / SQL Preview / `execute_data_sync` 窗口接线（后续切片）
- 驱动方言 / Redis / Kiwi 专属 UI
- `compare.rs` / `inspect.rs` 覆盖率（非本切片门槛）

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat

cargo test -p datazen --lib commands::sync::tests -- --nocapture
cargo test -p datazen --lib data_sync::legacy -- --nocapture
```

| 命令 | 通过 | 失败 | 忽略 | 过滤 | 结果 |
|---|---:|---:|---:|---:|---|
| `commands::sync::tests` | **24** | 0 | 0 | 865 | **PASS**（0.05s；含编译 9.02s） |
| `data_sync::legacy` | **1** | 0 | 0 | 888 | **PASS**（0.00s） |
| **合计（本切片必跑）** | **25** | 0 | 0 | — | **PASS** |

失败详情：无。

`commands::sync::tests` 24 项全部 ok，其中包括本切片钉桩：

- `sync_table_impl_refuses_overwrite_copy`
- `table_sync_module_has_no_drop_insert_body`

以及既有 compare / inspect / classify / execute / task CRUD 等（非本切片门槛，记录为全绿）。

编译告警（既有，非本切片引入，不记缺陷）：`dashboard/execute.rs` unused import `RefreshMode`；`dashboard/create.rs` unused `registry`；`data_sync/execute.rs` unused/mut `exec`。

### 3.2 已落地单测清单

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-TS-01 | `sync_table_impl_refuses_overwrite_copy` | `sync_table_impl` 返回 Err；`is_overwrite_copy_retired_message` | PASS |
| UT-TS-02 | `table_sync_module_has_no_drop_insert_body` | `include_str!("table_sync.rs")` 不含 `DROP TABLE` / `sync_one_table` / `sync_table_impl_legacy`；含 `refuse_overwrite_copy` | PASS |
| UT-TS-03 | （无） | `sync_tables_impl` 同样 refuse | **缺失** |
| UT-LEG-01 | `refuse_message_is_stable_and_detectable` | `OVERWRITE_COPY_RETIRED` 含 Transfer / Diff Sync；与 `is_overwrite_copy_retired_message` 对齐 | PASS |

### 3.3 静态对照（不改代码）

| 检查项 | 结果 | 证据 |
|---|---|---|
| `table_sync.rs` 无 `DROP TABLE` / `sync_one_table` / `sync_table_impl_legacy` | **PASS** | 文件 32 行；仅两处 `refuse_overwrite_copy()`；`rg` 无匹配；UT-TS-02 同步断言 |
| `table_sync.rs` 仅 refuse，无 DROP+INSERT 执行体 | **PASS** | `sync_table_impl` L5–13、`sync_tables_impl` L16–32 均 `tracing::info` + `Err(...)` |
| `compare.rs` 无 `fetch_table_options` / `resolve_adapters` | **PASS** | `rg` 无匹配 |
| `types.rs` 无 `SyncProgressEvent` / `BATCH_SIZE` | **PASS** | 仅 `DATA_COMPARE_SAMPLE_LIMIT` / `DATA_COMPARE_MISMATCH_LIMIT`（2 行） |
| `DataSyncWindow.tsx` 不 `invoke('sync_tables'` | **PASS** | 窗口只 `invoke('get_connections')` + `syncCommands.inspectDataSync`；`rg` 无 `sync_tables` |
| `docs/architecture/backend/data-sync.md` 存在且区分三类 | **PASS** | 表格：Data Synchronization / Data Transfer / Structure Sync；写明 DROP+INSERT 已拆除 |
| `AGENTS.md` 含 `data_sync/` 与 ≠ Transfer | **PASS** | 目录树 L42 `data_sync/`；L231 `Data Synchronization ≠ Transfer ≠ Structure Sync` |
| 其余架构文档点到 Sync ≠ Transfer / 覆盖拷贝拆除 | **PASS** | `README.md` Sync 层 + 数据同步条目；`commands.md` L399；`drivers.md` L979；`testing.md` L214；`windows.md` L17 Diff Workspace |

**不记缺陷**（兼容残留，符合「IPC 立即拒绝」而非执行）：

- `src/commands/sync.ts` 仍导出 `syncTables` / `syncTable` → `invoke('sync_tables'|'sync_table')`，供 E2E 打 refuse 路径；窗口未调用。
- `commands/sync/mod.rs` 仍注册 `sync_table` / `sync_tables` IPC，转发到 `*_impl` refuse。

---

## 4. 覆盖率

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f10-cov/f10-coverage.json -- \
  commands::sync::tests::sync_table_impl_refuses_overwrite_copy \
  commands::sync::tests::table_sync_module_has_no_drop_insert_body
```

llvm-cov 跑到的测试：**2 passed / 0 failed**（887 filtered）。JSON 按路径 `/commands/sync/table_sync.rs` 过滤。

| 文件 | Lines（门槛） | Funcs | Regions |
|---|---|---|---|
| `table_sync.rs` | **35.71%（10/28）** | 50.00%（2/4） | 35.71%（5/14） |

唯一源码行（hasCount segments，非正式门槛）：**50.00%（5/10）** — COVERED L5/10/12/13，PARTIAL L11，UNCOVERED L16/29–32。

**门槛：table_sync.rs Lines ≥ 80% → 35.71% → FAIL。**

根因（测试缺口，不是 DROP+INSERT 残留）：指定的两条测试只**执行** `sync_table_impl`；`table_sync_module_has_no_drop_insert_body` 是 `include_str!` 文本断言，不跑函数。`sync_tables_impl`（批量 refuse）全程 count=0。`commands::sync::tests` 全模块也没有调用 `sync_tables_impl` 的用例。

未覆盖行见 `f10-coverage.txt`。llvm-cov 可用，故未走 tarpaulin / BLOCKED 降级。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 无 Tauri webdriver debug 二进制（`pnpm e2e:skip-build` 在 `assertBinaryReady` 失败）
3. 无 `dist/index.html`
4. 无 `e2e/.env`；`setup-e2e-env.sh` 因 `role "postgres" does not exist` 失败（额外挡住真实 PG 夹具）

未启动应用，未执行任何 WDIO / IPC 断言。**不把单元测试结果记为 E2E PASS。按任务约定 E2E BLOCKED 不单独导致 FAIL。**

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F10-01 | IPC `sync_tables` 批量覆盖拷贝（`data-sync-real.ts` SYNC-BATCH-001） | 拒绝；message 含 `overwrite copy is no longer Data Synchronization` | **BLOCKED**（无 webdriver 二进制 + 无 `e2e/.env` + 无 `postgres` 角色）。UT-TS-03 亦缺失，仅文档/源码显示 `sync_tables_impl` 走 refuse |
| E2E-F10-02 | IPC `sync_table` 单表覆盖拷贝（SYNC-BATCH-003） | 同上 refuse 文案 | **BLOCKED**。UT-TS-01 覆盖同断言（Rust 层） |

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`（或等价 `scripts/e2e-tauri-build.mjs`），得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 复制 `e2e/.env.example` → `e2e/.env`，填入本机 PG（当前 Homebrew PG 无 `postgres` 角色）
3. `pnpm e2e:skip-build -- --spec e2e/specs/data-sync-real.ts`，至少断言 SYNC-BATCH-001 / SYNC-BATCH-003

---

## 6. 缺陷列表

| ID | 严重度 | 标题 | 说明 |
|---|---|---|---|
| **F10-BUG-01** | **P0（门槛）** | `table_sync.rs` 行覆盖 35.71% < 80% | llvm-cov 在指定两条测试下 Lines **10/28 = 35.71%**（unique source 5/10 = 50%）。`sync_tables_impl`（L16, L29–32）从未执行。缺少与 UT-TS-01 对称的 `sync_tables_impl` refuse 单测。**不是** DROP+INSERT 执行体残留。 |

产品缺陷（DROP+INSERT 执行体仍在 / 窗口仍 `invoke('sync_tables'` / 文档未区分三类）：**无。**

**不记缺陷**（测试/后续切片）：

1. `src/commands/sync.ts` 保留 `syncTables` 封装 — 兼容 IPC + E2E refuse；窗口未调用。
2. llvm-cov `functions` 计 4 个（async 脱糖）；2/4 未覆盖同属 `sync_tables_impl`。
3. `compare.rs` 注释仍有 schema 语义 “extraOnTarget → DROP”（列 DROP，不是 `DROP TABLE` 覆盖拷贝）。
4. E2E SYNC-BATCH-001/003 BLOCKED — 按约定不单独 FAIL。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib commands::sync::tests` | **24 passed / 0 failed** |
| `cargo test -p datazen --lib data_sync::legacy` | **1 passed / 0 failed** |
| `table_sync.rs` Lines ≥80% | **35.71%（10/28）→ FAIL** |
| 静态：无 `DROP TABLE` / `sync_one_table` / `sync_table_impl_legacy` | **PASS** |
| 静态：`DataSyncWindow.tsx` 不 `invoke('sync_tables'` | **PASS** |
| 静态：`data-sync.md` 区分 Sync / Transfer / Structure Sync | **PASS** |
| 静态：`AGENTS.md` `data_sync/` + ≠ Transfer | **PASS** |
| 与 P0 规格冲突的 DROP+INSERT 执行体 | **无** |
| E2E SYNC-BATCH-001 / 003 | 全部 **BLOCKED**（无桌面应用、无 webdriver 二进制） |
| 记入缺陷 | **1**（F10-BUG-01 覆盖率门槛） |

**总评：FAIL**

F10 拆除目标在源码与文档上已达成：`table_sync.rs` 只剩 ~32 行 refuse 桩；`compare.rs` / `types.rs` 已去掉指定符号；窗口不再调用 `sync_tables`；架构与 AGENTS 已写明 Sync ≠ Transfer ≠ Structure Sync，且旧 DROP+INSERT 已拆除。单测全绿。失败原因是覆盖率门槛：指定的两条测试无法执行 `sync_tables_impl`，llvm-cov Lines **35.71% < 80%**。E2E 因本机无 DataZen / 无 webdriver debug 二进制全部 BLOCKED，**不单独导致 FAIL**。补测建议：增加 `sync_tables_impl` refuse 单测后重跑同一 llvm-cov 过滤；有应用后再跑 SYNC-BATCH-001/003。
