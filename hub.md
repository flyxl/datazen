# P5 Hub Ledger (integration: `codex/p5-integration`, baseline `ec857bdd2`)

用户范围：P5 = JobRuntime + 数据迁移三件套。所有轨道只合并到 `codex/p5-integration`，整个 P5 完成前不合并 main。沿用 P4 协调的用户裁定：略过性能基准与文档/注释准确性审计。

上面历史 P3 hub 为误留内容，随 P5 收口一并清除（其合并时应已删除）。

## 波次

| Wave | Track | Branch | Worktree | Status |
| --- | --- | --- | --- | --- |
| 0 | `p5-job-core` JobRuntime/契约/持久化/预算/多端预留 | feature/p5-job-core | — | MERGED `bb2cc5606`，3 轮 Tester 后 TEST_PASSED，worktree/分支已清理 |
| 0 | `p5-domain-extract` 三件套抽 packages/*（纯机械，旧 IPC 保持可用） | feature/p5-domain-extract | — | MERGED `97885d6e0`，TEST_PASSED，worktree/分支已清理 |
| 1 | `p5-schema-diff` handler（Wave0 合入后开） | — | — | NOT_STARTED |
| 1 | `p5-data-sync` handler | — | — | NOT_STARTED |
| 1 | `p5-data-transfer` handler | — | — | NOT_STARTED |
| 1 | `p5-client` 任务中心/订阅/幂等回执 UI 适配 | — | — | NOT_STARTED |
| R | 全量回归 + 故障旅程 + 平台门禁 | — | — | NOT_STARTED |

## 契约接缝

- job-core 只碰 `packages/runtime`、`packages/application`、`packages/platform-api`、持久化模型与生成清单；domain-extract 只移动 `src-tauri/src/{schema_diff,data_sync,data_transfer,transfer}` → `packages/` 并接回 host 薄 adapter，不引入 JobRuntime。
- Wave 1 各 handler 轨在 Wave 0 合入 `codex/p5-integration` 后新开 worktree，各自只改自己的 `packages/<domain>` + `src-tauri/src/commands/<domain>` + 对应窗口最小接线。
- `progress.md` 仅存在于各轨分支；本文件与之合并时一律删除，不得带进 main。

## 记录（追加式）

- `97885d6e0` domain-extract 合入（`867bd81a2` 删台账）；`bb2cc5606` job-core 合入（`d2282dcbd` 删台账），3 轮验收：R1 FAIL(D1–D5) → 修复 → R2 FAIL(D6–D8) → 补测试/台账 → R3 TEST_PASSED。job-core 集成后 `cargo test -p datazen-runtime` EXIT=0，32 个 ok 块。
