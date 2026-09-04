# Track `rem-ci-guards` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-ci-guards）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

CI 接入三守护脚本。见计划 §2。

## 状态

- Phase: PASSED
- 编码 commit: 568f4b6f6
- 测试 commit: (tester commit, see git log)
- 合并 commit: —

## 心跳

- 2026-09-04 BOOTSTRAP: worktree clean, branch feature/rem-ci-guards
- 2026-09-04 CODING: ci.yml +3 steps, ci-local.sh +3 commands, allowlist fix
- 2026-09-04 TESTER: BOOTSTRAP ok (worktree .worktrees/datazen-rem-ci-guards, branch feature/rem-ci-guards, clean)；四阶段 A/B/C/D 完成

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

## 测试复验（Tester 独立实测 2026-09-04）

- 阶段 A（代码审查）：`git show 568f4b6f6` 三处改动逐项核对——
  - ci.yml +3 step（ID 术语强制 / CI-docs 强制 / i18n-sync `continue-on-error: true` warning），插入位置在结构守护之后、前端单测之前，顺序合理；无多余改动、无调试残留。
  - ci-local.sh +3 命令（3.1/11、3.2/11 强制 `|| fail`；3.3/11 warning 不阻塞），与 ci.yml 语义镜像一致。
  - check-id-terminology.mjs allowlist +1 条（`e2e/helpers.ts` `/configId/`），已核实该文件 L680-719 确为向后兼容 shim（`connectionId` 优先、`configId` fallback 旧版构建；双向注释齐全），allowlist 正则 `/configId/` 与命中行匹配。
  - 自报"ci.yml 129 行"实测为 128 行（`wc -l`），属行数口径差（尾换行），非功能问题。
- 阶段 B（独立重跑，`CI=true pnpm` 以跳过无 TTY 的 install 守卫；未执行 `pnpm install`）：
  - `pnpm test:ids` → exit 0（`[check-id-terminology] 5 allow-listed occurrence(s) skipped` / `ok (1205 files scanned)`）✅ 与自报一致。
  - `pnpm test:ci-docs` → exit 0（drivers ok 11 ids / window boundaries ok / toolchain ok Node 24 pnpm 11 Rust stable）✅ 与自报一致。
  - `node scripts/i18n-sync-check.mjs` → exit 1（预期内）⚠️；实测为 **136 missing + 192 stale × 8 locales**（de/es/fr/ja/ko/pt-BR/ru/zh-TW 各 17 missing + 24 stale），自报"17 missing + 24 stale × 8 locales"口径实为"每语言"，总数层面一致；自报"6 语言"实测为 **8 locales**（含 de + zh-TW），语言数口径有差，不影响 `continue-on-error: true` 选型结论。
  - YAML 结构有效性：`yaml.parseDocument` 零 error，无 tab 字符；三新增 step 名（`Guard ID terminology (connectionId / dbSessionId)` / `Guard CI-docs consistency` / `Guard i18n sync (warning only)`）解析确认，i18n step 含 `continue-on-error: true`。
- 阶段 C（fail 语义，全部在 /tmp 副本 + fixture 注入完成，工作树零触碰，事后已清理）：
  - ID 术语守护：在 fixture `src/bad.ts` 写入 `activeConfigId`，经 `checkIdTerminology({root: fixture})` → exit 1 并正确报出 `old prop name; use activeConnectionId` ✅ 守护能检出。
  - CI-docs 守护：`extractCiMatrixDriverIds` 对含 `unknowndriver123` 的文本能提取出未知 id（fail 路径可达）✅；真实仓库根 `checkCiDocsConsistency` → exit 0。
  - i18n-sync 守护：本轮实测 exit 1 即 fail 语义的活证据（warning 先行选型正确）；ci.yml / ci-local.sh 均不阻塞。
- 判定：验收标准（计划 §2 rem-ci-guards：ci.yml 新增三 step 其中 i18n warning 先行 + progress 注明选型；ci-local.sh 同步；本地验证 pass/fail 语义）逐项满足 → **PASSED**，无 Bug 登记。
