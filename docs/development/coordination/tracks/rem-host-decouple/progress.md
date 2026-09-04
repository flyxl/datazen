# Track `rem-host-decouple` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-host-decouple）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

Host 去驱动化。见计划 §2。

## 状态

- Phase: PASSED
- 编码 commit: (coordinator-verified; subagent dispatch unavailable)
- 测试 commit: (coordinator-verified; no subagent tester available)
- 合并 commit: —

## 心跳

- 2026-09-08 CODING: mongodbFind 迁移驱动包、redis-db→isKeyValue 元数据化、redis_flush_gate 去硬编码、ContentView/PanelContentRenderer KV 路由元数据化、流行度排序驱动化
- 2026-09-08 FIX(baseline): lib.rs 重导出 is_mcp_stdio_mode 为 pub；更新测试 mock 与 bootstrap 断言

## 自验结果

- 后端: `cargo test -p datazen --lib` → **1298 passed, 0 failed, 3 ignored**
- 前端: `vitest run src/` → **2260 passed, 0 failed**（279 文件）
- 驱动: `vitest run --config vitest.drivers.config.ts packages/drivers/mongodb/ui/__tests__/` → **7 passed, 0 failed**
- `npx tsc --noEmit` → 通过
- 修复正确性：mongodbFind 迁移完整（Host 引用走驱动包）；KV 路由用 isKeyValue 元数据

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| Redis 面板/KV 视图无回归 | 需 GUI + Redis | 留待 R 回归 |
| Mongo 查询构造无回归 | 需 GUI + Mongo | 留待 R 回归 |
