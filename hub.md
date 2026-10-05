# P5 Hub Ledger (integration: `codex/p5-integration`, baseline `ec857bdd2`)

用户范围：P5 = JobRuntime + 数据迁移三件套。所有轨道只合并到 `codex/p5-integration`，整个 P5 完成前不合并 main。沿用 P4 协调的用户裁定：略过性能基准与文档/注释准确性审计。

上面历史 P3 hub 为误留内容，随 P5 收口一并清除（其合并时应已删除）。

## 波次

| Wave | Track | Branch | Worktree | Status |
| --- | --- | --- | --- | --- |
| 0 | `p5-job-core` JobRuntime/契约/持久化/预算/多端预留 | feature/p5-job-core | — | MERGED `bb2cc5606`，3 轮 Tester 后 TEST_PASSED，worktree/分支已清理 |
| 0 | `p5-domain-extract` 三件套抽 packages/*（纯机械，旧 IPC 保持可用） | feature/p5-domain-extract | — | MERGED `97885d6e0`，TEST_PASSED，worktree/分支已清理 |
| 1 | `p5-schema-diff` handler | feature/p5-schema-diff | — | MERGED `4ba769fc9`，R1 FAIL(D1–D4)→修复→R2 TEST_PASSED（225+10 / runtime 32 块 / host 1711+0+6 / typecheck / vitest 87），worktree/分支已清理 |
| 1 | `p5-data-sync` handler | feature/p5-data-sync | .worktrees/datazen-p5-data-sync | CODING |
| 1 | `p5-data-transfer` handler | feature/p5-data-transfer | .worktrees/datazen-p5-data-transfer | FIX-ROUND（R1 **TEST_FAILED**：4 门禁全绿但 D1 blocker + D2–D4 high，详见下方记录；已派回原 coder） |
| 1 | `p5-client` 任务中心/订阅/幂等回执 UI 适配 | feature/p5-client | — | MERGED `5f46a0262`，TEST_PASSED（D1–D5 遗留：D2 hydration onUpdate 空实现、D5 schema-diff 窗口 step 未回退，记后续） |
| R | 全量回归 + 故障旅程 + 平台门禁 | — | — | NOT_STARTED |

## 契约接缝

- job-core 只碰 `packages/runtime`、`packages/application`、`packages/platform-api`、持久化模型与生成清单；domain-extract 只移动 `src-tauri/src/{schema_diff,data_sync,data_transfer,transfer}` → `packages/` 并接回 host 薄 adapter，不引入 JobRuntime。
- Wave 1 各 handler 轨在 Wave 0 合入 `codex/p5-integration` 后新开 worktree，各自只改自己的 `packages/<domain>` + `src-tauri/src/commands/<domain>` + 对应窗口最小接线。
- `progress.md` 仅存在于各轨分支；本文件与之合并时一律删除，不得带进 main。

## 跨轨事实与遗留

- **i18n 门禁盲区（缺陷 D-I18N-1，中，归门禁维护轨）**：`scripts/i18n-sync-check.mjs` 的 `LOCALE_FILES` 硬编码 8 个 host locale、**不含 zh-CN**（Tester 实证：仅加 zh-CN 的补丁副本即报 117 missing EXIT=1，原门禁全绿）；且 vitest 用 `getTranslation` 缺 key 静默回退 en 使断言恒过。**P5 各轨新增 en key 必须同时写 zh-CN**（中文 UI 会直接显示英文原文）；其余 8 locale 留给发布前 i18n-sync 收尾。
- **zh-CN 存量欠债**：baseline（集成分支）缺 117 key（sync 114 + settings 3），**main 现值缺 138**（另含 query 18 + schema 3）；全部可追溯到 v0.2.3 之前的迁移三件套/更新器提交，非 P5 引入。处置：另立**发布前 i18n 收尾轨**在 main 上统一补，不随 P5 各轨。
- **lazy domain 测试 fixture**：测试若把 `useLocaleDomains` mock 成恒 true，lazy `sync` pack 永不注册，而 `@datazen/ui` 组件走包内**真 `t()`**，会刷 `Missing translation` 警告。修法为 test-only：`import '../../../locales'` + `beforeAll(() => ensureAllLazyDomains('en'))`。data-transfer 套件与 `MigrationRunHistoryDialog.test.tsx` 仍有同根因残留（非阻塞）。
- **禁止 `cargo fmt --all`**：P5 各轨 worktree 同仓共存，`--all` 会顺手格式化**其他轨/禁区**的文件。Tester 实测入场时 `p5-data-transfer` 树里有 45 个文件的 `cargo fmt --all` 未提交漂移（含 `packages/data-sync/**`），后被外部还原。各轨一律 `cargo fmt -p <自己的 crate>`。
- **绿门禁 ≠ 功能接上**：`p5-data-transfer` R1 四门禁全绿、禁区 diff=0、台账数字真实，但审查出 D1 SQL 文件迁移被 §8 门禁判死（CM-49 测试**直驱 handler 绕过门禁**才没暴露）、D2 字节账只记不拦、D4 取消路径是死码（`cancelled: None`，全仓无 `Some`）。后续轨 Tester brief 必须包含「测试是否走生产门禁/接线路径」这一问；派单时也须要求 coder 的契约测试经 IPC/命令层而非直驱内部函数。
- 结构性观察（未修，属产品决策）：de/es/fr/ja/ko/pt-BR/ru/zh-TW 8 套字典完整但**运行时不可达**——`builtin-locales.json`、`lazyPacks.loaders`、`generated-locales.ts` 三处仅覆盖 en/zh-CN，实测 `setLocale('de')` 返回 en 文案。

## 记录（追加式）

- `97885d6e0` domain-extract 合入（`867bd81a2` 删 progress.md）。
- `bb2cc5606` job-core 合入（`d2282dcbd` 删 progress.md），3 轮验收：R1 FAIL(D1–D5) → 修复 → R2 FAIL(D6–D8) → 补测试/台账 → R3 TEST_PASSED。
- `0db23e6d0` i18n 合入（ledger 6e99a1701，TEST_PASSED，删 progress.md `1edcead4c`）：补齐 9 locale 的 migrationJob key + 修 3 个 schema-diff 测试 fixture（27→0 警告，生产代码零改动、断言未削弱）。合并后 sanity：i18n-sync-check EXIT=0、vitest schema-diff+locales 108 passed EXIT=0（本 worktree 需先补 codegen：generate-builtin-locales + resolve-drivers --codegen-only）。残留（既存、非本轨）：data-transfer 套件 9 行同类警告、MigrationRunHistoryDialog 1 行 → 用同 2 行 fixture 模板清或收敛进共享 setup。
- `4ba769fc9` schema-diff 合入（`2486ac679` 删 progress.md），2 轮验收：R1 FAIL(D1–D4) → 修复 `edf2713a0` → R2 TEST_PASSED；合并后 sanity `cargo test -p datazen-schema-diff -p datazen-runtime` EXIT=0。R2 遗留：`read_only_verify` 仍保守返回 Indeterminate；`schemaDiff.limitations.*` 警告已由 i18n 轨修复。
- `5f46a0262` client 合入（`1029e9fae` 删 progress.md）。
