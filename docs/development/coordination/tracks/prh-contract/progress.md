# Track `prh-contract` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-contract** 章节。依赖 prh-split-mcp 合并后启动。

## 状态

- Phase: PASSED
- 编码 commit: 56b1d37e5
- 测试 commit: fb7a55bbc

## 设计决策

- 外部契约策略文档：`docs/development/external-contract-policy.md`（v0.x vs 近 1.0 deprecation、MCP tool/参数/资源 URI 规则、Driver/AI protocol 引用）
- MCP golden 测试：`src-tauri/src/mcp/contract.rs` + `fixtures/mcp_external_contract.json`，由 `cargo test -p datazen --lib` 覆盖（CI 已有该步骤）
- 破坏性契约检查项：PR 模板 checklist + CONTRIBUTING 外部契约小节

## 自验结果

| 套件 | 编码代理自报 | 测试代理独立实测 | 备注 |
|------|-------------|-----------------|------|
| cargo test -p datazen --lib mcp::contract | 2 passed; 1 ignored | 6 passed; 1 ignored | +4 tester 强化用例 |
| cargo test -p datazen --lib | 1245 passed; 3 ignored | 1249 passed; 3 ignored | +4 新增测试 |
| external-contract-policy.md | pass | pass | 含 Deprecation (v0.x/≥0.9/v1.0) + MCP 规则 |
| CONTRIBUTING + PR 模板 | pass | pass | External contracts 检查项 |

## Tester 强化测试（Phase C）

| 测试函数 | 覆盖点 |
|----------|--------|
| `test_tester_golden_fixture_covers_all_tools_and_input_keys` | 10 个 MCP 工具名称 + 各工具 inputProperties/requiredInputProperties 与 live snapshot 一致 |
| `test_tester_contract_break_detection_tool_rename` | 模拟工具重命名后 golden 比较应失败 |
| `test_tester_contract_break_detection_input_key_removal` | 模拟删除 required 输入键后 golden 比较应失败 |
| `test_tester_resource_uri_contract_snapshot` | 固定 resourceUris + URI template 与 golden 双向一致 |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | — |

## 遗留

—
