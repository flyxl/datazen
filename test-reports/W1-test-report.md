# W1 独立测试报告 — 后端核心/服务层 ID 术语落地

> 测试对象：commit `f0aa9882`（`refactor(ids): W1 backend core/services terminology…`）
> 基线：`main` 尖端 `d4f1aa4f`（即 merge-base，`git diff main` 即全部变更）
> 分支 / 工作区：`feature/db-session-id-rename` @ `.worktrees/db-session-id-rename`，测试期间工作区保持干净
> 测试方式：全新独立会话，只测不修；未修改任何 git 跟踪文件，未执行 `git add/commit`，未动 `ID_RENAME_PROGRESS.md`
> 报告日期：2026-08-24（T1 轮，独立测试 agent）

---

## 一、执行摘要

| 维度 | 结果 |
|------|------|
| T1 独立测试执行 | ✅ **1115 通过 / 2 失败 / 2 忽略**；2 个失败经基线实机复核确属既有环境问题，非 W1 引入 |
| T2 范围合规 | ✅ 26 个 `src-tauri/src` 文件（+482/−278）+ `ID_RENAME_PROGRESS.md`，无越界文件；services 下 `config_id` 残留为 0；**185 个 `#[tauri::command]` 签名与 main 完全一致**；全库旧标识符残留 0 |
| T3 覆盖率 | ✅ llvm-cov 精确行覆盖：connection_manager **86.94%**、db_tools **96.74%**、query_executor **89.40%**，核心模块全部 ≥80%（全 crate TOTAL 77.03%） |
| T4 E2E 用例清单 | ✅ 产出 10 条用例；6 条已在 Rust 单元/IPC 等价层实际执行并通过，1 条与任务书预期不符（既有语义，见 OBS‑1），GUI/MCP 级 3 条未执行并注明原因 |
| T5 缺陷 | ✅ **W1 缺陷 0 个**；另有观察项 2 条 + 既有环境问题 1 条 + 覆盖缺口建议，均不计为 W1 缺陷 |

**结论：通过。**

---

## 二、T1 独立测试执行

### 2.1 执行结果（不采信开发方数字，本机独立运行）

```bash
export CARGO_TARGET_DIR=/Users/wuxiaolong/code/rust-projects/datazen/target
cargo test -p datazen --lib
# test result: FAILED. 1115 passed; 2 failed; 2 ignored; 0 measured (finished in 5.99s)
```

### 2.2 失败用例及既有性复核

| 用例 | 失败现象 |
|------|----------|
| `tests::resolve_log_settings_defaults_without_settings_file` | `lib.rs:1104` panic：向真实用户数据目录写回 `settings.json` 时 `Os { code: 1, PermissionDenied, "Operation not permitted" }` |
| `tests::resolve_log_settings_reads_custom_level_and_path` | `lib.rs:1110` panic：前一个用例持有 `SETTINGS_FILE_LOCK` 时中毒导致的级联 `PoisonError` |

**复核方法（基线实机执行，非推断）**：由于工作区干净且 `src-tauri/src/lib.rs` 不在 W1 变更清单中（两个用例的代码 main 与 HEAD 逐字节一致），另建临时分离 worktree 于 `/tmp/w1-main-baseline` 检出 `d4f1aa4f`（补拷 gitignore 的 codegen 文件 `plugin_init.rs`、`capabilities/default.json` 后），在同一 sandbox 环境运行：

```bash
cargo test -p datazen --lib resolve_log_settings
# 基线结果：1 passed; 2 failed —— 与 HEAD 上完全相同的两个用例、相同的 panic 位置与错误
```

复核后已 `git worktree remove --force` + `git worktree prune` 清理，`git worktree list` 仅剩原有两项。

**结论**：2 个失败均为"sandbox 拒绝写真实用户数据目录"的既有环境问题（第二个是同锁级联），干净基线同样失败，**非 W1 引入**；除此之外 1115 个用例全部通过，无新增失败。注：仓库现有 2 个 stash（`feat/arch-review-all`，2026-08‑20 创建）为历史遗留，本次测试全程未使用/未改动 `git stash`。

### 2.3 开发方声明核对

| 声明 | 核对结果 |
|------|----------|
| lib 测试 1115 过 / 2 失败 | ✅ 与独立执行一致 |
| 新增 3 个不变式测试 | ✅ `evicted_session_auto_reconnects_preserving_db_session_id`、`resolve_session_prefers_db_session_id_over_connection_id`、`resolve_session_falls_back_to_connection_id_and_creates_session` 均存在且通过 |
| IPC 签名零变化 | ✅ 见 T2 |

---

## 三、T2 范围合规审计

### 3.1 变更范围
- `git diff main --stat`：共 27 个文件 = `src-tauri/src` 下 **26 个**（+482/−278，`--numstat` 精确统计）+ `ID_RENAME_PROGRESS.md`（+41）。与声明完全一致。
- `git diff main --name-only | grep -v '^src-tauri/' | grep -v ID_RENAME_PROGRESS` → **空**：前端、驱动包、extensions、e2e 均未被触碰。
- 工作区干净，无未跟踪代码文件（本报告文件除外）。

### 3.2 术语残留
- `grep -rn "config_id" src-tauri/src/services/` → **空**。（store 层 `store.get_connection()` 属持久化层 API，按计划归 W2/W4，不在本项范围且行为未变。）
- 全 `src-tauri/src` 扫描旧标识符 `resolve_config_id | ui_session_map | ActiveConnection | ConnectionError::ConfigNotFound | ConnectionError::ConnectionNotFound | get_connection_config | config_id_map` → **命中文件数 0**。

### 3.3 Tauri 命令签名零变化（脚本比对）
临时 Python 脚本（仅存于 `/tmp`，已不留在仓库）对 main 与 HEAD 的全部 185 个 `.rs` 文件提取每个 `#[tauri::command]` 属性后的完整函数签名（至 `{` 或 `;`）逐一比对：

```
files_compared=185 commands_main=185 commands_head=185
RESULT: ALL #[tauri::command] SIGNATURES IDENTICAL
```

→ 全部 185 个命令（含 `pub async fn` / `pub fn`）名称、参数、返回类型相对 main **零变化**，前端 IPC 契约不受影响。

### 3.4 行为漂移审查
对全部 diff 先过滤已知改名标识符后扫描残余行：剩余变更仅为——注释/文档改写、错误文案字符串、日志文案（`driver_command.rs` 中 history-skip 警告措辞）、局部变量改名（`config_id`→`connection_id` 等）、以及新增 `#[cfg(test)]` 辅助方法（`insert_test_session`/`expire_test_session`）与测试代码。逐 hunk 复核（含 `driver_command.rs`、`ai/context.rs`、`monitor/connections.rs`、`testing/app_state.rs`、`db_tools.rs`、`query_executor.rs`、`commands/error.rs` 等）**未发现任何生产逻辑变化**，与"纯内部重构"声明相符。持久化字段 `QueryHistoryEntry.config_id` 名称保持不变（W4 范围），无契约影响。

---

## 四、T3 覆盖率评估（精确测量）

工具：`cargo llvm-cov 0.8.7` 已预装（llvm-tools-preview 就绪，无需安装）。首次运行因上述 2 个环境失败用例导致 llvm-cov 拒绝生成报告（exit 101）；改用 `cargo llvm-cov -p datazen --lib --summary-only -- --skip resolve_log_settings` 成功生成（被跳过的 2 个用例位于 `lib.rs` 日志设置逻辑，与本节三个目标模块无关）。

| 文件 | 行数 | 未覆盖行 | **行覆盖** | 函数执行率 | 区域覆盖 |
|------|-----:|--------:|----------:|-----------:|---------:|
| `services/connection_manager.rs` | 697 | 91 | **86.94%** | 81.13% | 87.43% |
| `services/db_tools.rs` | 460 | 15 | **96.74%** | 78.79% | 95.38% |
| `services/query_executor.rs` | 547 | 58 | **89.40%** | 86.96% | 87.03% |
| `testing/app_state.rs`（测试基建） | 99 | 0 | 100.00% | 100.00% | 100.00% |
| **TOTAL（全 crate）** | 44741 | 10279 | 77.03% | 70.64% | 77.14% |

✅ 本次改动核心三模块行覆盖全部 ≥80%，达标。

### 4.1 方法级追溯矩阵（补充，connection_manager.rs）

| 方法 | 覆盖它的测试（均已实际执行通过） |
|------|----------------------------------|
| `connect` | connect_registers_session_and_returns_db_session_id；connect_errors_when_connection_config_missing；driver_not_found_when_type_unregistered；shutdown_disconnects_all；IPC：connect_updates_last_connected_and_ping_disconnect |
| `establish_connection` | 上述 connect 系列 + monitor_registry_does_not_touch_ui_session_owner_map + db_tools/IPC 各 resolve 用例 |
| `driver_for_type` | connect 系列；driver_not_found_when_type_unregistered |
| `owner_connection_id` | connect_registers…；evicted_session_auto_reconnects…；resolve_session 两不变式 |
| `get_or_connect_session` | get_or_connect_session_reuses_existing_session；evicted…；IPC：connect_impl 系列、discovers_commands_from_config_id |
| `release` / ref_counts 递减 | ⚠️ 无直接单测（见 4.2 缺口） |
| `disconnect` | disconnect_removes_session；shutdown_disconnects_all；IPC：connect_updates_last_connected_and_ping_disconnect |
| `get_session` | get_session_returns_driver_and_updates_last_used；evicted…；query/schema/ai/data/backup/sync 等大量命令层用例 |
| `resolve_session` | passthrough / reuses_existing_runtime / prefers_db_session_id / falls_back_to_connection_id 四测；IPC：executes_query_command_through_ipc |
| `reconnect`（私有） | evicted_session_auto_reconnects_preserving_db_session_id |
| `get_session_config` | get_session_config_returns_stored_config；IPC：get_connection_info_and_available_drivers |
| `set_active_database` | IPC：schema_commands_with_connected_mock（use_database_impl 链路） |
| `test_connection` | test_connection_uses_driver；IPC：test_connection_success_and_unknown_driver |
| `ping` | ping_returns_true_for_active_session；IPC 同名用例 |
| `cleanup_idle_connections` | evicted_session_auto_reconnects_preserving_db_session_id |
| `shutdown` | shutdown_disconnects_all |
| `maybe_start_tunnel` | 非 SSH 分支全覆盖；SSH 分支 ⚠️ 无直接单测 |
| `start_cleanup_task` | ⚠️ 定时循环无直接单测（依赖 tokio 时间，合理豁免） |

### 4.2 覆盖缺口（改进建议，非缺陷）
1. `release()` 引用计数递减到 0 触发拆除的路径缺直接单测（当前仅 IPC 层间接经过）；
2. SSH 隧道分支（`maybe_start_tunnel` 启停）未覆盖——91 行未覆盖的主要构成之一；
3. `start_cleanup_task` 周期任务未覆盖（可用 tokio::time pause 级测试补齐）。

---

## 五、T4 E2E 视角用例清单与结果

说明：W1 为纯后端内部重构、零 UI 变化，浏览器级 GUI 对本项无增量价值。以下把两种 id 的语义映射到可观察场景；能在 Rust 单元/IPC（`*_impl`）等价层执行的已**实际执行**（下列 ✅ 均出自本次独立运行的通过用例）。

| # | 场景（id 语义映射） | 前置条件 | 步骤 | 预期 | 实际结果状态 |
|---|---------------------|----------|------|------|--------------|
| E2E‑01 | connect(有效 connectionId) → 返回 dbSessionId | Store 已保存连接 `cfg-1`；MockDriver 注册 | `invoke("connect",{connectionId:"cfg-1"})` | 返回运行时会话 id（mock 以 `mock-cfg-1` 为前缀），owner 映射建立 | ✅ 等价层执行通过：`connect_registers_session_and_returns_db_session_id`；IPC：`connect_updates_last_connected_and_ping_disconnect` |
| E2E‑02 | 后续 query IPC 用该 dbSessionId 正常工作 | E2E‑01 完成 | `invoke("execute_driver_command",{connectionId:<dbSessionId>, command:"query", input:{sql:"SELECT 1"}})` | 查询成功返回行列；查询历史记录成功条目 | ✅ 等价层执行通过：`executes_query_command_through_ipc`（dbSessionId 直通 resolve_session） |
| E2E‑03 | use_database / schema 浏览用 dbSessionId 正常 | E2E‑01 完成 | `get_databases(dbSessionId)` → `use_database(dbSessionId,"app")` → `get_tables/get_columns` | 库/表/列元数据正常返回，活动库随 use_database 更新 | ✅ 等价层执行通过：`schema_commands_with_connected_mock` |
| E2E‑04 | connect(未知 connectionId) → 错误文案区分"配置不存在" | 无需连接 | `connect({connectionId:"missing"})` | 错误含 `Connection config 'missing' not found (connectionId refers to a persisted connection configuration…)` | ✅ manager 层文案断言通过：`connect_errors_when_connection_config_missing`；IPC 层报错：`connect_unknown_config_errors`；文案经 `cmd_err` Display 透传（走查确认） |
| E2E‑05 | 对未知 dbSessionId 执行 disconnect / ping | 应用运行 | `disconnect(<不存在id>)`、`ping_connection(<不存在id>)` | 任务书预期：错误提示"运行时会话不存在（是否把 connectionId 当成了 dbSessionId）" | ⚠️ **实际与预期不符（既有语义，main 行为逐字相同，非 W1 引入）**：`ping` 返回 `Ok(false)`，`disconnect` 静默 `Ok(())`。带 DbSessionNotFound 提示文案的错误仅出现在 `get_connection_info`（IPC 报错有测试 `get_connection_info_unknown_connection_errors`）、`set_active_database` 及 owner 映射缺失的 `reconnect` 路径。详见 OBS‑1 |
| E2E‑06 | 会话空闲驱逐后自动重连，dbSessionId 保持不变 | E2E‑01 完成 | 将 `last_used` 回拨超过 idle_timeout（模拟等待 30 分钟）→ `cleanup_idle_connections()` → `get_session(原 dbSessionId)` | ping 先 false（已驱逐）、owner 映射仍在；重建后 handle.id == 原 dbSessionId、ping true、owner 不变 | ✅ 集成层等价执行通过：`evicted_session_auto_reconnects_preserving_db_session_id`（以回拨时间替代真实 30 分钟等待） |
| E2E‑07 | resolve_session 双模·优先 dbSessionId | 构造同一字符串既是活动 dbSessionId 又是持久化 connectionId | `resolve_session("dual")` | 按运行时会话直通返回，不新建会话，owner 保持 `owner-a` | ✅ 执行通过：`resolve_session_prefers_db_session_id_over_connection_id`、`resolve_session_passthrough_db_session_id` |
| E2E‑08 | resolve_session 双模·connectionId 回退建连 | Store 已保存 `cfg-fb` 且无活动会话 | `resolve_session("cfg-fb")` | 新建会话并返回新 dbSessionId，owner 映射记录；已有会话时复用不重复建连 | ✅ 执行通过：`resolve_session_falls_back_to_connection_id_and_creates_session`、`resolve_session_connection_id_reuses_existing_runtime`、IPC：`discovers_commands_from_config_id` |
| E2E‑09 | 监控连接与会话映射隔离 | Monitor registry 初始化 | 建立 monitor 连接后检查 UI 会话映射长度 | 不写入 `session_owner_map`（监控连接独立池） | ✅ 执行通过：`monitor_registry_does_not_touch_ui_session_owner_map` |
| E2E‑10 | 真实 GUI 全链路 + MCP stdio 冒烟（侧栏连接→查询编辑器→断开；`--mcp-stdio` 下 DB tools 用持久化 id） | 完整桌面构建 / MCP 环境 | 手工或 WebdriverIO | 两类 id 流转无感、无回归 | ❌ 未执行。原因：W1 零 UI/契约变化，浏览器级对本项无增量价值，且需 `pnpm tauri build --debug --features webdriver` 完整构建与真实 DB 环境。**建议纳入收尾回归 `pnpm e2e:minimal`**（MCP 冒烟按 docs/architecture/backend/mcp.md 手工执行） |

---

## 六、T5 缺陷记录

### 6.1 W1 引入缺陷
**无。** 独立测试与行为漂移审查均未发现本次重构引入的功能缺陷。

### 6.2 观察项（既有设计/文案层面，不计 W1 缺陷）

| 编号 | 描述 | 重现 | 影响面 | 建议 |
|------|------|------|--------|------|
| OBS‑1 | `disconnect(未知 id)` 静默成功、`ping_connection(未知 id)` 返回 false，二者从不产生 `DbSessionNotFound` 提示；"是否把 connectionId 当成了 dbSessionId" 的引导文案只在 `get_connection_info` / `set_active_database` / owner 缺失的重建路径出现。main 上行为逐字相同（已核对），属既有语义而非 W1 回归 | 对任意不存在 id 调用 `disconnect` / `ping_connection`，均得到成功响应 | 低：误传 id 时部分调用点得不到强反馈，排障成本略增 | 在 W2（IPC 契约切换）时评估让 `disconnect` 对未知会话显式报 `DbSessionNotFound`，或在 API 文档标注"幂等"语义 |
| OBS‑2 | `resolve_session` 双模设计下，既非活动会话又非已存配置的 id（例如应用重启后的过期 dbSessionId）最终以 `ConnectionConfigNotFound("Connection config '<x>' not found…")` 报错——对"我传的是会话 id"的调用方存在轻度误导 | `resolve_session(<既非会话也非配置的 id>)` | 低：错误仍能阻断流程且事实正确，仅归属语义有歧义 | 可在 W4/W5 文档中明确双模解析的错误归属规则；或考虑复合错误信息同时提示两种可能 |

### 6.3 既有问题（非本次引入）
- ENV‑1：`lib.rs` 两个 `resolve_log_settings_*` 用例在受限环境（拒绝写真实用户数据目录）失败，已用基线 worktree 实机复核为既有问题（见 §2.2）。建议后续改造为注入 tempdir 数据目录，消除对真实用户目录的依赖。

---

## 七、结论

**通过。**

- 独立执行：1115 通过 / 2 失败（均为经基线复核的既有环境问题）/ 2 忽略，无新增失败；
- 范围合规：26 个后端文件 + 进度文件，无越界改动；services 下旧术语残留 0；185 个 Tauri 命令签名零变化；无生产行为漂移;
- 覆盖率：核心模块精确行覆盖 connection_manager 86.94% / db_tools 96.74% / query_executor 89.40%，全部 ≥80%；
- W1 缺陷 0 个；观察项 2 条（OBS‑1/2，均既有语义）供编排方在 W2/W4 决策；GUI/MCP 级 E2E 建议 `pnpm e2e:minimal` 收尾回归。
