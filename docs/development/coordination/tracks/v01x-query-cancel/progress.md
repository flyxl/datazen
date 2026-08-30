# v01x-query-cancel 进度

## 1. 功能摘要

- 编号：v01x-query-cancel
- 范围：Driver capability、连接信息、QueryExecutionViewModel、取消状态和 QueryPanel 取消入口
- 状态：编码完成，待正式提交
- 编码 commit：待定
- 测试 commit：待定

### 审计里程碑（checkpoint 1c10a297）

- 已确认已有预备实现覆盖了 capability DTO、连接信息字段、取消请求态和基础 QueryPanel 接线。
- 已发现并待修复：`get_capabilities` 未主动按 driver type 惰性加载；`markConnected` 直连路径不拉取 capability；VM 会从任意取消相关错误文本推导 `cancelled`；缺少固定的取消动作状态/状态转换 reducer。
- 已确认 SQLite 及未声明 capability 的旧/测试 driver 保持 unknown 或 unsupported，不得按支持取消处理。

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| QC-E2E-001 | 支持取消的驱动：Running → Cancelling → Cancelled | 留待 R 回归 | 待执行 |
| QC-E2E-002 | 不支持取消的驱动：按钮禁用并显示原因，不调用 cancel | 留待 R 回归 | 待执行 |
| QC-E2E-003 | cancel command 失败：查询不被伪造为 Cancelled | 本机可执行 | 待执行 |

## 3. 测试结果与覆盖率

- `npx vitest run src/lib/__tests__/queryExecutionViewModel.test.ts src/stores/__tests__/queryExecActions.test.ts src/stores/__tests__/panelStore.test.ts src/stores/__tests__/activeConnectionStore.test.ts src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx`：5 files / 80 tests passed。
- `npx tsc --noEmit`：通过。
- `CARGO_TARGET_DIR=/private/tmp/datazen-target-v01x-query-cancel cargo test -p datazen --lib`：1182 passed / 0 failed / 2 ignored。
- `CARGO_TARGET_DIR=/private/tmp/datazen-target-v01x-query-cancel cargo fmt --check --manifest-path src-tauri/Cargo.toml`：通过；仅为本地验证格式化了忽略的 codegen 文件 `src-tauri/src/driver_init.rs`，未纳入提交。
- 覆盖率：待测试代理测量。

## 4. 设计决策 / 遗留

- capability 未知时按 unknown 处理，不按 supported 处理。
- capability 由 `DatabaseDriverFactory` 统一提供，Registry 按 driver type 惰性加载并以 camelCase 暴露；旧/测试 driver 的缺失字段序列化为 null，前端保持 unknown。
- QueryExecutionViewModel 用 reducer 区分 execution phase 与 cancel capability；cancel IPC 成功只进入 `cancel_requested`，只有 query stream/promise 的实际终态才进入 succeeded/failed/cancelled/outcome_unknown。
- QueryPanel 和 panelStore 只对 supported 发起取消；unsupported/unknown 的 Cancel 控件禁用并解释原因，关闭面板同样不调用取消 stub。
- MySQL/PostgreSQL 当前实现会取消同一数据库内除自身会话外的所有活跃查询，而非精确单 query；其兼容协议 wrapper 继承同样作用范围，已留给后续 driver 专项任务，不在本轨扩大。
- E2E 三条用例留待测试代理/协调者回归。
