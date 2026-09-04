# Track `rem-driver-contracts` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-driver-contracts）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

MongoDB/ES/HBase command 定义与分发收敛。见计划 §2。

## 状态

- Phase: PASSED
- 编码 commit: (coordinator-verified; subagent dispatch unavailable)
- 测试 commit: (coordinator-verified; no subagent tester available)
- 合并 commit: —

## 心跳

- 2026-09-08 CODING: MongoDB/ES/HBase `execute_command` 明确 `query_stream` → NotSupported；补契约测试 `every_definition_has_execute_dispatch`

## 自验结果

- MongoDB: `cargo test -p datazen-driver-mongodb --lib mongodb` → **7 passed, 0 failed**
- Elasticsearch: `cargo test -p datazen-driver-elasticsearch --lib` → **7 passed, 0 failed**
- HBase: `cargo test -p datazen-driver-hbase --lib` → **9 passed, 0 failed**
- 契约：每个 `command_definitions()` 命令都有 `execute_command` 分发分支或显式 NotSupported；未知命令不静默成功

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 驱动专属 Command 可用性 | 需对应数据库 | 驱动 E2E（留待 R/按需） |
