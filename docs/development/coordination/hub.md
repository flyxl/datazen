# Post-Review Hardening — 协调总览

> **PRD**：`docs/todo/post-review-hardening-prd.zh-CN.md`  
> **计划**：`docs/development/coordination/post-review-hardening-plan.md`  
> **Playbook**：`docs/development/subagent-dev-playbook.md`  
> **集成分支**：`feat/post-review-hardening`  
> **状态**：PRD + 计划已就绪，待派发 Wave 1

## 功能总览表

| Track | 任务 | 状态 | 编码 Commit | 测试 Commit | 合并 Commit |
|-------|------|------|------------|------------|------------|
| prh-split-mcp | 拆分 mcp/server.rs | 未开始 | — | — | — |
| prh-split-dcmd | 拆分 driver_command.rs | 未开始 | — | — | — |
| prh-sql-guard | SQL 安全边界 + 单测 | 未开始 | — | — | — |
| prh-ai-egress | AI 出域默认策略 | 未开始 | — | — | — |
| prh-split-lib | 拆分 lib.rs | 未开始（Wave 2） | — | — | — |
| prh-panic-policy | 生产路径 panic 约定 | 未开始（Wave 2） | — | — | — |
| prh-contract | 外部契约 + MCP golden | 未开始（Wave 2，依赖 split-mcp） | — | — | — |
| prh-ci-docs | CI 矩阵 / 窗口边界 / onboarding | 未开始（Wave 2） | — | — | — |

## 写锁台账

| Track | 写锁代理 | Worktree | Branch | Phase | 最后心跳 |
|-------|----------|----------|--------|-------|----------|
| prh-split-mcp | — | — | feature/prh-split-mcp | — | — |
| prh-split-dcmd | — | — | feature/prh-split-dcmd | — | — |
| prh-sql-guard | — | — | feature/prh-sql-guard | — | — |
| prh-ai-egress | — | — | feature/prh-ai-egress | — | — |
| prh-split-lib | — | — | feature/prh-split-lib | — | — |
| prh-panic-policy | — | — | feature/prh-panic-policy | — | — |
| prh-contract | — | — | feature/prh-contract | — | — |
| prh-ci-docs | — | — | feature/prh-ci-docs | — | — |

## 波次记录

### Wave 1（split-mcp + split-dcmd + sql-guard + ai-egress 并行）

- 开始时间：—
- 合并时间：—

### Wave 2（split-lib + panic-policy + contract + ci-docs）

- 开始时间：—
- 前置：Wave 1 全部 MERGED；contract 额外要求 split-mcp 已合并
- 合并时间：—

## 跨轨风险

- `commands/mod.rs`：split-dcmd 合并时注意导出并集
- `prh-contract` 依赖 `prh-split-mcp` 合并后的 mcp 模块布局
- `prh-split-lib` 宜在 Wave 1 合并后启动

## R 阶段清单

- [ ] `cargo test -p datazen-driver-api --lib`
- [ ] `cargo test -p datazen --lib`（basic）
- [ ] basic path drivers lib 测试
- [ ] `cargo test -p datazen-ai-api --lib`
- [ ] `pnpm typecheck` / unit
- [ ] 登记的 E2E 用例执行
- [ ] 集成分支 → main PR 说明与 CHANGELOG（若有用户可见行为变化）
