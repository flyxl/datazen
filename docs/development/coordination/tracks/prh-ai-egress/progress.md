# Track `prh-ai-egress` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-ai-egress** 章节。

## 状态

- Phase: PASSED
- 编码 commit: d44fff88c
- 测试 commit: aa967c108

## 设计决策

- 新增 `AppSettings.ai_strict_egress`（默认 `true`）：严格模式下 JSON 结果行/payload 键被剥离；凭据始终脱敏。
- Rust 侧统一 `redact_for_egress(value, strict_egress)`，在 `ai_chat` / `ai_generate_sql` 等 command 边界及 tool loop 结果回注时应用。
- 前端设置页提供 Strict AI egress 开关；关闭前弹出「数据离开本机」确认。
- 聊天 / NL2SQL / Workflow AI 面板在 relaxed 模式或附加 @ 文件上下文时展示 egress 提示条（`AiEgressNotice`）。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | 1237 passed / 0 failed | 含 ai::safety 7 项 + commands::ai 215 项 |
| npx tsc --noEmit | pass | — |

## 测试代理复验（2026-09-03）

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | 1237 passed / 0 failed / 2 ignored | 沙箱内 2 项 hidden-file 测试 PermissionDenied；无沙箱重跑全绿 |
| cargo test -p datazen --lib ai:: | 215 passed / 0 failed | 含 ai::safety 7 项 |
| npx tsc --noEmit | pass | — |
| i18n en + zh-CN | pass | 7 个新键 en/zh-CN 齐全；其它语言留待发布前 i18n-sync |
| 验收标准审查 | pass | 见下方 checklist |

### 验收 checklist

- [x] 默认不自动附带查询结果行（`ai_strict_egress` 默认 `true`；`redact_for_ai` → strict）
- [x] 开启高敏感上下文时有明确「数据离开本机」提示（设置关闭确认 + `AiEgressNotice`）
- [x] `ai/safety` 单测覆盖常见密钥键、URI 用户信息、Bearer（7 项）
- [x] 既有 AI 单测不回归（215/215）
- [x] `cargo test -p datazen --lib` 全量通过
- [x] `npx tsc --noEmit` 通过

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| E1 | AI 已配置 | 设置 → AI → 关闭 Strict egress | 确认对话框文案可见；开关关闭 | 【留待 R 回归】 |
| E2 | strict egress off | 打开 AI Chat，附加 @ 文件 | egress 提示条可见 | 【留待 R 回归】 |

## 遗留

—
