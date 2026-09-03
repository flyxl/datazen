# Schema 基础设施整合 — 协调总览

> PRD: `docs/todo/schema-infra-consolidation-prd.md`  
> 计划: `docs/development/coordination/schema-infra-plan.md`  
> 集成分支: `feat/schema-diff-hardening`

## 功能总览表

| Track | 任务 | 状态 | 编码 Commit | 测试 Commit | 合并 Commit |
|-------|------|------|------------|------------|------------|
| infra-a | fetch_full_column_types 去重 | ✅ 已合并 | 8ca1e2b0 | — | 已 fast-forward |
| infra-b | effective_primary_key 去重 | ✅ 已合并 | 4f837bcb | — | merge commit |
| infra-c | TypeNormalizer | 🔨 编码中 | — | — | — |
| infra-d | TransactionScope | 🔨 编码中 | — | — | — |
| infra-e | Job Cancel 推广 | 🔨 编码中 | — | — | — |

## 波次记录

### Wave 1（A + B 并行）
- 开始时间: 2026-09-03
- 合并时间: 2026-09-03（全量 CI 通过：15 驱动编译 + 1228 Rust 测试）

### Wave 2（C + D + E 并行）
- 开始时间: 2026-09-03
- 合并时间: —

## 跨轨风险
- Track C (`schema_diff/ir.rs`) 与 Track B 有文件冲突 → C 必须在 B 合并后开始

## R 阶段清单
- [ ] `cargo test -p datazen-driver-api`
- [ ] `cargo test -p datazen`
- [ ] `cargo test -p datazen-driver-postgres`
- [ ] `cargo test -p datazen-driver-mysql`
- [ ] `cargo test -p datazen-driver-sqlite`
- [ ] `npx vitest run`
- [ ] `pnpm e2e --suite schema-diff`
