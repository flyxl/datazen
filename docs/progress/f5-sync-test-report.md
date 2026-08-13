# F5 QA 报告：Data Sync 剩余接线（Cancel / 行比较 / Apply）

| 项 | 值 |
|---|---|
| 切片 | F5（Data Sync 剩余接线；非 Transfer / 非 Web 菜单 F1–F4 / 非 AGENTS F6） |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus` |
| 分支 | `feat/web-context-menus`（HEAD `d331831`） |
| 被测代码 | `src-tauri/src/commands/sync/{jobs,exec,apply,mod}.rs`；`src/windows/data-sync/{DataSyncWindow.tsx,mappingView.ts}`；`src/commands/sync.ts`（对照）；`src/locales/*`（`sync.rowDiffs`） |
| 规格 | ① `cancel_data_sync(jobId)` 与 `execute_data_sync(..., jobId)` 共用同一 `AtomicBool`；取消后 execute 失败/回滚。② `compare_data_sync` 对 MATCHED 表做 PK 行比较，返回 `rows`（INSERT/UPDATE/DELETE/UNCHANGED）。③ `apply_data_sync` 生成 ChangeSet SQL 并走专用 execute；空 ChangeSet 拒绝。前端 Compare 后显示 `sync.rowDiffs`，有差异时 Apply 可点；同步中可 Cancel |
| 测试角色 | **全新独立验收会话**；未修改任何产品代码；未 commit；只写本报告 + `f5-sync-coverage.txt` |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（Rust `commands::sync` 31/31；jobs+exec+apply Lines **83.97%** ≥80%；前端 26/26；mappingView Lines **95.23%**；DataSyncWindow Lines **85.91%**；E2E 全部 **BLOCKED**；产品缺陷 0） |

---

## 1. 测试环境

| 工具 | 版本 |
|---|---|
| OS | macOS darwin 24.6.0 (arm64) |
| rustc | 1.90.0 (1159e78c4 2025-09-14) |
| cargo | 1.90.0 (840b83a10 2025-07-30) |
| cargo-llvm-cov | 0.8.7 |
| Node | v22.20.0 |
| vitest | 4.1.10 |
| crate / app | `datazen` 0.0.9（`package.json` / `src-tauri/tauri.conf.json`） |
| 桌面应用（本分支） | **未运行 / 未构建** |
| webdriver 二进制 | **不存在**（本 worktree 无 `target/debug/datazen`） |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus
```

覆盖率原始摘要：`docs/progress/f5-sync-coverage.txt`。

### 1.1 桌面应用 / E2E 前置探测

computer-use MCP：

- `list_running_apps`：访达 / Microsoft Edge / Sublime Text / 微信 / iTerm2 / Cursor / 终端。**无 DataZen。**
- `list_windows`：仅 Cursor（`Cursor Agents`）。**无 DataZen 窗。**

本机探测：

- `ps`：无 `datazen` 应用进程（仅 Cursor extension-host 工作区名含 datazen）
- 本 worktree `target/debug/datazen`：**不存在**
- 本 worktree `src-tauri/target/debug/datazen`：**不存在**
- 本 worktree `target/debug/bundle/macos/DataZen.app`：**不存在**
- 本 worktree `dist/index.html`：**不存在**
- `127.0.0.1:4445`：**未监听**
- `e2e/.env`：**不存在**（仅有 `.env.example`）
- Host `e2e/specs/data-sync-window.ts`：存在 DSW-001~005 / DSW-MAP-001，**未覆盖** rowDiffs / Apply 执行 / Cancel

旁路（**不可用于本切片验收**）：

- `/Applications/DataZen.app` 存在，版本 **0.0.8**，时间戳 2026-08-07，**未运行**。与本分支 0.0.9 / `feat/web-context-menus` 不是同一构建，禁止用旧包冒充 F5 E2E。
- 主仓 `/Users/wuxiaolong/code/rust-projects/datazen/target/debug/datazen` 存在（2026-08-13 14:36），**非本 worktree、非 webdriver 监听**，未启动。

**结论：Host E2E 与 computer-use 黑盒均 BLOCKED，不假装 PASS。**

---

## 2. 范围 / 非范围

### 2.1 本切片范围

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-JOB-01 | Rust jobs | `cancel_job` 写入 flag；后续 `ensure_job` 读到 true | `cancel_before_execute_keeps_the_flag` |
| UT-EXEC-01 | Rust exec+jobs | 先 `cancel_job` 再 `execute_data_sync_impl(..., Some(job))` → 失败且文案含 cancel | `cancel_data_sync_stops_execute_before_start` |
| UT-EXEC-02 | Rust exec | 只读目标拒绝 execute | `execute_data_sync_rejects_read_only_target` |
| UT-CMP-01 | Rust apply | MATCHED 表 `compare_data_sync_impl` 填 `rows` | `compare_data_sync_fills_row_diff_for_matched_tables` |
| UT-APL-01 | Rust apply | 空 ChangeSet → apply 拒绝（empty / nothing） | `apply_data_sync_rejects_empty_change_set` |
| UT-APL-02 | Rust apply | MySQL `` ` `` / PG `"` | `mysql_uses_backticks_postgres_uses_double_quotes` |
| UT-MAP-01~04 | TS mappingView | 状态文案 / 改名显示 / 汇总 / INSERT·UPDATE·DELETE 计数（UNCHANGED 不计差异） | `mappingView.test.ts` 4 cases |
| UT-DSW-01~06 | TS DataSyncWindow | 横幅；空 Compare；同族 Compare + `sync.rowDiffs` + Apply 可点并调用 `applyDataSync`；异构禁用；连接/门闸错误；全选 | `DataSyncWindow.test.tsx` 6 cases |
| UT-I18N-SYNC | TS locales | `sync.rowDiffs` / `sync.applyUnavailable` 等 10 语种非空 | `locales.test.ts` Data Sync keys |
| E2E-F5-SYNC-01~04 | E2E | 真窗 Compare rowDiffs / Apply / 空差异禁用 / 同步中 Cancel | **BLOCKED**（无本分支应用 / 无 webdriver） |

同模块顺带跑到、属既有 Host 同步 IPC（不单独当 F5 门槛）：`inspect_data_sync`、旧 `sync_tables` 拒绝覆盖拷贝、task CRUD、`classify_sync_pair` 等。`cargo test -- commands::sync` 合计 **31**。

### 2.2 非范围

- 不修代码、不改产品逻辑、不 commit
- 不跑 `pnpm tauri build --debug --features webdriver`（无既有本 worktree 二进制则 BLOCKED，不新建构建）
- 不启动 `/Applications/DataZen.app` 0.0.8 旧包
- Transfer / 异构 IR；Web 右键菜单（F1–F4）；AGENTS 文档（F6）
- `data_sync::{sql,execute,compare,changeset}` 生成器本体（先前切片已测；本切片只验收 IPC 接线）

---

## 3. 单元测试

### 3.1 Rust

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/web-context-menus
cargo llvm-cov --package datazen --lib --json --output-path /tmp/f5-sync-cov.json -- commands::sync
```

| 命令 | 通过 | 失败 | 忽略 | 过滤 | 结果 |
|---|---:|---:|---:|---:|---|
| `commands::sync`（llvm-cov 内嵌 test） | **31** | 0 | 0 | 883 | **PASS**（0.05s；编译约 40s） |

失败详情：无。

编译告警（不计入本切片）：`dashboard` unused import/variable；`plugin_init.rs` unexpected cfg；`data_sync/execute.rs` 测试内 unused `exec`。

#### 3.1.1 F5 规格对照

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-JOB-01 | `jobs::tests::cancel_before_execute_keeps_the_flag` | `cancel_job` → `ensure_job` 同一 `Arc<AtomicBool>` 为 true | PASS |
| UT-EXEC-01 | `cancel_data_sync_stops_execute_before_start` | 先 cancel 再 `execute_data_sync_impl(..., Some(job))`；错误含 `cancel`；与 IPC `cancel_data_sync`/`execute_data_sync` 同 `jobs` map | PASS |
| UT-EXEC-02 | `execute_data_sync_rejects_read_only_target` | 只读目标不执行 | PASS |
| UT-CMP-01 | `compare_data_sync_fills_row_diff_for_matched_tables` | MATCHED `users` 的 `rows` 非空（夹具同源同数据 → 实际为 UNCHANGED，仍属四种操作之一） | PASS |
| UT-APL-01 | `apply_data_sync_rejects_empty_change_set` | 全 UNCHANGED → ChangeSet 空 → `execute_statements`「change set is empty; nothing to execute」 | PASS |
| UT-APL-02 | `mysql_uses_backticks_postgres_uses_double_quotes` | `ident_quote` | PASS |

静态对照（不改代码）：

- `mod.rs`：`cancel_data_sync` → `cancel_job`；`execute_data_sync` / `compare_data_sync` / `apply_data_sync` 均把 `job_id` 传入 impl。
- `exec.rs`：`job_id` 有值则 `jobs::ensure_job`，把 `Arc<AtomicBool>` 交给 `execute_statements`；返回后 `remove_job`。与 `cancel_job` 同一 `JOBS` map。
- `data_sync/execute.rs`：空语句 → validation；开始前已 cancel → `Err(cancelled)`；循环中 cancel → `rollback` 且 `rolled_back: true`。本切片 IPC 单测打到「开始前 cancel 失败」，**未**经 `LiveExecutor` 打到循环中回滚（见覆盖率缺口）。
- `apply.rs`：`compare_data_sync_impl` 对 MATCHED 调 `compare_sorted_rows` 填 `TableResult::matched(..., rows)`；`apply_data_sync_impl` → compare → `ChangeSet::from_comparison` → `generate_table_sql` → `execute_data_sync_impl`。空集在 execute 入口拒绝，与 UT-APL-01 一致。

### 3.2 前端

```bash
npx vitest run \
  src/windows/data-sync/__tests__/DataSyncWindow.test.tsx \
  src/windows/data-sync/__tests__/mappingView.test.ts \
  src/locales/locales.test.ts \
  --coverage \
  --coverage.include='src/windows/data-sync/DataSyncWindow.tsx' \
  --coverage.include='src/windows/data-sync/mappingView.ts' \
  --reporter=verbose
```

| 命令 | 通过 | 失败 | 忽略 | 结果 |
|---|---:|---:|---:|---|
| vitest 3 files（verbose） | **26** | 0 | 0 | **PASS**（2.21s） |

分文件：

| 文件 | 通过 | 失败 |
|---|---:|---:|
| `mappingView.test.ts` | 4 | 0 |
| `DataSyncWindow.test.tsx` | 6 | 0 |
| `locales.test.ts` | 16 | 0 |

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-MAP-01 | `labels every mapping status` | 五种 mapping 文案 key | PASS |
| UT-MAP-02 | `displays renamed mappings` | `src → tgt` / 同名 / 仅目标 | PASS |
| UT-MAP-03 | `summarizes mapping buckets` | matched / incompatible / unmapped | PASS |
| UT-MAP-04 | `counts INSERT/UPDATE/DELETE row diffs` | 1/2/1；UNCHANGED 不计入；`tableHasRowDiffs` | PASS |
| UT-DSW-01 | `shows the overwrite-retired banner and idle prompt` | 横幅 + idle + Compare | PASS |
| UT-DSW-02 | `prompts to select both endpoints when Compare is clicked empty` | `sync.selectBoth`；不调 compare | PASS |
| UT-DSW-03 | `compares same-family connections and enables Apply when row diffs exist` | `compareDataSync(..., jobId)`；文案 `sync.rowDiffs`；`data-sync-start` 可点；`applyDataSync(..., ['users'], jobId)` | PASS |
| UT-DSW-04 | `marks heterogeneous targets as unsupported` | MySQL 目标带 `sync.unsupportedHint` | PASS |
| UT-DSW-05 | `shows inspect errors and connect failures` | connect / compare 错误对话框 | PASS |
| UT-DSW-06 | `toggles MATCHED rows and select-all / deselect-all` | 无 rows 的 MATCHED；INCOMPATIBLE 原因；全选 | PASS |
| UT-I18N-SYNC | `resolves Data Sync workspace keys for every locale` | 含 `sync.rowDiffs`、`sync.applyUnavailable` | PASS |

前端接线静态对照：

- `syncCommands.compareDataSync` / `applyDataSync` / `cancelDataSync` / `executeDataSync` 均传 `jobId`。
- Compare 成功后 MATCHED 且 `tableHasRowDiffs` → 右侧 `t('sync.rowDiffs', rowDiffCounts(row))`；Apply `disabled` 当且仅当没有「已选 MATCHED + 有行差异」。
- 同步中：`syncState === 'syncing'` 时底栏出现 `data-testid="data-sync-cancel"` → `cancelDataSync(jobIdRef)`。

UT 未击中但代码存在（记测试缺口，不记产品缺陷）：

1. `handleCancel` / Cancel 按钮点击（L167–170、L392）
2. Apply `rolledBack` 提示（L195–199）
3. Apply 过程中 connect 失败 / apply throw
4. `sourceId === targetId` 的 `sync.cannotSame`（目标选择已被 useEffect 清掉）
5. 勾选框从取消再勾回（L359）
6. Dialog `onClose`（测试走 `common.ok`）

---

## 4. 覆盖率

### 4.1 Rust（门槛：三文件合计 Lines ≥ 80%）

| 文件 | 行 covered | 行 total | **Lines（门槛）** | 函数 | 区域 |
|---|---:|---:|---:|---:|---|
| `jobs.rs` | 28 | 28 | **100%** | 100%（10/10） | 100%（49/49） |
| `exec.rs` | 37 | 49 | **75.51%** | 27.27%（3/11） | 63.93%（39/61） |
| `apply.rs` | 134 | 160 | **83.75%** | 73.33%（11/15） | 75.67%（199/263） |
| **合计** | **199** | **237** | **83.97%** | 66.67%（24/36） | 76.94%（287/373） |

**门槛：jobs+exec+apply Lines ≥ 80% → 83.97% PASS。** `exec.rs` 单独 75.51% 不是独立门槛；缺口为 `LiveExecutor::{begin,execute,commit,rollback}`（现有用例在 begin 前返回）。

### 4.2 前端（门槛：mappingView Lines ≥ 80%；DataSyncWindow 尽量 ≥ 80%）

| 文件 | Stmts | Branch | Funcs | **Lines（门槛）** |
|---|---|---|---|---|
| `mappingView.ts` | 95.23%（20/21） | 94.11%（16/17） | 100%（11/11） | **95.23%（20/21）** |
| `DataSyncWindow.tsx` | 84.04%（137/163） | 83.33%（100/120） | 93.18%（41/44） | **85.91%（122/142）** |
| **合计（include）** | **85.32%（157/184）** | 84.67%（116/137） | 94.54%（52/55） | **87.11%（142/163）** |

**mappingView.ts 95.23% ≥ 80% → PASS。DataSyncWindow.tsx 85.91% ≥ 80% → PASS。** Compare / Apply / rowDiffs 用例均 PASS。Cancel 点击未覆盖，属缺口，不单独 FAIL。

---

## 5. E2E 用例表

现状：**全部 BLOCKED**。原因叠加：

1. computer-use 确认无 DataZen 窗口 / 进程
2. 本 worktree 无 Tauri webdriver debug 二进制
3. 无 `dist/index.html`、无 `e2e/.env`、4445 未监听
4. 已有 `e2e/specs/data-sync-window.ts` 只覆盖壳层 DSW-001~005 与 DSW-MAP（无差异时 Apply 禁用），**没有** rowDiffs / Apply 执行 / Cancel
5. `/Applications/DataZen.app` 为 0.0.8（2026-08-07），不是本分支构建

未启动应用，未执行任何 WDIO / computer-use 交互断言。**不把单元测试结果记为 E2E PASS。**

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F5-SYNC-01 | 同族两端 Compare | mapping 行出现；MATCHED 有差异时显示 `sync.rowDiffs`（+ins / ~upd / −del） | **BLOCKED**。UT-DSW-03 / UT-MAP-04 / UT-CMP-01 覆盖组件层与 IPC |
| E2E-F5-SYNC-02 | 有行差异且勾选 MATCHED | Apply（`data-sync-start`）可点；走 `apply_data_sync`；空 ChangeSet 不得执行 | **BLOCKED**。UT-DSW-03 Apply 调用；UT-APL-01 空集拒绝 |
| E2E-F5-SYNC-03 | Compare 后无 INSERT/UPDATE/DELETE | Apply `data-sync-start-disabled` + `sync.applyUnavailable` | **BLOCKED**。DSW-MAP-001 有此意图但无二进制；UT-DSW-06 无 rows 时不会出现 `data-sync-start` |
| E2E-F5-SYNC-04 | Apply 进行中点 Cancel | `cancel_data_sync(jobId)` 与 execute 同 flag；execute 失败或 `rolledBack` | **BLOCKED**。UT-EXEC-01 覆盖开始前取消；UI Cancel **无 UT** |
| DSW-001~005 / DSW-MAP-001 | 既有 Host spec | 开窗 / 横幅 / 空 Compare / 无差异 Apply 禁用 | **BLOCKED**（无本 worktree webdriver）。不记 PASS |

### 5.1 失败则重现步骤

E2E 未跑到断言，无 FAIL 重现。解除 BLOCKED 的前置：

1. 在本 worktree 执行 `pnpm tauri build --debug --features webdriver`，得到 `target/debug/datazen` 或 macOS `.app` bundle
2. 扩展 `e2e/specs/data-sync-window.ts`（或契约 journey）：同族夹具制造行差异 → `sync.rowDiffs` → Apply；无差异 Apply 禁用；同步中 `data-sync-cancel`
3. `pnpm e2e:skip-build -- --spec e2e/specs/data-sync-window.ts`
4. 或启动本分支 `pnpm tauri:dev` 后用 computer-use 按 E2E-F5-SYNC-01~04 点选

---

## 6. 缺陷列表

无。本切片 P0（同一 `AtomicBool` 取消、MATCHED 行比较返回 `rows`、空 ChangeSet 拒绝、前端 rowDiffs + 有差异才可 Apply）在单元测试与静态审查中与规格一致。

**不记缺陷**（测试缺口 / 后续）：

1. `LiveExecutor` begin/execute/commit/rollback 无 IPC 成功路径；循环中 cancel → rollback 只在 `data_sync::execute` 单测，不经 `commands::sync::exec`。
2. `apply_data_sync_impl` 的 `generate_table_sql` 循环未击中（空 ChangeSet 在 execute 前返回）；成功 Apply 生成 SQL 依赖 `data_sync::sql` 既有单测。
3. `compare_data_sync_impl` 未测 `job_id=Some` / 比较中途 cancel。
4. 前端 Cancel / `rolledBack` 无 UT（DataSyncWindow Lines 仍 ≥80%）。
5. UT-CMP-01 夹具源=目标同行，只断言 `rows` 非空，未分别断言 INSERT/UPDATE/DELETE（算法在 `data_sync::compare::compare_sorted_rows`）。
6. Host E2E spec 未覆盖 rowDiffs / Apply 执行 / Cancel；有应用后仍须补，否则 E2E 表会长期 BLOCKED。
7. 底栏 Cancel 条件含 `syncState === 'comparing'`，但该底栏只在 `compared`（含 syncing/done）时渲染，**比较中** UI 不出现 Cancel。规格原文是「同步中可 Cancel」，按 Apply/`syncing` 理解，不记冲突。

---

## 7. 总评

| 门槛 | 结果 |
|---|---|
| `cargo llvm-cov` / `commands::sync` | **31 passed / 0 failed** |
| jobs+exec+apply 合计 Lines ≥80% | **83.97%**（199/237）；jobs **100%**；apply **83.75%**；exec **75.51%**（非单独门槛） |
| `npx vitest run` 指定 3 files | **26 passed / 0 failed** |
| `mappingView.ts` Lines ≥80% | **95.23%**（20/21） |
| `DataSyncWindow.tsx` Lines 尽量 ≥80% | **85.91%**（122/142）；Compare/Apply/rowDiffs 用例全 PASS |
| include 合计 Lines | **87.11%**（142/163） |
| cancel / compare rows / apply 空集 | 静态 + UT-JOB/EXEC/CMP/APL **有覆盖** |
| 与 P0 规格冲突的产品缺陷 | **无** |
| E2E E2E-F5-SYNC-01~04（及既有 DSW-*） | 全部 **BLOCKED**（无本分支桌面应用、无 webdriver 二进制） |
| 记入缺陷 | **0** |

**总评：PASS**

Data Sync 剩余接线满足：`cancel_data_sync` 与 `execute_data_sync` 共用 `jobs` 中的 `AtomicBool`，取消后 execute 在 begin 前失败；`compare_data_sync` 对 MATCHED 表填 PK 行比较 `rows`；`apply_data_sync` 走 ChangeSet SQL + 专用 execute，空 ChangeSet 拒绝。前端 Compare 后展示 `sync.rowDiffs`，有 INSERT/UPDATE/DELETE 时 Apply 可点。jobs/exec/apply 行覆盖合计 83.97%，mappingView 95.23%，DataSyncWindow 85.91%。E2E 因本机无本分支 DataZen / 无 webdriver debug 二进制全部 BLOCKED，按任务约定**不单独导致 FAIL**。有应用后优先补 Host spec：rowDiffs、Apply 执行、同步中 Cancel，以及（可选）经 LiveExecutor 的成功执行 / 中途回滚。
