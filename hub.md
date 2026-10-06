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
| 1 | `p5-data-transfer` handler | feature/p5-data-transfer | — | MERGED `3e28d49b8`，2 轮验收：R1 FAIL(D1 blocker + D2–D8)→修复 `760af1cf0`→R2 TEST_PASSED（209 / runtime 834 / host 1732 / typecheck），worktree/分支已清理。**遗留 D9（功能级 blocker，见下）** |
| 1 | `p5-client` 任务中心/订阅/幂等回执 UI 适配 | feature/p5-client | — | MERGED `5f46a0262`，TEST_PASSED（D1–D5 遗留：D2 hydration onUpdate 空实现、D5 schema-diff 窗口 step 未回退，记后续） |
| 2 | `p5-runtime-cancel-watcher` 内核级 stage 内取消 watcher（收敛 data-transfer 的 host 侧重复实现） | feature/p5-runtime-cancel-watcher | .worktrees/datazen-p5-runtime-cancel-watcher | CODING |
| 2 | `p5-frontend-cutover` 前端切 Job 路径 + §10 CM-40 / 丢回执 E2E（解 D9） | feature/p5-frontend-cutover | .worktrees/datazen-p5-frontend-cutover | CODING |
| R | 全量回归 + 故障旅程 + 平台门禁 | — | — | NOT_STARTED |

两轨基线均为 `7f56e59c4`。冲突面已核算为 0：cutover 只碰 `src/**` + `e2e/**`；runtime 只碰 `packages/runtime`、`packages/data-transfer`、`src-tauri/src/commands/data_transfer/**`。**两轨互为禁区**；runtime 轨另禁 `src-tauri/src/commands/sync/**` 与 `packages/data-sync/**`（data-sync 轨在跑），已就此向 data-sync coder 发过接缝通知。

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
- **跨轨待裁定：stage 内取消信号不可达**。`packages/runtime/src/job/runtime.rs:108/:164` 只在 **stage 之间**检查 `cancel_requested`，stage 执行期间无并发 watcher 去 cancel CancelToken。长 stage（data-transfer 的数据搬运、data-sync 的批量 apply、schema-diff 的多语句 deploy）一旦开跑就收不到取消——这是 job-core 的接缝缺口，而 runtime 属 Wave-1 各轨禁区，各 handler 轨无法自行修复。data-transfer D4 首现。**R2 Tester 已独立判定：这是 runtime 层固有设计缺口，非 handler 轨接线问题**——`runtime.rs:155` 在 `dispatch` 内新建 token，全仓唯一 `cancel.cancel()` 调用点在 stage 执行**之前**（`:163-166`），而唯一的 await `run_stage(spec, &cancel)`（`:177-180`）内部无任何轮询，且 `CancelToken` 以 `&` 传递，handler 原理上无法在 stage 内翻转或观察。data-transfer 用 host 侧 `spawn_cancel_watcher`（`job_api/runtime.rs:404-427`，50ms 轮询）绕过并在 `pipeline.rs:469-479` 回滚在途批次（不产生提交边界）达成合规。**注意**：R2 Tester 同时核实 schema-diff / data-sync **尚未接入 Job kernel**（无 `StageSpec`/`run_stage`），故今天不受影响，但迁移时会继承同一缺口并各自重写 watcher —— 该缺陷应记在 `packages/runtime` / P5 层面，而非某个 handler 轨。**已裁定 → `p5-runtime-cancel-watcher` 轨（见上），非挂起项。**
- **跨轨不变量：新 apply 路径必须 claim 存储侧 plan**（data-transfer D8，Tester 发现）。`job_api` 只 `peek_plan_any`、不调 `plans::claim_plan`，`plan.state` 停在 `Available` 直到 apply 成功才 `mark_consumed` → Job 失败/Unknown 后**旧 `execute_data_transfer` 仍可 claim 同一 planId 成功**，新旧双管理器重复执行，违反 §9「同一请求自始至终只走一种管理器」与 §2.1 的存储侧不变量（repository 的 `consumed_plans` 只在同一 Job host 内去重）。**R2 已修复**：apply 在 `job_api/mod.rs:246` 先 `claim_plan` 再 `:259` 起跑，并有 `tokio::join!` 真并发双 apply 测试。**data-sync / schema-diff 轨派单时必须同样检查这条**。
- 结构性观察（未修，属产品决策）：de/es/fr/ja/ko/pt-BR/ru/zh-TW 8 套字典完整但**运行时不可达**——`builtin-locales.json`、`lazyPacks.loaders`、`generated-locales.ts` 三处仅覆盖 en/zh-CN，实测 `setLocale('de')` 返回 en 文案。

### 用户裁定：D9 与 stage 内取消缺口各自立轨（已决定，非待定）

2026 本轮用户就两项范围问题各选推荐项，**不再挂起**：

- **D9 → `p5-frontend-cutover`**：开独立前端切换轨（`src/commands/**`、`src/windows/data-transfer/**`、`src/components/**`、`src/hooks|lib|stores/**`、`src/locales/**`、`e2e/**`），把 UI 从 `execute_data_transfer` 切到 `prepare_data_transfer_job` + `apply_data_transfer_job`，让 §2.3 取消标志与 §7 提交边界/恢复裁决**对用户可见**，并补 §10 的 CM-40 窗口关闭/恢复、丢回执不重复 apply、UI 层执行中取消 E2E。**三件套一并纳入同一轨的接缝设计**（本轮只切 data-transfer：schema-diff 宿主侧尚无 prepare/apply Job 命令，data-sync Job 链路仍在开发；coder 必须留出可复用形状，后续低成本接入而非各件套重写）。`src-tauri/**` 为该轨禁区——需要后端支撑只报不改。
- **stage 内取消缺口 → `p5-runtime-cancel-watcher`**：开独立 runtime 轨（`packages/runtime` + 宿主取消通道），在 `run_stage` 执行期间起并发 watcher 翻转 token，随后退役 data-transfer 的 `job_api/runtime.rs:404-427` host 侧重复实现，使所有 handler 轨共用**一个**内核实现。C7 明确允许：若收敛会显著放大风险面（如改 `JobHandler` trait 波及 `packages/schema-diff` 禁区），**正确性优先于整洁度**，保留重复实现并记录后续方案。

### 阻塞 P5 功能级收口（非某轨缺陷）

- **D9 — 新 Job 命令零调用方，§2.3/§7 对用户不可见（功能级 blocker）**。R2 Tester 实测：`grep -rn "apply_data_transfer_job|prepare_data_transfer_job" src/ e2e/` **零命中**——两条命令已在 `bootstrap/run.rs` 注册，但前端 `src/commands/transfer.ts:294` 仍调旧 `execute_data_transfer`，所有 WDIO transfer spec 也全走旧路径。`exec.rs` 不发布任何 `commit_boundaries` / `recoveryVerdict` / `StageTerminal` / `CancelToken`，即提交边界、恢复裁决、取消标志全部**已实现但用户碰不到**。这是 R1「绿门禁 ≠ 功能接上」的上一层重演。**不是本轨缺陷**：`src/commands/**` 是该轨禁区，Tester 明确不因此判失败；§9 单次消费在真实路径上仍成立（`exec.rs:275` 已 `claim_plan`）。**但「4 门禁全绿」不得读作「已交付」** —— 需一条前端切换轨才能做功能级收口。schema-diff / data-sync 轨收口时同样要问这条。**已裁定 → `p5-frontend-cutover` 轨（见上），非挂起项。**
- **D10 — 低，非阻塞（测试靶向精度）**。`job/pipeline/budget.rs:63-71` 与 `:78-84` 都把 `used` 夹到 `capacity`，`max_used()` 永远不超 `PIPELINE_INITIAL_BYTES`，导致 `handler.rs:720-726` 的 `unbounded_stage(...)` 防御分支在生产路径上不可达；`cm46_budget_overrun_keeps_the_confirmed_boundaries`（`tests/cm46_pipeline.rs:560-584`）只**直驱 helper**，未证明真实超预算批次走生产路径。底层强制（D2）本身正确，属覆盖率加固。

## 记录（追加式）

- `97885d6e0` domain-extract 合入（`867bd81a2` 删 progress.md）。
- `bb2cc5606` job-core 合入（`d2282dcbd` 删 progress.md），3 轮验收：R1 FAIL(D1–D5) → 修复 → R2 FAIL(D6–D8) → 补测试/台账 → R3 TEST_PASSED。
- `0db23e6d0` i18n 合入（ledger 6e99a1701，TEST_PASSED，删 progress.md `1edcead4c`）：补齐 9 locale 的 migrationJob key + 修 3 个 schema-diff 测试 fixture（27→0 警告，生产代码零改动、断言未削弱）。合并后 sanity：i18n-sync-check EXIT=0、vitest schema-diff+locales 108 passed EXIT=0（本 worktree 需先补 codegen：generate-builtin-locales + resolve-drivers --codegen-only）。残留（既存、非本轨）：data-transfer 套件 9 行同类警告、MigrationRunHistoryDialog 1 行 → 用同 2 行 fixture 模板清或收敛进共享 setup。
- `4ba769fc9` schema-diff 合入（`2486ac679` 删 progress.md），2 轮验收：R1 FAIL(D1–D4) → 修复 `edf2713a0` → R2 TEST_PASSED；合并后 sanity `cargo test -p datazen-schema-diff -p datazen-runtime` EXIT=0。R2 遗留：`read_only_verify` 仍保守返回 Indeterminate；`schemaDiff.limitations.*` 警告已由 i18n 轨修复。
- `5f46a0262` client 合入（`1029e9fae` 删 progress.md）。
- `3e28d49b8` data-transfer 合入（`6385eff49` 删 progress.md），2 轮验收：R1 FAIL(D1 blocker + D2–D8) → 修复 `760af1cf0` + 台账 `22f685a6d` → R2 TEST_PASSED（Tester `aa8cb6f1` 独立复跑 D1–D8 全部在**生产调用点**核实通过，11 条具名测试逐条单独执行而非仅编译；禁区 diff=0、新增告警 0、新增生产 unwrap=0、最大文件 789 行）。R2 遗留：D9（功能级 blocker）、D10（低，覆盖率）。
  - **合并期缺陷（git 按行合并不报冲突）**：`422341c4b` 在自己的基线 `214c44565` 上新增 `datazen-runtime` 依赖，而集成分支早已由 schema-diff 轨在 `src-tauri/Cargo.toml:24` 引入**完全相同**的一行。两行行位不同 → 无文本冲突 → 留下重复键，整个 workspace `cargo metadata` 失败（EXIT=101）。已由 `79ea4a872` 去重修复。**教训：各轨基线不同，集成分支已存在的依赖/注册项，合并后必须重跑 manifest 解析**；合流 sanity 的第一步应是 `cargo metadata --no-deps`。
  - **集成 worktree `node_modules` 退化**：`datazen-p5-integration/node_modules` 已从软链变成只含 `.vite` / `.vite-temp` 的真实空目录，导致 `tsc: command not found`（typecheck EXIT=127、误报 `errorTS=0`）。已重挂软链。同批检查 `datazen-p5-data-sync` / `datazen-p5-data-transfer` 两树软链完好。**教训：typecheck EXIT=127 且 grep 计数为 0 时，先怀疑工具链缺失而非通过**。
  - 合并后 sanity：`cargo test -p datazen --lib` 1733 passed EXIT=0（比轨内 1732 多 1，来自已合并的 schema-diff 轨）；`pnpm typecheck` EXIT=0 且 errorTS=0；`npx vitest run` 567 files / 5948 tests EXIT=0。
- 用户裁定 D9 与 stage 内取消缺口**各自立轨**（非挂起），Wave-2 两轨同时开跑、基线 `7f56e59c4`：`p5-runtime-cancel-watcher`（coder `3f52e76d`）与 `p5-frontend-cutover`（coder `1ea0f84f`）。已核实的事实：`apply_data_transfer_job` / `prepare_data_transfer_job` 已在 `bootstrap/run.rs:453/460` 注册，`cancel_data_transfer`（`mod.rs:311-323`）已优先路由到 Job repo——**前端无需新增任何 Rust 命令**，cutover 轨可以纯前端完成主切换。schema-diff 侧 handler 在 `packages/schema-diff/src/job/handler.rs` 但**宿主无 prepare/apply Job 命令**（`grep` 零命中），故本轮 cutover 只切 data-transfer。已向 data-sync coder 发出接缝通知：禁止再造 host 侧 `spawn_cancel_watch`，取消语义对齐 data-transfer 样板（execute 后 commit 前检查 → 回滚在途批次 → break，不产生提交边界）。
- **cutover 轨第一次死亡（网络）**：coder `1ea0f84f` 中途消失，无收尾消息，**零提交**。工作树现场：HEAD 仍 `7f56e59c4`，7 个文件未提交——新 `src/commands/transferJobs.ts`(182) / `src/hooks/useTransferJobRun.ts`(248) / `src/lib/migrationJobVerdict.ts`(303) / `src/components/migration/MigrationJobVerdictPanel.tsx`(255) / `MigrationJobFailureNotice.tsx`(121)，加 `src/locales/{en,zh-CN}/sync.ts`。**主切换（`transfer.ts:294` 仍调 `execute_data_transfer`）、`src/windows/data-transfer/**` 接线、`e2e/` 三块零改动**。已 `send_message` 恢复同一实例（保留其上下文中的未提交代码），并要求**先 commit 现有产出**再续做。
  - 待其自查：en +61 行 vs zh-CN +52 行**不等**，可能是嵌套结构也可能是漏译，须跑 key 集合 diff 证明一致（zh-CN 为主语言）。
  - **教训：长任务 coder 必须按阶段提交**，全押在一个最终 commit 上，一次网络抖动即损失整轮上下文。
- **cutover 轨第二次死亡，实例已弃用**：恢复 `1ea0f84f` 后**它一个字都没写**（HEAD 仍 `7f56e59c4`、7 个文件原样未变、`transfer.ts:294` 未切、windows/e2e 零改动）便再次消失——该实例每次开口即死，非任务难度问题。已按规则换**全新实例 `fd466b08`**：brief 要求它先 read 那 5 个未提交文件并**接管复用**（明令不得从零重写，那正是上一轮的死因），并把「立刻 commit 现有产出、之后每完成 T1/T2/T3 一块就 commit 一次」列为**第一个硬性动作**。
  - 现场未丢失：1100 行产出仍在树里，可由任何新实例接手。
- **`p5-runtime-cancel-watcher` 交付 `9affc0740`**（单提交，基线 `7f56e59c4`），已派全新 Tester `e72f46ce`，**未合并**。自查：runtime 841（基线 834+7）、data-transfer 209 持平、`datazen --lib` 1733+6 ignored、typecheck 0、禁区 diff 0、新增告警 0。改动：内核 `run_stage_watched` + `CancelToken::flag()`（`JobHandler` 签名未变）+ 删宿主 `spawn_cancel_watch`（44 行）与 data-transfer 的 `cancel_flag` 通道。C7 收敛成功未启用豁免。C3 决策 = 轮询失败 `warn` 后停轮询、让在跑阶段走完（理由：读失败时无法区分「无取消」与「有取消但读不到」）。
  - **待 Tester 重点核验**：C1 两个用例是否为真并发（coder 称用 Notify/await、收到 Entered 才 request_cancel）；C6 存在自曝缺口——`cm46_cancel_flag_stops_the_pipeline_before_the_first_write` 自建 `Arc<AtomicBool>`、**不走新的 `cancel.flag()` 路径**，需判定 data-transfer 究竟有无「handler 经 `flag()` 在 stage 内观察到内核看守者翻转」的覆盖（`sqlfile.rs` 那处从入口快照改共享 token 同理）；`repository.rs` 的 `fail_get()`/`get_calls()` 是否真在 production 编译下消失；`active_cancel_watchers()` 是否为生产常驻全局计数器；阶段边界检查与看守者启动之间的丢取消窗口；50ms × 每阶段的轮询开销。
- ⚠️ **地雷（已广播给 data-sync coder）：仓库基线本身不是 rustfmt-clean**，`cargo fmt -p <crate>` 会重排该 crate 大量历史未格式化代码，**已实测溢出到禁区** `src-tauri/src/commands/schema_diff/**` + 20 余个无关文件（靠 `git checkout` 全量还原才保住）。**「别跑 `cargo fmt --all`」这条纪律不够**，正确的做法是**根本不跑 `cargo fmt`**，改为手工对齐自己改动的文件；提交前用 `git diff --numstat <基线>..HEAD -- <禁区路径> | wc -l` 复核为 0。
