# Track `rem-ci-guards` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-ci-guards）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

CI 接入三守护脚本。见计划 §2。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: (see git log)
- 测试 commit: —
- 合并 commit: —

## 心跳

- 2026-09-04 BOOTSTRAP: worktree clean, branch feature/rem-ci-guards
- 2026-09-04 CODING: ci.yml +3 steps, ci-local.sh +3 commands, allowlist fix

## 自验结果

- `pnpm test:ids` → exit 0 ✅（1205 files scanned, 5 allow-listed）
- `pnpm test:ci-docs` → exit 0 ✅（drivers ok, window boundaries ok, toolchain ok）
- `node scripts/i18n-sync-check.mjs` → exit 1（expected: 17 missing + 24 stale × 8 locales；ci.yml 用 `continue-on-error: true` 先行 warning）
- YAML 语法验证通过（无 tab 字符，129 行）

### 选型说明
- i18n-sync-check 当前 6 语言各缺 17 key + 24 stale，强制会红——选型为 `continue-on-error: true` warning 先行，不阻塞既有 CI 红线。后续补翻译后可改为强制。
- test:ids 与 test:ci-docs 强制（exit 1 会阻塞 CI）。
- e2e/helpers.ts 含 `configId` 向后兼容 shim（connect IPC 对旧版 fallback），已加入 allowlist。

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| CI 三守护 pass/fail 语义 | 需 CI | 本机验证脚本语义，CI 行为留待观察 |
