# 代码审查 Remediation — 协调 Hub

> **协调者维护的总览**。各轨细节见 `tracks/<track-id>/progress.md`；bug 见 `tracks/<track-id>/bugs.md`。
> 本文件通过 worktree 软链指向主检出，编辑一处全局可见。

**集成分支：** `feature/code-review-remediation`  
**计划文档：** [code-review-remediation-plan.md](./code-review-remediation-plan.md)  
**最后更新：** 2026-08-29

---

## 功能总览

| 轨 ID | 优先级 | 功能 | 状态 | 编码 commit | 测试 commit | 备注 |
|-------|--------|------|------|-------------|-------------|------|
| cr-p0-mcp | P0 | MCP 认证 + Resource 策略 | 已完成 | 808ee458 | 1e672b81 | 已合并 |
| cr-p0-ext-install | P0 | 扩展安装路径门闸 | 已完成 | 41c3cf34 | 89c551e7 | 已合并 |
| cr-p1-session-id | P1 | 退役 resolve_session 双模 | 测试中·修复轮 | 390d4efc | 1a10bcfb | fix 594e908b 待复验 |
| cr-p1-workflow-ui | P1 | Workflow UI 统一 | 已完成 | 5791d527 | — | 已合并 |
| cr-p1-split-ai | P1 | 拆分 ai.rs | 已完成 | f06c0c9f | — | 已合并 |
| cr-p2-sync-pairing | P2 | sync pairing 单一来源 | 未开始 | — | — | W3 待启动 |
| cr-p2-schema-cmd | P2 | Schema Driver Command 收敛 | 未开始 | — | — | W3 |
| cr-p2-navigator-split | P2 | Navigator 拆分 | 未开始 | — | — | W3 |
| cr-p3-dedup-dialogs | P3 | 对话框/toErrorMessage 去重 | 未开始 | — | — | W4 |
| cr-p3-secrets-hardening | P3 | 密钥文件 + 日志脱敏 | 未开始 | — | — | W4 |

---

## 波次进度

| 波次 | 轨道 | 状态 | 合并到集成分支 |
|------|------|------|----------------|
| W1 | cr-p0-mcp, cr-p0-ext-install | 已完成 | c67ea693 |
| W2 | cr-p1-* (×3) | 进行中 | workflow-ui + split-ai 已合并；session-id 待 fix 复验 |
| W3 | cr-p2-* (×3) | 未开始 | — |
| W4 | cr-p3-* (×2) | 未开始 | — |
| R | 全量回归 | 待 W4 | — |

---

## 合并记录

| 日期 | 源分支 | 目标 | 备注 |
|------|--------|------|------|
| 2026-08-29 | feature/cr-p0-mcp, cr-p0-ext-install | feature/code-review-remediation | W1 |
| 2026-08-29 | feature/cr-p1-workflow-ui, cr-p1-split-ai | feature/code-review-remediation | W2 部分 |

---

## R 阶段检查清单

- [ ] `cargo test -p datazen --lib`
- [ ] `npx vitest run`
- [ ] `pnpm exec tsc --noEmit`
- [ ] E2E 登记项（MCP stdio、plugins J1-001-R 等）
