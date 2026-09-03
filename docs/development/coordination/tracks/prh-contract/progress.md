# Track `prh-contract` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-contract** 章节。依赖 prh-split-mcp 合并后启动。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 56b1d37e5
- 测试 commit: —

## 设计决策

- 外部契约策略文档：`docs/development/external-contract-policy.md`（v0.x vs 近 1.0 deprecation、MCP tool/参数/资源 URI 规则、Driver/AI protocol 引用）
- MCP golden 测试：`src-tauri/src/mcp/contract.rs` + `fixtures/mcp_external_contract.json`，由 `cargo test -p datazen --lib` 覆盖（CI 已有该步骤）
- 破坏性契约检查项：PR 模板 checklist + CONTRIBUTING 外部契约小节

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | pass | 1245 passed; 3 ignored |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

—
