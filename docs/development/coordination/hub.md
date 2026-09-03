# Schema Migration Hardening — 协调总览

> Initiative: `feat/schema-diff-hardening`
> 基分支: `feat/schema-diff-hardening`
> 启动时间: 2026-09-03

## 功能总览表

| Track | 范围 | 状态 | 编码 commit | 测试 commit | 合并 commit |
|-------|------|------|-------------|-------------|-------------|
| sd-drv | Driver 渲染完整性 + Capability 一致性 | 未开始 | — | — | — |
| sd-plan | Plan 安全 (type mapper + narrowing) | 未开始 | — | — | — |
| sd-ui | Backfill UI + Requirements 展示 | 未开始 | — | — | — |
| sd-ir | IR/Compare/Dependencies 完善 | 未开始 | — | — | — |

## 波次编排

### Wave 1（并行，无文件冲突）

4 轨同时启动：sd-drv / sd-plan / sd-ui / sd-ir

文件冲突面分析：
- sd-drv: `packages/drivers/*/migration.rs`, `driver-api/schema_migration.rs`, `schema_diff/operations.rs`
- sd-plan: `schema_diff/plan.rs`
- sd-ui: `src/windows/schema-diff/*`, `src/commands/schemaDiff.ts`, `src/locales/*`
- sd-ir: `schema_diff/{ir,compare,dependencies}.rs`

→ 无交叉 ✅

### R 阶段

Wave 1 全部合并后统一跑：
- `cargo test -p datazen --lib`
- `npx vitest run`
- `npx tsc --noEmit`
- E2E schema-diff suite

## 写锁台账

| Track | Agent ID | Worktree | Branch | Phase | Last Heartbeat |
|-------|----------|----------|--------|-------|----------------|
| — | — | — | — | — | — |

## 合并记录

（无）

## 跨轨风险

1. sd-drv 改 `MigrationColumn` 字段（如加 `is_primary_key`）→ sd-ir 的 `to_driver_api()` 桥接需对应更新 → 但 `operations.rs` 已划入 sd-drv
2. sd-plan 的 type mapper 错误处理依赖 sd-drv 的 Renderer 能正确返回 Err → 当前 Renderer 已实现 Err 返回
3. sd-ui 需要后端 Plan 返回 requirements 字段 → 后端已有，仅 TS 类型和 UI 缺失
