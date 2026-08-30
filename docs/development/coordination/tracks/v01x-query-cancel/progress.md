# v01x-query-cancel 进度

## 1. 功能摘要

- 编号：v01x-query-cancel
- 范围：Driver capability、连接信息、QueryExecutionViewModel、取消状态和 QueryPanel 取消入口
- 状态：修复验证完成（BUG-001/BUG-002 已关闭；保留直接绕过 capability 的 driver 风险说明）
- 编码 commit：`6bbbf2e8`（含前置 checkpoint `1c10a297`）
- 测试 commit：`e20ed2b5`
- 第二轮独立测试 commit：本轨最新提交（f2）
- 修复 commit：本次修复提交（见分支最新 commit）

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

## 4. 设计决策 / 遗留

- capability 未知时前端按 unknown 处理，不按 supported 处理；Rust `cancel_query_impl` 现对 unknown fail-closed，在调用 driver 前返回结构化拒绝，见 `v01x-query-cancel-BUG-001`。
- capability 由 `DatabaseDriverFactory` 统一提供，Registry 按 driver type 惰性加载并以 camelCase 暴露；旧/测试 driver 的缺失字段序列化为 null，前端保持 unknown。
- QueryExecutionViewModel 用 reducer 区分 execution phase 与 cancel capability；cancel IPC 成功只进入 `cancel_requested`，只有 query stream/promise 的实际终态才进入 succeeded/failed/cancelled/outcome_unknown。
- QueryPanel 和 panelStore 只对 supported 发起取消；unsupported/unknown 的 Cancel 控件禁用并解释原因，关闭面板同样不调用取消 stub。
- PostgreSQL 当前实现仍会取消同一数据库内、除取消连接自身外的所有 active backend；MySQL 更宽，会扫描实例级 `information_schema.processlist` 并对所有非 Sleep 且有 SQL 的线程执行 `KILL QUERY`，不限定数据库。由于现有 `dbSessionId`/Driver Command API 无法精确定位当前 query，本修复已将两组实现及其兼容 wrapper 的 `supports_cancel_query` 降为 unsupported；底层实现保留给后续 driver 专项协议工作，直接绕过 Host capability 门控的调用仍有作用域风险。
- QC-E2E-001 至 QC-E2E-003 的 Host 契约已由本机单测覆盖；真实桌面/数据库 E2E 因当前环境没有可用的 computer-use MCP 与稳定数据库 fixture，留待 R 回归。
