# v01x-pending-changes 进度

## 1. 功能摘要

- 编号：v01x-pending-changes
- 范围：PendingRowChange、Preview change plan、Commit/Rollback、事务和无主键安全门槛
- 状态：编码中
- 编码 commit：待定
- 测试 commit：待定

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| PC-E2E-001 | 编辑单元格后只产生 pending change，不立即 UPDATE | 留待 R 回归 | 待执行 |
| PC-E2E-002 | Delete/Backspace 后只产生删除标记，不立即 DELETE | 留待 R 回归 | 待执行 |
| PC-E2E-003 | Preview SQL 不执行，Commit 成功后刷新数据 | 留待 R 回归 | 待执行 |
| PC-E2E-004 | Commit 失败后 pending changes 保留 | 本机可执行 | 待执行 |

## 3. 测试结果与覆盖率

- 编码轮：待测试代理独立复验。
- 覆盖率：待测试代理测量。

## 4. 设计决策 / 遗留

- 无主键表禁止 UPDATE/DELETE，不能用全列匹配作为默认降级方案。
- Preview 和 Commit 必须使用同一 immutable plan/fingerprint。
