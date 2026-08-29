# 代码审查 Remediation — 协调 Hub

> **协调者维护的总览**。各轨细节见 `tracks/<track-id>/progress.md`；bug 见 `tracks/<track-id>/bugs.md`。

**集成分支：** `feature/code-review-remediation`  
**计划文档：** [code-review-remediation-plan.md](./code-review-remediation-plan.md)  
**最后更新：** 2026-08-29

---

## 功能总览

| 轨 ID | 优先级 | 功能 | 状态 | 编码 commit | 备注 |
|-------|--------|------|------|-------------|------|
| cr-p0-mcp | P0 | MCP 认证 + Resource 策略 | 已完成 | 808ee458 | |
| cr-p0-ext-install | P0 | 扩展安装路径门闸 | 已完成 | 41c3cf34 | |
| cr-p1-session-id | P1 | 退役 resolve_session 双模 | 已完成 | 594e908b | |
| cr-p1-workflow-ui | P1 | Workflow UI 统一 | 已完成 | 5791d527 | -1018 行 WorkflowPanel |
| cr-p1-split-ai | P1 | 拆分 ai.rs | 已完成 | f06c0c9f | |
| cr-p2-sync-pairing | P2 | sync pairing 单一来源 | 已完成 | 01e05731 | |
| cr-p2-schema-cmd | P2 | Schema Driver Command 收敛 | 已完成 | f67e9121 | |
| cr-p2-navigator-split | P2 | Navigator 拆分 | 已完成 | 9127fee2 | 2888→660 行 |
| cr-p3-dedup-dialogs | P3 | 对话框/toErrorMessage 去重 | 已完成 | c8f25a6a | |
| cr-p3-secrets-hardening | P3 | 密钥文件 + 日志脱敏 | 已完成 | c1ffc385 | history 明文风险已文档化 |

---

## 波次进度

| 波次 | 状态 | 备注 |
|------|------|------|
| W1 P0 | 已完成 | |
| W2 P1 | 已完成 | |
| W3 P2 | 已完成 | |
| W4 P3 | 已完成 | |
| R 全量回归 | 进行中 | follow-up 修复代理 2026-08-29 |

---

## R 阶段检查清单

- [ ] `cargo test -p datazen --lib`
- [ ] `npx vitest run`
- [ ] `pnpm exec tsc --noEmit`
- [x] E2E：MCP stdio 手工
- [x] E2E：plugins J1-001-R（dialog injection + browse folder）

## 遗留（非阻塞）

- Windows `.key` / `mcp.token` ACL（Unix `chmod 600` 已实现；Windows DACL 待跟进）
- 非 en i18n key sync
