# F6 QA 报告：专用 Execute（read_only / 事务 commit·rollback / 取消 / 不走 execute_query）

| 项 | 值 |
|---|---|
| 切片 | F6 |
| 工作目录 | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat` |
| 被测代码 | `src-tauri/src/data_sync/execute.rs`；`src-tauri/src/commands/sync/exec.rs`；IPC `execute_data_sync` |
| 规格 | 专用执行通道；拒绝 Target `read_only`；事务 begin → 参数化执行 → commit，失败 rollback；可取消；**不**经 `execute_query` / `sql_guard`。PRD V1.2 §28.2 / §30.2；方案 `docs/data-synchronization-implementation-plan.zh-CN.md` §5 / §16 / §30.2 / §41.6 |
| 测试角色 | 独立 QA；未修改任何产品代码；未 commit |
| 日期 | 2026-08-13 |
| **总评** | **PASS**（1 个 S3：IPC 未接线 Cancel） |

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
| 桌面应用 | **未运行**（`ps` 无 DataZen；computer-use `list_windows` 仅 Edge）。E2E **BLOCKED** |

工作目录（所有命令均在此执行）：

```text
/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat
```

覆盖率原始摘要：`docs/progress/f6-coverage.txt`。

---

## 2. 本切片测试设计（对照规格）

规格要点（本切片）：

- 执行走专用 IPC `execute_data_sync`，**禁止**把 ChangeSet SQL 交给 `execute_query`（否则 Safe Mode「UPDATE 必须有 WHERE」与 `read_only` 语义错位）
- Target `read_only` → 拒绝 Execute；不得 `begin`
- 空 ChangeSet / 空 statements → 拒绝
- 成功：`begin` → 逐条 `execute(sql, parameters)` → `commit`；`rolled_back=false`
- 语句失败：`rollback`，不得 `commit`；错误含已 applied 条数
- 取消：启动前 → `DataSyncError::Cancelled` 且无 begin；中途 → rollback，`rolled_back=true`
- 执行用绑定参数（`stmt.sql` + `stmt.parameters`），不得执行 Preview 文本

| ID | 层 | 意图 | 已有自动化 |
|---|---|---|---|
| UT-EX-01 | Rust `execute` | 全成功 commit，调用序 begin→execute×N→commit | `commits_all_statements` |
| UT-EX-02 | Rust `execute` | read_only 拒绝且 `calls` 为空 | `read_only_never_begins` |
| UT-EX-03 | Rust `execute` | 空 statements 拒绝 | `empty_set_rejected` |
| UT-EX-04 | Rust `execute` | 第 2 条失败 → rollback，无 commit | `failure_rolls_back` |
| UT-EX-05 | Rust `execute` | 启动前取消 → Cancelled，无 begin | `cancel_before_start` |
| UT-EX-06 | Rust `execute` | 中途取消 → rollback，applied=1 | `cancel_mid_run_rolls_back` |
| UT-IPC-01 | Rust IPC | read_only Target → `execute_data_sync_impl` 报 read-only | `execute_data_sync_rejects_read_only_target` |
| UT-IPC-02 | Rust IPC | 成功 commit / 失败 rollback / 参数化 query_with_params | **缺失**（LiveExecutor 0 hit） |
| UT-IPC-03 | Rust IPC | Cancel 经 IPC | **缺失**（`cancelled: None` 写死） |
| E2E-F6-* | E2E | 真实库 + IPC/UI | **BLOCKED**（无桌面应用；无 E2E spec；UI 未调 `executeDataSync`） |

---

## 3. 单元测试

### 3.1 命令与结果

```bash
cargo test -p datazen --lib data_sync::execute -- --nocapture
```

| 项 | 数量 |
|---|---:|
| 通过 | **6** |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 879 |
| 耗时 | 0.00s（已编译） |

```bash
cargo test -p datazen --lib commands::sync::tests::execute_data_sync -- --nocapture
```

| 项 | 数量 |
|---|---:|
| 通过 | **1**（`execute_data_sync_rejects_read_only_target`） |
| 失败 | **0** |
| 忽略 | 0 |
| 过滤掉的其它 lib 测试 | 884 |
| 耗时 | 0.02s |

失败详情：无。

编译告警（不计入本切片缺陷；**未改代码**）：

- `src-tauri/src/dashboard/execute.rs` unused import `RefreshMode`（与 F5 相同，无关）
- `src-tauri/src/dashboard/create.rs` unused variable `registry`（无关）
- `data_sync/execute.rs:205` `cancel_mid_run_rolls_back` 中被遮蔽的 `let mut exec`（测试夹具，未使用）

### 3.2 已落地单测清单（对照规格）

| ID | 测试名 | 覆盖规格 | 结果 |
|---|---|---|---|
| UT-EX-01 | `commits_all_statements` | 2 条 INSERT；`applied=2`；`rolled_back=false`；calls=`begin, execute:1:INSERT 1, execute:1:INSERT 2, commit` | PASS |
| UT-EX-02 | `read_only_never_begins` | `read_only=true` → Err 含 `read-only`；`calls` 空 | PASS |
| UT-EX-03 | `empty_set_rejected` | `[]` → Err | PASS |
| UT-EX-04 | `failure_rolls_back` | `fail_at=1` → 「execution failed after 1」；有 rollback、无 commit | PASS |
| UT-EX-05 | `cancel_before_start` | flag 预置 true → `DataSyncError::Cancelled`；calls 空 | PASS |
| UT-EX-06 | `cancel_mid_run_rolls_back` | 首条 execute 后翻 flag → `rolled_back=true`，`applied=1`，有 rollback | PASS |
| UT-IPC-01 | `execute_data_sync_rejects_read_only_target` | mock PG + `cfg.read_only=true` + 已连接 → `execute_data_sync_impl` Err 含 `read-only` | PASS |

### 3.3 手工对照（不改代码）

`execute_statements` 控制流与 RecordingExecutor 记录一致：

| 场景 | begin | execute | commit | rollback | 返回 |
|---|---|---|---|---|---|
| 2 条成功 | 1 | 2 | 1 | 0 | `applied=2, rolled_back=false` |
| read_only | 0 | 0 | 0 | 0 | validation「read-only」 |
| 空列表 | 0 | 0 | 0 | 0 | validation「change set is empty」 |
| 第 2 条失败 | 1 | 2（第 2 失败） | 0 | 1 | validation「execution failed after 1」 |
| 启动前取消 | 0 | 0 | 0 | 0 | `Cancelled` |
| 中途取消 | 1 | 1 | 0 | 1 | `applied=1, rolled_back=true` |

执行调用为 `execute(&stmt.sql, &stmt.parameters)`，**不**读 `preview_sql`。

**不走 `execute_query`（静态审查）**：

| 路径 | 结论 |
|---|---|
| `commands/sync/exec.rs` | 无 `execute_query` / `sql_guard` / `execute_driver_command` import 或调用 |
| `LiveExecutor::execute` | `driver.query_with_params(&handle, sql, params)` |
| `execute_query_impl` | 走 `execute_driver_command` + `sql_guard::check_sql`（同步路径不经过） |
| `execute_data_sync` 注册 | `src-tauri/src/lib.rs` 独立 command；前端 `syncCommands.executeDataSync` → `invoke('execute_data_sync')` |
| 前端窗口 | `src/windows/data-sync/` **无** `executeDataSync` 调用（属 F8） |

PG / MySQL 驱动的 `query_with_params` 若该 `handle.id` 已 `begin_transaction`，会走同一条事务连接；Host 用 `ConnectionHandle` 而非把 `TransactionHandle` 传入 execute，与现有驱动约定一致。SQLite 驱动 `query_with_params` **不**查找 open tx（连接池可能换连接）——属驱动缺口，V1 同步目标为同族 MySQL/PG，不记本切片 P0。

---

## 4. 覆盖率（`execute.rs` 门槛；`exec.rs` 附报）

### 4.1 测量方法

1. `cargo llvm-cov --version` → 0.8.7。
2. 不以整 crate 覆盖率为验收标准。
3. 实际采用：

```bash
cargo llvm-cov -p datazen --lib --json --output-path /tmp/datazen-f6-cov/f6-coverage.json \
  -- data_sync::execute commands::sync::tests::execute_data_sync
```

然后按路径 `/data_sync/execute.rs`、`/commands/sync/exec.rs` 过滤。摘要写入 `docs/progress/f6-coverage.txt`（完整 crate JSON 未入库）。

llvm-cov 本次跑 **7 passed / 0 failed**（6 + 1）。

### 4.2 行覆盖率

| 文件 | 行 covered | 行 total | 行覆盖率 | 函数 | 区域 |
|---|---:|---:|---:|---:|---|
| `execute.rs` | 163 | 165 | **98.79%** | 96.43%（27/28） | 97.25%（212/218） |
| `exec.rs` | 29 | 41 | **70.73%** | 27.27%（3/11） | 54.17%（26/48） |

**门槛：`execute.rs` 行覆盖率 ≥ 80% → 满足（98.79%）。**

`exec.rs` 70.73% **低于 80%**，因 IPC 仅测 read_only 早退，`LiveExecutor::{begin,execute,commit,rollback}` 全部 count=0。按任务说明「及若方便」附报，**不单独作为 FAIL 门槛**。建议补：writable mock 上 `execute_data_sync_impl` 成功 commit、注入 `query_with_params` 失败 rollback、断言调用的是 `query_with_params` 而非 `execute_query`。

### 4.3 未覆盖行（不构成 FAIL）

| 位置 | 说明 |
|---|---|
| `execute.rs` 231、233 | 测试内 `FlipOnExecute::commit`（中途取消只 rollback） |
| `exec.rs` 25–33 | `LiveExecutor::begin` |
| `exec.rs` 39–46 | `LiveExecutor::execute` |
| `exec.rs` 48–56 | `LiveExecutor::commit` |
| `exec.rs` 58–66 | `LiveExecutor::rollback` |

测试缺口（建议后续补，不改本切片结论）：

1. IPC 成功路径：writable 连接 + 1 条 parameterized INSERT → `applied=1, rolledBack=false`；mock 上可见 `begin_transaction` / `query_with_params` / `commit`。
2. IPC 失败路径：`query_with_params` 注入失败 → rollback、无 commit。
3. IPC 空 statements / 未知 connection id。
4. IPC Cancel（当前 API 无法测，见 §7）。
5. `commit()` 自身失败是否离开悬挂事务。
6. `rollback()` 失败时 `let _ = rollback` 吞掉错误（仍返回原 execution failed）。
7. `DataSyncError::Cancelled` → `CommandError::Validation`（全部 DataSyncError 都映射成 Validation）。

---

## 5. E2E 用例表

现状：**BLOCKED**。

- 本机无 DataZen 桌面进程；computer-use 窗口列表无应用窗。
- `e2e/specs/` **无** `execute_data_sync` / `executeDataSync`。
- `DataSyncWindow` 未调用 `syncCommands.executeDataSync`（仅 `src/commands/sync.ts` 封装）。
- 现有 `e2e/specs/data-sync-real.ts` 仍面向旧 compare / `sync_tables`，不能当 F6 验收。

| ID | 步骤 | 期望 | 实际 |
|---|---|---|---|
| E2E-F6-01 Target read_only 拒绝 | 同族两库；Target 连接 `read_only=true`；Compare 后 Execute | 允许 Compare；Execute 失败含 read-only；Target 无 DML；无 BEGIN | **BLOCKED**（无应用） |
| E2E-F6-02 成功 commit | Target 可写；ChangeSet 含 INSERT+UPDATE；Execute | `applied=N`，`rolledBack=false`；Recompare 差异减少；库内已提交 | **BLOCKED** |
| E2E-F6-03 语句失败 rollback | 第 2 条故意失败（坏表名 / 约束） | 第 1 条不可见（已 rollback）；无残留半写入；错误含 after N | **BLOCKED** |
| E2E-F6-04 启动前取消 | Execute 前已点取消 / 取消 flag | 无 BEGIN；返回 cancelled；数据不变 | **BLOCKED**（IPC 亦无 cancel 参数） |
| E2E-F6-05 中途取消 rollback | 多语句执行中取消 | rollback；`rolledBack=true`；已执行行不落库 | **BLOCKED** |
| E2E-F6-06 不走 execute_query | spy / 查询历史 / Safe Mode | 不得调用 `execute_query` / `execute_driver_command(query\|execute)`；不得写查询历史；Safe Mode 开着同步仍可跑带 PK WHERE 的 UPDATE | **BLOCKED** |
| E2E-F6-07 执行绑定而非 Preview | spy `query_with_params` | 调用 SQL 为占位符语句 + `parameters`；不得把 `preview_sql` 当脚本执行 | **BLOCKED** |
| E2E-F6-08 空 ChangeSet | 全不选 / 全 UNCHANGED | 拒绝；无事务 | **BLOCKED** |
| E2E-F6-09 未连接 / 无效 id | 错误 targetConnectionId | CommandError；无 begin | **BLOCKED** |
| E2E-F6-10 sql_guard 旁路对照 | 若误走 `execute_query`，无 WHERE 的 UPDATE 会被拦；同步 SQL 带 PK WHERE | 同步通道不调用 `check_sql`；仍拒绝 read_only | **BLOCKED** |
| E2E-F6-11 参数化 INSERT 真库 | PG `$n` / MySQL `?` | 行插入成功；特殊字符不拼接进 SQL | **BLOCKED** |
| E2E-F6-12 前端未接线 | 打开 Data Sync 窗点 Execute | 本切片无 UI；F8 前应无假按钮走旧 `sync_tables` | **BLOCKED**（窗未接新 IPC） |

F8/F9 落地建议：E2E-F6-01/02/03/06/07 进 Host 契约 journey（PG + MySQL）；E2E-F6-06 可用查询历史为空 + mock 调用计数，不必 UI。Cancel 须先给 IPC 接 `cancelled` / `cancel_data_sync`。

---

## 6. 规格缺口审查（F6 范围内）

审查范围：`execute_statements` / `StatementExecutor` / `RecordingExecutor` / `LiveExecutor` / `execute_data_sync` / `execute_data_sync_impl`。Compare、SQL 生成、Diff UI 属其它切片。

| 规格 | 实现 | 结论 |
|---|---|---|
| 专用 IPC，不经 `execute_query` | `execute_data_sync` → `LiveExecutor.query_with_params` | 一致（静态） |
| 不经 `sql_guard` | `exec.rs` 无 `check_sql` | 一致 |
| Target read_only 拒绝 Execute | `is_read_only()` 在 begin 前；IPC 读 `config.read_only` | 一致；IPC 有单测 |
| 事务 commit | `begin` → loop execute → `commit` | 引擎单测一致；**LiveExecutor 未经 IPC 单测** |
| 失败 rollback | execute Err → rollback，无 commit | 引擎一致；IPC 未测 |
| 取消 | `AtomicBool`：启动前 Cancelled；中途 rollback | **引擎一致；IPC 硬编码 `None`（§7）** |
| 参数绑定 | `execute(&stmt.sql, &stmt.parameters)` | 一致；不执行 `preview_sql` |
| 空 ChangeSet 拒绝 | 空 slice → validation | 一致 |
| 仅已勾选行 | IPC 收 `Vec<SqlStatement>`，不看 ChangeSet.selected | 调用方契约（F7 须只传入已选行生成的语句） |

**未记为 P0 冲突**的残留（不构成 FAIL；不改代码）：

1. **IPC Cancel 未接线**（记为 1 个 S3，见 §7）：`execute_data_sync_impl` 第三参恒 `None`；command 无 cancel token / 无 `cancel_data_sync`。引擎层 UT-EX-05/06 已覆盖。
2. **`DataSyncError::Cancelled` → `CommandError::Validation`**：一旦 IPC 接上取消，前端会看到 Validation 而非独立 cancelled。
3. **LiveExecutor 事务路径零覆盖**：read_only 测试在 `begin` 前返回；commit/rollback/query_with_params 无 IPC 自动化。
4. **IPC 信任调用方 statements**：任意前端可提交字面量 SQL + 空 parameters，绕过 sql_guard。F7 应改为服务端从 ChangeSet 生成语句，或校验占位符与 parameters 对齐。
5. **`query_with_params` + `fetch_all` 的 `rows_affected`**：PG/MySQL 把结果行数当 affected；DML 常为 0。`execute_statements` 只按语句成功计数 `applied`，行为可接受。
6. **`commit` 失败不 rollback**：`executor.commit().await?` 失败直接返回；悬挂事务依赖驱动。
7. **失败路径 `let _ = rollback`**：rollback 失败被丢弃。
8. **SQLite `query_with_params` 不走 open transaction map**：与 PG/MySQL 不同；非本切片 V1 目标。
9. **无 UI / 无 Host E2E**：属 F8/F9。

---

## 7. Bug 列表

| ID | 等级 | 标题 | 说明 |
|---|---|---|---|
| F6-BUG-01 | S3 | `execute_data_sync` 无法取消 | `execute_data_sync_impl` 调用 `execute_statements(..., None)`。方案 §41.6 / 本切片 DoD 含 Cancel。引擎 `AtomicBool` 已实现且 UT 通过，但 IPC 无 flag、无独立 cancel command。UI（F8）即使有取消按钮也无法送达。**不阻塞** read_only / 引擎事务结论。 |

无 S1/S2。无「走了 `execute_query`」或「read_only 仍 begin」的证据。

---

## 8. 总评

| 门槛 | 结果 |
|---|---|
| `cargo test -p datazen --lib data_sync::execute` | **6 passed / 0 failed** |
| `cargo test -p datazen --lib commands::sync::tests::execute_data_sync` | **1 passed / 0 failed** |
| `execute.rs` 行覆盖 ≥80% | **98.79%**（163/165） |
| `exec.rs` 行覆盖（附报） | **70.73%**（29/41）；LiveExecutor 未击中 |
| 与 P0 规格冲突的缺陷 | 无（read_only / 引擎事务 / 不走 execute_query 成立） |
| E2E | 全部 **BLOCKED**（无桌面应用；无 UI/E2E 接线） |
| 记入缺陷 | **1**（F6-BUG-01 S3 IPC Cancel） |

**总评：PASS**

`execute_statements` 满足 read_only 拒绝、空集拒绝、成功 commit、失败 rollback、取消（启动前 / 中途）。专用 IPC `execute_data_sync` 静态上不经过 `execute_query` / `sql_guard`，并对 read_only Target 拒绝。LiveExecutor 真事务路径与 IPC Cancel 仍缺自动化；后者记 S3。E2E 待有应用 + F8/F9 再跑。
