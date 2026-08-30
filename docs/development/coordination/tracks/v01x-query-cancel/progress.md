# v01x-query-cancel 进度

## 1. 功能摘要

- 编号：v01x-query-cancel
- 范围：Driver capability、连接信息、QueryExecutionViewModel、取消状态和 QueryPanel 取消入口
- 状态：编码中
- 编码 commit：待定
- 测试 commit：待定

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| QC-E2E-001 | 支持取消的驱动：Running → Cancelling → Cancelled | 留待 R 回归 | 待执行 |
| QC-E2E-002 | 不支持取消的驱动：按钮禁用并显示原因，不调用 cancel | 留待 R 回归 | 待执行 |
| QC-E2E-003 | cancel command 失败：查询不被伪造为 Cancelled | 本机可执行 | 待执行 |

## 3. 测试结果与覆盖率

- 编码轮：待测试代理独立复验。
- 覆盖率：待测试代理测量。

## 4. 设计决策 / 遗留

- capability 未知时按 unknown 处理，不按 supported 处理。
- 当前 PostgreSQL/MySQL cancel 实现的作用范围需要在后续 driver 专项任务中确认。
