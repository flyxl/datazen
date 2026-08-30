# v01x-query-cancel 进度

## 1. 功能摘要

- 编号：v01x-query-cancel
- 范围：精确 QueryExecutionId 协议、Host ownership、PostgreSQL/MySQL target cancel、QueryExecutionViewModel、取消状态和 QueryPanel 取消入口
- 状态：协议重构实现与验证完成（BUG-001/BUG-002 已关闭）
- 编码 commit：`1c531d84`（driver-api/Host 最小闭环）、`5f00b563`（PG/MySQL + frontend）、`19dcaaf9`（target/control pool 生命周期竞态修正）、`0a23b82b`（duplicate owner 保持不变）
- 历史修复 commit：`6bbbf2e8`、`e20ed2b5`、`5d23c50d` 保留为本轨前置记录

### 审计里程碑（checkpoint 1c10a297）

- 已确认已有预备实现覆盖了 capability DTO、连接信息字段、取消请求态和基础 QueryPanel 接线。
- 已发现并待修复：`get_capabilities` 未主动按 driver type 惰性加载；`markConnected` 直连路径不拉取 capability；VM 会从任意取消相关错误文本推导 `cancelled`；缺少固定的取消动作状态/状态转换 reducer。
- 已确认 SQLite 及未声明 capability 的旧/测试 driver 保持 unknown 或 unsupported，不得按支持取消处理。

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| QC-E2E-001 | 支持取消的驱动：Running → Cancelling → Cancelled | 留待 R 回归 | 留待真实数据库/桌面 E2E |
| QC-E2E-002 | 不支持取消的驱动：按钮禁用并显示原因，不调用 cancel | 留待 R 回归 | Host UI 单测已覆盖；真实桌面 E2E 待执行 |
| QC-E2E-003 | cancel command 失败：查询不被伪造为 Cancelled | 本机可执行 | Host Rust/Store/UI 契约已覆盖；真实桌面 E2E 待执行 |

## 3. 测试结果与覆盖率

以下命令均在 `/Users/wuxiaolong/code/rust-projects/datazen-v01x-query-cancel` 执行；除明确标注的环境限制外未修改业务源码：

- `npx vitest run src/lib/__tests__/queryExecutionViewModel.test.ts src/stores/__tests__/queryExecActions.test.ts src/stores/__tests__/panelStore.test.ts src/stores/__tests__/activeConnectionStore.test.ts src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx`：5 files / 80 passed / 0 failed。
- `npx vitest run`：262 files / 2120 passed / 0 failed。
- `npx tsc --noEmit`：通过。
- `npx vitest run src/lib/__tests__/queryStream.test.ts src/windows/connection/__tests__/QueryPanelHistory.test.tsx src/stores/__tests__/queryExecActions.test.ts`：3 files / 24 passed / 0 failed；覆盖 query history、流式和多语句执行回归。
- `CARGO_TARGET_DIR=/private/tmp/datazen-test-v01x-query-cancel cargo test -p datazen --lib`：首次在沙箱中为 1136 passed / 46 failed / 2 ignored；46 项均为 wiremock 绑定本机端口的 `EPERM` 环境失败。允许本机临时端口后以同一命令重跑：1182 passed / 0 failed / 2 ignored。
- `cargo fmt --all -- --check`：通过。
- `git diff --check 6bbbf2e8^ 6bbbf2e8`：通过。
- `npx vitest run --coverage --coverage.reportsDirectory /private/tmp/datazen-v01x-query-cancel-coverage-focused --coverage.include src/lib/queryExecutionViewModel.ts --coverage.include src/stores/queryExecActions.ts --coverage.include src/stores/panelStore.ts --coverage.include src/stores/activeConnectionStore.ts --coverage.include src/windows/connection/QueryPanel.tsx src/lib/__tests__/queryExecutionViewModel.test.ts src/stores/__tests__/queryExecActions.test.ts src/stores/__tests__/panelStore.test.ts src/stores/__tests__/activeConnectionStore.test.ts src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx`：5 files / 80 passed；聚焦变更文件 Statements 62.76%、Branches 46.40%、Functions 58.30%、Lines 63.01%。文件级 Lines：`queryExecutionViewModel.ts` 100%、`activeConnectionStore.ts` 91.52%、`panelStore.ts` 82.22%、`queryExecActions.ts` 98.03%、`QueryPanel.tsx` 42.58%（后者为大型组件整体统计，取消分支已有单测）。
- 覆盖率默认目录阈值命令因 worktree 不可写而先遇到 `coverage/` 创建 `EPERM`；改用 `/private/tmp` 报告目录后测试通过，但仅跑定向文件会触发仓库全量目录阈值，故采用上述 `--coverage.include` 的聚焦结果。
- 未新增 Host 中的驱动专属测试；未提交 hub、规格文档、SVG 或 codegen 文件。
- 本修复轮 `npx vitest run src/lib/__tests__/queryExecutionViewModel.test.ts src/stores/__tests__/queryExecActions.test.ts src/stores/__tests__/panelStore.test.ts src/stores/__tests__/activeConnectionStore.test.ts src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx`：5 files / 80 passed / 0 failed。
- 本修复轮 `npx tsc --noEmit`：通过。
- 本修复轮 `CARGO_TARGET_DIR=/private/tmp/datazen-repair-v01x-query-cancel cargo test -p datazen --lib`：1183 passed / 0 failed / 2 ignored；沙箱首次运行的 46 个 wiremock 端口 `EPERM` 在允许本机临时端口后消失。
- 本修复轮 `CARGO_TARGET_DIR=/private/tmp/datazen-repair-v01x-query-cancel-mysql cargo test -p datazen-driver-mysql --lib`：68 passed / 0 failed。
- 本修复轮 `CARGO_TARGET_DIR=/private/tmp/datazen-repair-v01x-query-cancel-postgres cargo test -p datazen-driver-postgres --lib`：82 passed / 0 failed。
- 本修复轮 `cargo fmt --all -- --check`：通过；`git diff --check`：通过。

### 第二轮独立回归（f2，2026-08-30）

- 定向 Vitest：`npx vitest run src/lib/__tests__/queryExecutionViewModel.test.ts src/lib/__tests__/queryStream.test.ts src/stores/__tests__/queryExecActions.test.ts src/stores/__tests__/panelStore.test.ts src/stores/__tests__/activeConnectionStore.test.ts src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx src/windows/connection/__tests__/QueryPanelHistory.test.tsx`：7 files / 91 passed / 0 failed；覆盖 BUG-001/002 相关 UI/Store 状态、history、stream、多语句和 QueryPanel 取消入口。
- 全量前端：`npx vitest run`：262 files / 2120 passed / 0 failed。
- 类型检查：`npx tsc --noEmit`：通过。
- Rust Host：`CARGO_TARGET_DIR=/private/tmp/datazen-retest-v01x-query-cancel cargo test -p datazen --lib`：提权重跑 1183 passed / 0 failed / 2 ignored；沙箱首次运行 1137 passed / 46 failed / 2 ignored，46 项均为 wiremock 绑定本机临时端口的 `Operation not permitted`，不属于测试断言失败。
- IPC 精确子集：`cargo test -p datazen --lib commands::query::tests::cancel_query`：3 passed / 0 failed；unknown capability 路径确认返回结构化拒绝且 MockDriver `cancel_query` 调用次数为 0，supported/unsupported 路径保持预期。
- 受影响驱动：`CARGO_TARGET_DIR=/private/tmp/datazen-retest-v01x-query-cancel-mysql cargo test -p datazen-driver-mysql --lib`：68 passed / 0 failed；`CARGO_TARGET_DIR=/private/tmp/datazen-retest-v01x-query-cancel-postgres cargo test -p datazen-driver-postgres --lib`：82 passed / 0 failed。静态审查与驱动 capability 回归确认 MySQL/MariaDB/Doris/StarRocks/Manticore/Oracle 兼容 wrapper（6）及 PostgreSQL/QuestDB/Cloudberry wrapper（3）均不宣称 unsafe cancel supported。
- 聚焦覆盖率（上述 7 个 Vitest 文件，include 变更相关源码）：Statements 64.56%（523/810），Branches 53.04%（270/509），Functions 61.81%（170/275），Lines 64.73%（435/672）；`queryExecutionViewModel.ts` Lines 100%、`queryStream.ts` Lines 100%、`queryExecActions.ts` Lines 98.03%、`QueryPanel.tsx` 整文件 Lines 44.20%（取消相关断言全通过）。
- 质量检查：`cargo fmt --all -- --check`、`git diff --check ecfa2bcf^ ecfa2bcf`：均通过。
- 本轮只更新本轨 `progress.md`/`bugs.md`；未修改业务源代码，未提交 hub、规格文档、SVG 或 codegen；真实桌面/数据库 E2E 因当前未提供 computer-use MCP 与稳定数据库 fixture，仍留待 R 回归。

### 精确 execution-handle 协议交付（2026-08-31）

- 协议字段：`QueryExecutionId` 是只暴露 opaque string 的新类型；Host 每次 `query_stream` 生成 UUID，driver-api 提供 `prepare_query_execution`、`query_stream_with_execution`、`cancel_query_with_execution`、`cleanup_query_execution`，并由 factory 的 `supports_query_execution_cancel` 明确声明。`PROTOCOL_VERSION` 已从 2 提升到 3。
- 事件/IPC：流开始先发 `QueryStreamEvent::ExecutionStarted { executionId }`；`cancel_query` 接收 `dbSessionId + executionId`。Host registry 只保存 execution 到 session 的归属，拒绝 unknown/stale/wrong-session/duplicate；旧 driver 默认只复用 stream，不会被 Host 调用旧 `cancel_query(handle)`。
- 竞态：driver 在 `prepare` 阶段登记 pending entry；target 未就绪时 cancel 只记录 `cancel_requested`，专用连接拿到 PID/thread 后再次检查并放弃用户 SQL。精确 cancel 的控制 SQL 期间保持 entry 生命周期锁，terminal cleanup 等待控制调用完成，避免 pooled PID/thread 复用造成 late cancel。
- PostgreSQL：普通非事务 stream 从专用 `PoolConnection` 获取 `pg_backend_pid()`，使用同一连接执行全部 statements；精确取消仅由独立 control pool 执行 `SELECT pg_cancel_backend($1)`。native PostgreSQL capability 为 true；QuestDB/Cloudberry wrapper 不宣称。事务连接明确返回 unsupported。
- MySQL：普通非事务 stream 从专用 `PoolConnection` 获取 `SELECT CONNECTION_ID()`，使用同一连接执行全部 statements；native MySQL 精确取消仅由独立 control pool 执行 `KILL QUERY <threadId>`，不扫描 processlist。MariaDB/Doris/StarRocks/Manticore/ob_oracle wrapper 不宣称。事务连接明确返回 unsupported。
- Frontend：stream event、`queryExecActions`、panel close/cancel 均携带并在终态清理 `executionId`；无 executionId 不调用 cancel。带 params 的 `runBoundQuery` 复用 `executeQueryStream`，保留 history/schema refresh/多语句行为，无 driver type 分支。
- 最终验证：`pnpm typecheck` 通过；全量 Vitest `262 files / 2124 passed / 0 failed`；定向 query-cancel Vitest `6 files / 90 passed / 0 failed`；`CARGO_TARGET_DIR=/private/tmp/datazen-protocol-query-cancel cargo test -p datazen --lib` 为 `1186 passed / 0 failed / 2 ignored`；driver-api `99 passed`；MySQL `72` 个 crate 单元测试及其集成测试、PostgreSQL `86` 个 crate 单元测试及其集成测试全部通过；`cargo fmt --all -- --check` 与 `git diff --check` 通过。
- 测试落点：精确 PID/thread、pending/stale/wrong-session、并发 execution 隔离和事务限制测试位于各自 PG/MySQL driver crate；Host 仅覆盖协议/归属/事件序列化，不编码驱动方言测试。

## 4. 设计决策 / 遗留

- capability 未知时前端按 unknown 处理，不按 supported 处理；Rust `cancel_query_impl` 现对 unknown fail-closed，在调用 driver 前返回结构化拒绝，见 `v01x-query-cancel-BUG-001`。
- capability 由 `DatabaseDriverFactory` 统一提供，Registry 按 driver type 惰性加载并以 camelCase 暴露；旧/测试 driver 的缺失字段序列化为 null，前端保持 unknown。
- QueryExecutionViewModel 用 reducer 区分 execution phase 与 cancel capability；cancel IPC 成功只进入 `cancel_requested`，只有 query stream/promise 的实际终态才进入 succeeded/failed/cancelled/outcome_unknown。
- QueryPanel 和 panelStore 只对 supported 发起取消；unsupported/unknown 的 Cancel 控件禁用并解释原因，关闭面板同样不调用取消 stub。
- PostgreSQL/MySQL 已改为精确 execution-handle 取消，不再保留宽作用域实现作为新协议 fallback。只有 native PostgreSQL/MySQL 同时实现并由 factory advertise 新协议；兼容 wrapper 只有在无法证明同等精确语义时保持 false。事务连接不能安全并发取消，因此明确返回 unsupported，不伪装成普通 query 的可取消能力。
- QC-E2E-001 至 QC-E2E-003 的 Host 契约已由本机单测覆盖；真实桌面/数据库 E2E 因当前环境没有可用的 computer-use MCP 与稳定数据库 fixture，留待 R 回归。
