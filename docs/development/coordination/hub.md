# 代码审查 Remediation — 协调 Hub

> **协调者维护的总览**。各轨细节见 `tracks/<track-id>/progress.md`；bug 见 `tracks/<track-id>/bugs.md`。
> 本文件通过 worktree 软链指向主检出，编辑一处全局可见。

**集成分支：** `feature/code-review-remediation`  
**计划文档：** [code-review-remediation-plan.md](./code-review-remediation-plan.md)  
**最后更新：** 2026-08-29（协调者初始化）

---

## 功能总览

| 轨 ID | 优先级 | 功能 | 状态 | 编码 commit | 测试 commit | 备注 |
|-------|--------|------|------|-------------|-------------|------|
| cr-p0-mcp | P0 | MCP 认证 + Resource 策略 | 编码完成 | 808ee458 | — | 待测试代理 |
| cr-p0-ext-install | P0 | 扩展安装路径门闸 | 编码完成 | 41c3cf34 | — | 待测试代理 |
| cr-p1-session-id | P1 | 退役 resolve_session 双模 | 未开始 | — | — | W2 |
| cr-p1-workflow-ui | P1 | Workflow UI 统一 | 未开始 | — | — | W2 |
| cr-p1-split-ai | P1 | 拆分 ai.rs | 未开始 | — | — | W2 |
| cr-p2-sync-pairing | P2 | sync pairing 单一来源 | 未开始 | — | — | W3 |
| cr-p2-schema-cmd | P2 | Schema Driver Command 收敛 | 未开始 | — | — | W3 |
| cr-p2-navigator-split | P2 | Navigator 拆分 | 未开始 | — | — | W3 |
| cr-p3-dedup-dialogs | P3 | 对话框/toErrorMessage 去重 | 未开始 | — | — | W4 |
| cr-p3-secrets-hardening | P3 | 密钥文件 + 日志脱敏 | 未开始 | — | — | W4 |

**状态枚举：** `未开始` → `编码中` → `编码完成` → `测试中` → `已完成`（修复轮：`测试中·修复轮`）

---

## 波次进度

| 波次 | 轨道 | 状态 | 合并到集成分支 |
|------|------|------|----------------|
| W1 | cr-p0-mcp, cr-p0-ext-install | 编码中 | — |
| W2 | cr-p1-* (×3) | 待 W1 合并 | — |
| W3 | cr-p2-* (×3) | 待 W2 合并 | — |
| W4 | cr-p3-* (×2) | 待 W3 合并 | — |
| R | 全量回归 | 待 W4 合并 | — |

---

## 跨轨风险登记（非 bug，协调者维护）

| ID | 描述 | 影响轨 | 状态 |
|----|------|--------|------|
| RISK-001 | `lib.rs` invoke_handler 多轨同时增删 | 全部 | 开放 |
| RISK-002 | MCP 认证可能影响现有 MCP client 配置 | cr-p0-mcp | 开放 |

---

## 合并记录

| 日期 | 源分支 | 目标 | 协调者备注 |
|------|--------|------|-----------|
| — | — | — | — |

---

## R 阶段检查清单（全部完成后）

- [ ] `cargo test -p datazen --lib`
- [ ] `npx vitest run`
- [ ] `pnpm exec tsc --noEmit`
- [ ] E2E 登记项执行（见各轨 progress）
- [ ] `docs/architecture/backend/mcp.md` 等文档与实现一致
