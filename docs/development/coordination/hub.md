# Tier 1 & Tier 2 核心源码模块化重构 — 协调总览

> **计划**：`docs/development/coordination/refactor-tier1-2-plan.md`  
> **Playbook**：`docs/development/subagent-dev-playbook.md`  
> **集成分支**：`feat/refactor-tier1-2`  
> **状态**：Wave 1 三轨并行派发

## 功能总览表

| Track | 任务 | 状态 | 编码 Commit | 测试 Commit | 合并 Commit |
|-------|------|------|------------|------------|------------|
| **refactor-pg** | 拆解 `postgres/src/postgres.rs` (2,867 行) | 待派发 | — | — | — |
| **refactor-mysql** | 拆解 `mysql/src/mysql.rs` (2,756 行) | 待派发 | — | — | — |
| **refactor-sqldump** | 拆解 `driver-api/src/sql_dump.rs` (1,359 行) | 待派发 | — | — | — |

## 写锁台账

| Track | 写锁代理 | Worktree | Branch | Phase | 最后心跳 |
|---|---|---|---|---|---|
| refactor-pg | — | `.worktrees/datazen-refactor-pg` | feature/refactor-pg | DISPATCHED | — |
| refactor-mysql | — | `.worktrees/datazen-refactor-mysql` | feature/refactor-mysql | DISPATCHED | — |
| refactor-sqldump | — | `.worktrees/datazen-refactor-sqldump` | feature/refactor-sqldump | DISPATCHED | — |

## 波次记录

### Wave 1（refactor-pg + refactor-mysql + refactor-sqldump 三轨并行）
- 开始时间：2026-09-04
- 冲突面：各自修改独立的 crate 源码，零冲突。

## R 阶段清单
- [ ] `cargo test -p datazen-driver-api --lib`
- [ ] `cargo test -p datazen-driver-postgres --lib`
- [ ] `cargo test -p datazen-driver-mysql --lib`
- [ ] `cargo test -p datazen --lib`
- [ ] `pnpm test:unit:drivers`
- [ ] 集成分支 → main PR 说明
