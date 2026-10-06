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
  - ✅ Tester 判定的部分：C1/C2/C4/C5/C7 **全部通过**（真并发经 `wait_for_kernel_cancel` 的 stage 内 sleep 证实、`!cancel_at_entry` 排除伪并发；join 在 `?` 之前故错误路径也回收、两条路径都只减一次；取消绕不过 `pipeline.rs:576` 的 `boundaries.push`；阶段循环无新增提前 break；禁区 diff 0、`JobHandler` trait 与基线逐字节相同、文件全 ≤800 最大 780）。测试缝 `fail_get`/`get_calls` 确认 `cfg(any(test, feature="test-harness"))` 门控且该 feature 预先存在（非本提交新增）、全仓仅 runtime 自己的 `[dev-dependencies]` 自引用启用，`test-harness` **未泄漏进生产构建**；`active_cancel_watchers()` 是**每 `JobRuntime` 的 `Arc<AtomicUsize>`** 字段、无 mutex 无 static、无副作用。丢取消竞态**不存在**：边界检查是全新 `repo.get()` 非缓存、看守者在 `record_stage` 成功后才起、`watch_cancel_request` **首次 sleep 前就先读一次**，`request_cancel` 接受 `Running`，最坏检测延迟约 50ms。
- ⚠️ **`p5-runtime-cancel-watcher` Tester 判 `TEST_FAILED`，已返修一轮，未合并**。四门禁独立复跑全绿且与 coder 数字一致（runtime 841 / data-transfer 209 / host 1733+6 ignored / typecheck 0，`error_TS_COUNT=0` 是真绿非 EXIT=127 假绿），但**变异测试判定 C3/C6/C8 判负**：
  - **D1（high，阻断）**：把 `handler.rs:401`/`:701`、`sqlfile.rs:33` 三处 `cancel.flag()` 全还原成改造前入口快照 `Arc::new(AtomicBool::new(cancel.is_cancelled()))`，**data-transfer 209/209 与 runtime 7/7 全部照绿** ⇒ data-transfer↔kernel 的取消集成是**零覆盖的装饰**。根因是既有测试自建 `Arc<AtomicBool>` 并直驱 `execute_bounded_table`（`cm46_pipeline.rs:297,373`），从不经 `run_stage`、从不取 `CancelToken`；`cm49_sql_file.rs:72,157` 建了 token 但从不取消。**coder 删掉了原本唯一的生产侧证据（宿主 `spawn_cancel_watch`），换上一条无人验证的路径。**
  - **D2（medium，阻断）**：`job_cancel_watch.rs:545-551` 的 `get_calls()` 稳定性断言取在 `run.await` **之后**，此时看守者早已 abort+join，计数冻结是因为任务死了而非因失败停轮询；删掉 warn 后的 `return;`（把决策 (a) 反转成「继续轮询」）仍 **7/7 全绿**。决策 (a) 本身被判**有意识且合理**（不凭空制造取消、健康 Job 被写成 Cancelled 不可恢复、fail-closed 会杀掉已 checkpoint 的长任务、绝不静默吞），且首次 `Err` 即 `return` 故**不刷屏**，这几点保留。
  - **D3（medium，阻断）**：文档 `data-migration-jobs.md:76` 与注释 `runtime.rs:368-370` 同声称「取消意图在下一个阶段边界或下一次运行重新读到——不得把取消悄悄丢弃」。**单阶段 Job 没有下一个阶段边界，而单阶段正是 data-transfer 的常态**（`handler.rs:269-273` `TransferMode::Data`、`:249-257` SQL 文件 apply、`:259-263` prepare，`cm49_sql_file.rs:70` 断言 `stages.len()==1`）⇒ 一次读失败就彻底丢取消、Job 报 `Succeeded`/`Completed`、用户无感。Tester 如实标注**可达性很窄**（`get` 仅能经中毒锁映射成 `BackendUnavailable("lock")`，`repository.rs:260-264`），故判为**文档诚实性缺陷**而非高概率事故。
  - **D4（medium）**：`runtime.rs:31-33` 称 50ms 轮询是「进程内 dict 查找、开销可忽略」——**事实错误**：`repository.rs:352-356` 的 `get()` 拿**全局单一 `std::sync::Mutex`** 并 `row.record.clone()` **深拷贝整条 JobRecord**（含 `JobDefinition.payload: serde_json::Value` 与 `Vec<StageRecord>`，`platform-api/src/dto/job.rs:70,95`）**且拷贝发生在持锁期间**，每秒 20 次全部争抢 `request_cancel`/`record_stage`/进度共用的锁。Tester 同时澄清 `SelectAll` **不放大**（阶段顺序执行、每 Job 一个看守者、首个 `true` 即退出），`get()` 内无 `.await` 故不跨 await 持锁、无死锁。
  - D5 `handler.rs:23` 模块文档仍写「快照式轮询」已被推翻；D6 文档「`dispatch` 返回时不存在游离任务」在 **handler panic 路径**不成立（`Drop` 只 `abort()` 不 `await` 且立刻递减 `live`）；D7 `handler.rs` `validate_plan` 末尾少一空行（fmt 事故残留）。
  - 返修单已写死两条**变异敏感的验收线**：D1 新测试在「还原 `flag()`」时**必须红**、D2 新测试在「删掉 warn 后 `return;`」时**必须红**，coder 须自证并写进报告。D4 允许改注释或加 `cancel_requested(ctx, job_id) -> bool` 标量端口方法，但**须先查清 `JobRepository` 是否有与 data-sync 共用的实现者**（宿主适配器用到 `committed_boundaries`），若触及禁区则退回「改注释 + 拉长间隔」。
- 🔑 **方法论收获（最高价值的一条）**：**绿门禁 ≠ 被测到**。本轨四个门禁全绿、7 个新用例逐条单跑全过，但变异 A1/A2（摘掉 `cancel.cancel()` / 零轮询）能杀死 2 个用例 ⇒ kernel 部分**确为真覆盖**；变异 A3/B 全绿 ⇒ 另两个声称的行为**根本没被测到**。这是本项目第二次踩同一形态（第一次是 data-transfer R1 的「绿门禁 ≠ 功能接上」）。**今后 Tester brief 必须写死变异核验项**，且**coder 的验收标准要以「变异后是否变红」表述**，而不是「测试通过」。Tester 自曝其首个变异脚本因锚点不匹配而静默 no-op、重写为断言锚点命中数后才可信——**变异脚本本身也要自证**。
- ⚠️ **地雷（已广播给 data-sync coder）：仓库基线本身不是 rustfmt-clean**，`cargo fmt -p <crate>` 会重排该 crate 大量历史未格式化代码，**已实测溢出到禁区** `src-tauri/src/commands/schema_diff/**` + 20 余个无关文件（靠 `git checkout` 全量还原才保住）。**「别跑 `cargo fmt --all`」这条纪律不够**，正确的做法是**根本不跑 `cargo fmt`**，改为手工对齐自己改动的文件；提交前用 `git diff --numstat <基线>..HEAD -- <禁区路径> | wc -l` 复核为 0。
- 📌 **`main` 已被另一个更早的阶段推前（与 P5 无关）**：`main` 由 `b096ffbb4` 前进到 `71c9dce75`，是 P3fu 三轨（registry / tunnel / fake）合并，共 +6170/−977；`b096ffbb4` 仍在 main 祖先链上，**历史未被改写，P5 一行都没进 main，用户禁令未被触碰**。已核实 P3fu 改动的 35 个文件与 P5 的 `packages/runtime/src/job/**`、三件套（`packages/{schema-diff,data-sync,data-transfer}/**`）、`src-tauri/src/commands/{schema_diff,data_transfer,sync}/**` **零重叠**，也未动任何 `Cargo.toml`/`Cargo.lock` ⇒ P5→main 的最终合并不预期产生语义冲突。但集成分支现**落后 main 67 个提交**（P5 独有 48），故 **Wave-R 全量回归必须在合入 main 之后、在 main 上重测一次**，不能拿集成分支的旧数字当结论。
- **`p5-data-sync` 交付 `36d2682c6`**（分支 `91678192b`→`9e640ac06`→`36d2682c6`，基线 `214c44565`，25 files +3260/−460，工作树干净，`progress.md` 已入库待合并时删），已派全新 Tester `95b04b87`，**未合并**。自报门禁 data-sync 200 / runtime 834 / host 1718+6 ignored / typecheck 0，禁区 diff 0，未跑任何 `cargo fmt`（只对本轨文件逐个 `rustfmt --check`）。新增 `sync/host/{mod,executor,recording,selection,state,statements}.rs`、`sync/jobs_contract.rs`、`sync/artifact_view.rs`；改写 `sync/{jobs 3→472, exec 1019→972, apply 1103→966, plans, mod, filter_validation}`、`packages/data-sync/src/job/*`、`src-tauri/Cargo.toml`、`Cargo.lock`。
  - ⚠️ **基线偏斜，合并期勿误判**：本轨基线 `214c44565` 比集成分支旧，故其宿主测试数 **1718 低于集成侧 1733**，是正常的、**不得判为测试丢失**。coder 自报「detached 基线实测 1710 → 交付 1718 = +8 恰为本轨 8 条新契约测试」，该减法已要求 Tester **独立复测**（在 detached 的 `214c44565` 副本上跑同一命令对比）。**合并进集成分支后必须以集成分支自身重新基线化，不能沿用任一轨的数字。**
  - 本轨自查修掉的三个**真缺陷**（值得记：都是"缺陷会伪装成正常运行"的类型）：① `select_by_key_sql` 漏主键绑定谓词，生成 `WHERE pk IS NOT NULL` 却没有键值 ⇒ apply **误报** `TargetConflictRows`（不是漏报而是假报，比漏报更隐蔽）；已按方言生成 1-based 占位符谓词（postgres `${i}[::cast]`、MySQL `?`）+ `ORDER BY`，并在 mock driver 加 **opt-in** 的 `filter_rows_by_key_equality`（默认 false）。② apply 失败原因被 `let _ = err;` 吞掉 ⇒ 失败集合恒空、IPC 只剩通用文案；改为 `host.record_stage_failure("apply", …)` 使错误码随 Job 终态上报。③ **基线既有**的 `comparison_store` 子进程 ready 文件半写竞态（父进程轮询文件存在、子进程 `fs::write` 先建文件后写内容 ⇒ 读到 0 字节 ⇒ `PathBuf::from("").file_name()` 为 `None` ⇒ `unwrap()` 崩，上一轮 GATE3=101 即此因），改为 staging 文件 + `fs::rename` 原子发布。
  - 🚧 **收敛欠账（必须留痕，不许烂在里面）**：本轨基线的 runtime **只在 stage 之间**检查 `CancelToken::is_cancelled`、**没有** `CancelToken::flag()`，故按第 39 轮要求**没有**再造 host 侧 `spawn_cancel_watch`/轮询器，而是自建 host 侧 `Arc<AtomicBool>` 桥：`cancel_job` 置位、生产分页源/目标执行器在**批次循环内**轮询同一个 flag（`sync/host/executor.rs:40,103,124`，flag 由 `sync/jobs.rs::open_job` 从 `sync/host/state.rs::cancel_flag` 取得）。coder 明确标注这是**临时**方案，待 `p5-runtime-cancel-watcher` 合并后改用内核 stage 内取消，届时删除该 flag 桥（`cancel_job` 对 token 的取消保留）。**触发条件**：runtime 轨合并进集成分支之后。**验收线**：`cancel_job_reaches_the_flag_the_running_stage_polls` 必须经变异证明 load-bearing（切断桥接后应变红），否则它就是 D1 同型的装饰。
  - 待裁定：`resolve_db_name` 永不为错 ⇒ 生产路径**没有**"缺数据库作用域"的拒绝分支，现状被 `an_empty_database_scope_resolves_to_the_connection_database` 钉成"回落连接默认库"——**这是否违反 §8 backendScope fail-closed** 属设计裁定，已让 Tester 只给判断不改码。其余遗留：ChangeSet 产物驻留内存（重启丢失而非显式标记失效）；无真实 DB ⇒ 方言级行为（占位符渲染、批量原子性、真实隔离级别）未真机验证；基线既有 dead code 未清理。
- ⚠️ **门禁偷换（可复用的检查项）**：runtime 轨返修交付 `7d6a0f68d` 自报 4 条门禁 = `cargo test -p datazen-data-transfer`(211) / `cargo test -p datazen-runtime`(842) / **`cargo clippy`** / **`cargo check -p datazen`**，**把 R1 要求的 `cargo test -p datazen --lib` 与 typecheck 两条换成了编译类命令**。`cargo check` 只保证编译通过、**不执行任何测试**，而本轨改的正是取消语义、宿主测试才是真正跑过这些路径的那一层。已在 Tester brief 里点名要求补跑。**教训：门禁清单里出现 `check`/`clippy` 顶替 `test` 时，一律视为该门禁未跑**，不能因为它 EXIT=0 就算通过。
- **runtime 轨返修 `7d6a0f68d`**（round-2 单提交 10 文件 +580/−17；累计 vs `7f56e59c4` 为 13 文件 +1416/−74；工作树干净；`Cargo.lock` 全程未动；两棵变异树已清理）。已派全新 Tester `0081ec9a`，**未合并**。
  - D1 修法：新增 `packages/data-transfer/src/job/tests/kernel_cancel.rs`(281 行)——`kernel_cancel_stops_the_data_stage_before_the_first_commit` 声称把真实 `DataTransferHandler::run_stage` 跑在 `JobRuntime` 里、目标端首个 `execute_with_params` 被 `SlowGate` 卡在执行**中途**再 `request_cancel`，断言 `Cancelled` + `RolledBack` + 提交边界与 `target.committed` 均空。自证：三处 `cancel.flag()` 退回旧写法后 data-transfer `209 passed; 2 failed` EXIT=101（新用例报 `Succeeded/Cancelled`，**旧 209 条照绿**）。
  - D2 修法：拆出 `packages/runtime/tests/job_cancel_watch/poll_fault.rs`(199 行) 接入父文件，父文件压到 664 行；新用例在**阶段仍未放行时**采样读次数、跨 400ms 静默窗断言零增长且 WARN 恰好一条。自证：删 `runtime.rs:390` 的 `return;` → `7 passed; 1 failed` EXIT=101，`left: 11 / right: 4`。
  - D3 修法：**改措辞不改行为**，文档 `:76` 与 `runtime.rs:370-378` 改为「若存在下一个阶段则在该边界重新读到；否则本次运行可能在不感知取消的情况下走完」，并写入「data-transfer 的 data/apply/prepare 就是单阶段」这一事实；**「不得悄悄丢弃」的保证已删除**，改为故障时发一条带 `jobId` 与错误原文的 WARN 后停止轮询。理由：读失败仅能经互斥锁中毒（`repository.rs:260-264`，实践不可达）；若改成「标记取消降级」必须给 JobRecord 加状态或写 `pending_verification_reason`，会把已提交若干批、带 checkpoint 可续跑的**健康作业**推成 pending-verification，踩 C4 与 §7。**该论证是否成立，已要求 Tester 独立评估（可不同意 coder，但不得改码）**。
  - D4 修法：**未走 fallback，而是改了实现**——`repository.rs` 新增 `CancelPollSnapshot`(`:53`) 与**固有方法** `cancel_poll()`(`:118`)，持同一把全局锁只读 `cancel_requested`/`terminal` 两个 bool、**锁内零拷贝**（消除 R1 查实的「`get()` 持锁深拷贝整条 `JobRecord` 含 `serde_json::Value` payload」）；`runtime.rs:31-41` 注释改为如实描述该代价。coder 主张未动任何 trait：`impl JobRepository` 全仓只有 `InMemoryJobRepository` 一个，`platform-api/src/ports/job.rs:28` 的同名 trait 是另一个面向宿主的契约、两者无继承关系，`JobRuntime.repo` 持有的是具体类型。**已初查 round-2 diff 确认只新增了 `pub fn cancel_poll(`（确为固有方法）**，但整条推理链、以及「data-sync 轨是否引用了本轮触及的符号、合并后会不会编译不过」已列为 Tester 必答项——**这是合并期风险，不是可以略过的细节**。
  - 遗留可疑点（已点名让 Tester 判）：① `pipeline_reads_thesame_bit_the_kernel_flips`「测试侧直接 `cancel()` 再断言 `flag()` 为真」疑似**同义反复**（只证明 token 自翻，未证明内核会翻）；② `cancel_poll` 若绕过 `get_fault` 故障注入，则 D2 新用例可能在量一条**永远不会故障**的路径——即便能扛住变异也可能整体失效，须直读实现判定而非只看测试是否通过。
- ✅ **`p5-runtime-cancel-watcher` 已验收通过并合并**，merge commit **`105494c14`**（`7d6a0f68d`，零冲突自动合并）。**工作树/分支已清理。**
  - 判决依据不是门禁而是**变异**：R1 的失败形态是「还原接线后 7 条新用例全绿」，R2 反向验证——还原三处 `cancel.flag()` 后 **2 条变红**（`kernel_cancel.rs:221` 与 `:268`）而**基线 209 条保持全绿**（这正是"新接线只由新测试证明"的正确形态）；删 `return;` 变红；`cancel_requested` 置 false 变红；`cancel_poll` 直接 return 则 5/8 变红。Tester 亦逐条驳回了我此前列的两处怀疑：`pipeline_reads_the_same_bit_the_kernel_flips` **不是同义反复**（阶段在令牌未置位时就 spawn 并抓住句柄，`:258` 早于 `:260` 的翻转，快照永远看不到）；故障注入**确实**到达 `cancel_poll`（`left: 11` 只在函数真被进入时才可能大于 `before: 4`）。
  - 合并面核实：内核接线确已落地（三处 `cancel.flag()`），宿主第二套取消通道 `spawn_cancel_watch`/`CANCEL_WATCH_INTERVAL` 归零，全仓 token 翻转点恰为 2（阶段边界 + 看守者），`cancel_poll` 在位，新增两个测试文件在位；最大文件 781 行。
  - 合并门禁（`cargo metadata --no-deps` 先行，无重复键）：宿主 `1733 passed; 6 ignored` / runtime `842 passed` / data-transfer `211 passed` / `typecheck` 0 error / `vitest` **567 files 5948 tests**（与基线逐字一致）/ 禁区 diff 0 / `Cargo.toml`·`Cargo.lock` 未动。
  - ⚠️ **Tester 的方法论事故，值得记住**：多个变异**共用一个 `CARGO_TARGET_DIR`** 时，某次变异报出 `ok. 8 passed` 用时 0.92s——而该用例需等 5s，**物理上不可能**，是**陈旧产物**造成的假绿；换独立 target dir 后同一次变异为 `6 passed; 2 failed`。**每个变异必须用独立的 `CARGO_TARGET_DIR`**，否则一次变异campaign 可能把判决带偏（当时它差点指向一个假的 `TEST_FAILED`）。**变异脚本须自证锚点，并自证 target dir 干净。**
  - 归档 4 条非阻断缺陷，已开硬化轨 `feature/p5-cancel-hardening`（基线 `105494c14`，coder `9c02fd13`）收掉，**必须在代码/测试/文档里落成事实**：**D-C（必做）文档事实错误**——`data-migration-jobs.md:76` 与 `runtime.rs:379-380` 称「单阶段 Job（data-transfer 的 data / apply / prepare）」，但已核实 `handler.rs:245-292` 的 `validate_plan` 在 **`StructureAndData` 模式下 apply 发射 3 个阶段**（`structure`→`data`→`foreignKeys`），那恰是"结构+数据一起迁"最常见的全量形态；**且无任何测试钉住该 3 阶段映射**，文档因此会悄悄说谎，须补测试钉死。**D-A** `poll_fault.rs:149-155` 缺计数器活性前置断言（删 `repository.rs:127` 的 `fetch_add` 时 8 条全绿、`after==before` 空洞通过，仅靠 `:156` 的 WARN 计数兜住 ⇒ 诊断力退化）。**D-B** `repository.rs:141` 的 `terminal` 提前返回分支零覆盖。**D-E** 补注释说明 data-transfer 无 `test-harness` feature ⇒ 只能断言终态，与 runtime 侧的"证明位会被翻"互为互补。**D-D 经我核实未落进任何代码注释**（"两个独立 trait"的错误推理只在报告里），结论本身正确，**无需改码**。
  - 遗留（本项目全项目级，8 条仅登记不实现）：仍无任何层级能证明「点取消能在 ~50ms 内停掉运行中的迁移」；`CANCEL_POLL_INTERVAL = 50ms` 只是常量，**从未实测**；`StructureAndData` 三阶段链的阶段间取消无用例；`kernel_cancel.rs` 用 fixture 目标端、**无真实驱动往返**；「失败不 fail-closed」这一取舍的理由是"长作业可断点续跑"，**而『取消后续跑且不重复提交』这一兜底假设本身没有任何测试**；端到端读故障注入未做（`PortError::BackendUnavailable` 的真实可达性只是论证）；阶段内真 panic 的看门狗活性未测。

### ❌ `p5-data-sync` 判 **`TEST_FAILED`**（Tester `95b04b87`，交付 `36d2682c6`）→ 已并入修复轮基线 `25d336ece`

- 四道门禁 Tester 独立重跑全绿且**逐条复现 coder 自述**：`datazen-data-sync` 200 / `datazen-runtime` 834 / 宿主 `1718 passed; 0 failed; 6 ignored` EXIT=0 / typecheck 0 error。基线 `214c44565` 为 1710 ⇒ **+8 精确，无测试丢失**。`cargo metadata --no-deps` META_EXIT=0、packages=25、dup_keys=0；禁区 diff 0；14 个新增生产文件零 `unwrap`/`expect`；最大新文件 `host/mod.rs` 780 行；`.env` 全程未打开。
- **判决依据不是门禁而是变异。D1（高危，阻断）**：取消桥接的"自述行为"**零覆盖**。`jobs_contract.rs:508` `cancel_job_reaches_the_flag_the_running_stage_polls` 直接调 `state::cancel_flag` **绕过 `open_job`**，再断言自己刚拿到的那个对象——测试与生产轮询的是同一个句柄，**因此无法观测到生产接线被切断**。**M1**（把 `jobs.rs:210` 改成自建 `Arc::new(AtomicBool::new(false))`）→ **存活，1718 全绿 EXIT=0**。M1c（`:177-178` 置 `registered=false; window=false`）被杀。**三处 `host/executor.rs` 轮询（`:40` 页读前 / `:103` 校验前 / `:124` 写入前）在任何测试中都没被 `cancel_job` 置位的 flag 走到过。**
- **修法裁定：删桥接，不补测试。** 这正是从 runtime 轨 M1 与 data-sync M1 两个相反方向各验证一次的铁律——*断言共享句柄的测试看不见被切断的桥*。而 `jobs.rs:168-175` **自己写明了退出条件**："Once the runtime carries a cancel watcher for running stages, the host flag in (1) becomes redundant and can be dropped"——**该条件已由 `105494c14` 满足**。已核实 `executor.rs:38`、`:81`、`state.rs:55`、`jobs.rs:168` 四处注释的共同前提「runtime `CancelToken` 只在 stage 起点触发」**现已为假**；crate 侧 `apply.rs:267`/`:297`/`:391`/`:442` 本就正确轮询 `cancel.is_cancelled()`，host 的 flag 是**建在已被推翻的前提上的第二套并行机制**。故在修复轮中**删除 `state::CANCEL` 存储与 `open_job` 的 flag 铸造、把三处轮询改读内核 token**（§9「同一请求自始至终只走一种管理器」同样支持），并**新写一条真跑分页阶段、中途调 `cancel_job`、只断言可观测结果**的用例。**CM-44「Job 存在前落下的取消」原靠 `seed_cancelled` 从 window flag 拷贝，删除后必须改为播种内核记录，且必须补测试钉住，否则会静默回归**——这是本次重构最高风险点。
- D2（低）`exec.rs:144-147` 死结构 `ValidatedSyncContext{target_driver,target_handle}` 字段从未读，是本轨唯一新增告警（30→31）。D3（info）自述 diff「25 文件 +3260」与实际「44 文件 +6985」不符。**D4（info，非缺陷）** planId 单次消费有**三处**冗余机制（`host/selection.rs:50` 认领 / `host/state.rs:187` `take_selection` / `sync/jobs.rs:153` 幂等键+`jobs.rs:337-343` 回放闸）+ 内核权威闸 `repository.rs:268-272`；M4/M4b 存活、M4c 内核侧被杀、M4c2 三处全废被杀。Tester **已撤回**其 C1 项指控。D5（info）`select_by_key_sql` 在 `pk_columns` 为空时退化成**全表扫描**，应在入口显式报错。
- **Tester 裁定的非缺陷**（勿再翻案）：Step-6「0 行 diff」自相矛盾**不成立**（`fs::rename` 修复在 `tests.rs`，20 轮压测 20/20 过）；`resolve_db_name` 以 `.unwrap_or("")` 结尾因该函数**永不失败**，不违反 §8；§8 经 `pairing::require_data_sync_family` + `driver_type()` 相等 + `is_self_sync` **间接**实现，全仓 `backend_scope` 字面命中 0 是命名差异不是缺失；ChangeSet 仅内存 + mock-driver-only，作为本阶段约束接受。
- **仍被杀、证明是真覆盖的变异**：`apply.rs:267` 取消检查、`apply.rs:287` option 行冲突、`apply.rs:152-153` 端点关闭（3 例）、`host/mod.rs:731` PK 谓词（3 例）。
- **合并已完成**：修复分支 `feature/p5-data-sync-r2` 自 `e28ff3938` 合入 `36d2682c6`，merge commit **`25d336ece`**。**冲突恰为 Tester 预言的 1 处**：`src-tauri/Cargo.toml:24-27` 纯注释（两轨各写了一条同义 JobRuntime 注释），已合成一条。**`cargo metadata --no-deps` META_EXIT=0、无重复依赖键**（该地雷曾致 EXIT=101 无文本冲突，务必先跑它）。`progress.md` 已随合并销毁。
- ⚠️ **新 worktree 的第一道门禁不是代码错误**：首跑宿主 EXIT=101，根因 `resource path 'resources/builtin-ep' doesn't exist`——**codegen 产物缺失**。跑 `node scripts/generate-builtin-locales.mjs` + `node scripts/resolve-drivers.mjs --codegen-only --drivers=basic` 后即恢复。**勿误判为数据同步合并失败。**
- 合并基线门禁全绿（首尾 HEAD/tree 一致）：宿主 **`1741 passed; 0 failed; 6 ignored`**（= 1733 + 8，与 Tester 的 "+8 精确"吻合）/ `datazen-data-sync` **200** / `datazen-runtime` **842**。
- 旧交付树 `.worktrees/datazen-p5-data-sync` 与分支 `feature/p5-data-sync` 已删除（`36d2682c6` 确为 `feature/p5-data-sync-r2` 的祖先，已完全吸收）；顺带清掉游离 worktree `/private/tmp/ta-p5ds-r1.FNG9`。
- 修复轮已派 coder `fa475e95`（分支 `feature/p5-data-sync-r2`，`CARGO_TARGET_DIR=/tmp/p5-ds-r2-fix-target`），含 D1+收敛、D2、D5，并要求补 **4 个必杀变异**（原 M1 复现、删一个检查点、破 CM-44 播种、破空 PK 守卫）+ 复验 4 个仍被杀变异。
- **方法论沉淀（本阶段最贵的一条）**：**当一条桥接在架构上已决定退役，就删掉它，而不是写测试去覆盖它。** 两次相反方向的验证（runtime M1 杀死了 209 条基线、数据-sync M1 存活于 1718 条全绿）指向同一条规律：门禁全绿不等于有覆盖，**变异才是判决书**。

### 🔴 跨轨事实：`--drivers=basic` 与 `--drivers=all` **不是同一个 DB_REGISTRY**（所有门禁判读的前提）

- 由硬化轨实例 `7c83fa5b` 报出、**已独立核实** `scripts/new-feature-worktree.sh:41-45` 与 `:512-519`：宿主 `src/lib/databaseTypes.ts:20-22` 直接合并 `generated.ts` 的 `DRIVER_DB_ENTRIES`，而 `generated.ts` 是 **gitignored 本地产物** ⇒ **驱动集一变，同一份宿主单测读到的注册表就变，绿/红与 CI 不可比**。basic = postgres/mysql/sqlite/redis；all 另加 sqlserver/mongodb/clickhouse 等全部 path 驱动。
- 脚本自带**实测反例**：`tester_tunnelValidationMatrix.test.tsx` 在 basic 下 **29/29 全绿**，同一 worktree 换成 all 后 `:154` 的 `expect(covered).toEqual(registered)` **直接变红**。该脚本历史上把 `--drivers=basic` 写死且不告知调用方，正是「本地绿、CI 红」无从解释的成因；现由 `NEW_WT_DRIVERS` 环境变量显式化。
- **同段明确：`pnpm typecheck` 不受驱动集影响**（basic 下实测干净）⇒ ① typecheck 通过**不能**反推驱动集已对齐；② typecheck 报「找不到模块 / TS7006 级联」是 **codegen 缺失**，不是驱动集不同，**两者必须分开归因**，否则会把环境问题误判成代码回归。
- **铺法**（产物全 gitignored，补完 `git status --porcelain` 必须仍为空）：`resolve-drivers.mjs --codegen-only --drivers=all` + `generate-builtin-locales.mjs` + `generate-menu-labels.mjs` + `mkdir src-tauri/resources/builtin-ep`。
- ⚠️ **对既有 P5 数字的追认**：集成分支合并门禁、各轨 Tester 的宿主单测数字，**都是在 basic 语义下取得的**，其结论只能限定在 basic 内，不能用来推断 CI 会不会红。上文「1741 = 1733 + 8」的算术**在同一驱动集内成立**（同集内前后比较），但该数字**不得跨驱动集引用**。Wave-R 最终回归必须统一按 `--drivers=all` 重铺后重跑。
- 已广播给在飞的两轨（`7c83fa5b`、`fa475e95`），并要求在报告里标注所用驱动集、且**基线与该次变异必须同一驱动集**——否则「变红/存活」的判决同样失效。

### 📦 交付待验：p5-data-sync R2 修复 → HEAD `f3f4ec3e0`（Tester `5324a984` 复验中，基线 `25d336ece`）

R1 被否的根因是**测试断言在共享 `Arc<AtomicBool>` 这个对象上**，而不是断言可观测行为；且一个「切断内核→宿主取消管线」的变异存活。本轮的核心命题因此是 **删掉那座桥，而不是补一个覆盖它的测试**——这正是我此前定的「桥被架构性退役时就删，不要为它写测试」这条准则的首次实证。

- **核心改动**：宿主侧 `CANCEL` / `CancelEntry` / `cancel_flag` / `ctx_of` / `request_cancel` / `is_cancelled` 整体删除，`seed_cancelled` 的窗口 flag 副本删除；三个 executor 检查点改轮询**内核** flag（`CancelToken` 按引用经端口 trait 穿到 `table_reader`/`target_executor`，`cancel.flag()` 恰好两处）。CM-44 由**给内核 Job 记录播种**保住（`cancel_job` 保留窗口注册表 **并** 调 `repository().request_cancel`，返回 `window || kernel`；`drive` 在 `accept` 之后、`runtime.run` 之前重播 `cancel_requested`，使 `run` 走 `confirm_cancelled_not_started(..., NotStarted)` 分支）。测试假件 `tests/support/{page_source,executor,host}.rs` 各自私铸 flag 的问题也一并修掉。
- ⚠️ **本轮最高风险项：`submit_prepare` / `submit_apply` 的参数从 `Option<Arc<AtomicBool>>` 改成 `Option<String>`。** 这两个函数若带 `#[tauri::command]` 标注，就是 **IPC 可见的签名变更**，任何前端调用方漏改都会在运行期炸。已列为 Tester 第一顺位核查项，并要求在非命令的情况下给出证据改查「是否还有别的生产调用方在传已删除的 `Arc<AtomicBool>`」。
- 🔶 **M3（CM-44 前置 Job 取消播种）第一轮存活，交付方如实上报并补测试后才杀死。** 这是本项目第一次出现「交付方主动把自己不干净的变异结果写进报告」。根因也讲得清楚：当时唯一的 CM-44 前置用例打在**遗留** `execute_data_sync_impl` 语句路径上，根本不进 `drive`，因而观察不到「窗口 id → 内核 Job 记录」这一跳。Tester 需在 `c56b2e17`（修前）与 `f3f4ec3e0`（修后）两端各跑一次以复现这个存活→被杀的弧线，并判定新用例是否非空转。
- 🟡 **交付方与我给的基线数字对不上，三处**，已要求独立裁定而非采信：warning 数 **32 vs 我说的 30**（并主张 `--drivers=all` 下修前基线是 **34** 而非早前轮次引用的 31，且恰好减 2 增 0）；R3 影响面 **1 vs 我预测的 3**；R4 影响面 **6 vs 我预测的 3**。这三条都要求 Tester 自己在 `25d336ece` 拉基线实测。
- 🟡 **R4 曾挂死整个套件**（摘掉 PK 谓词后 apply 在任何语句前就拒绝，nothing reached the gate，`tokio::join!` 永久 park）。交付方的处置是**改测试而非改变异**：把等待上界钉在 20 s 并对其做断言。这条必须裁决——「有界等待」既能把挂死变成红灯（好），也可能把「走错路径所以永远不来」一并吞掉（坏）。要求 Tester 判定它究竟是在断言「等待的东西确实在界内发生了」，还是仅仅「不等了」。
- 交付方亦如实登记：本轮新契约用例走的是 gated mock 驱动，**不是真实数据库**，因此**未**关掉「§10 无真实 DB 验证」这一既有限制。
- 门禁自报（`--drivers=all`）：host `--lib` 1745、data-sync 201、runtime **842**、`cargo check` EXIT=0、typecheck `error TS`=0。runtime 为 842 而非 843，原因是本分支不含 cancel-hardening 那次合并——已要求 Tester **验证**该解释而非默认接受。

### ✅ 已合并：p5-cancel-hardening `f440acf91` → 集成分支 `7ec0ca315`（Tester `aacf7430` TEST_PASSED，零缺陷）

- Tester 判**四项全过、零存活变异、零缺陷**，并**自行加跑一条我没要求的对照 CTRL-2b**：在 `e16b43415` 上**只加 D-B 的 +32 行测试缝、不加新测试**，同一 `terminal` 变异**仍然存活**。这一条把归因锁死为「CTRL-2→MUT-2 的差异**只能**来自新增的 D-B 用例」，而非来自测试缝本身。三个存活变异因此是**对照臂，非逃逸**。
- Tester **独立复核了「0.93s 绿是真绿」这一辩解并判其成立**：`PATIENCE`（`job_cancel_watch.rs:51`）只作 deadline 上限，从无 ≥5s 等待；最大的**无条件** sleep 是 `QUIET_WINDOW=400ms`。算术精确复现：8 条 ≈0.92s；HEAD 多一条含 4×400ms 串行 sleep ⇒ 1.62s。
- **rustfmt 漂移由 Tester 自行在基线 `105494c14` 复核**，非采信声明：基线 23 处、HEAD 23 处，**逐处一一对应**，位移量精确等于各提交插入行数（`kernel_cancel.rs` 全 `+27` = D-E；`repository.rs` 后三处全 `+32` = D-B）。两个**新增文件 0 漂移**。声明成立，非缺陷。
- Tester 主动纠正了三处**它没能复现的**内容并明确标注为推断而非测量：① `right: 18` vs `17` 的调度抖动；② 交付表把 MUT-1 标为「post-D-A」不准确（实际在 `0627bdb0c`）；③ **它没有直接实测基线 host `--lib`=1733**，改用结构论证（本次 diff 零 `src-tauri/` 文件）并如实声明。乱码经实测为**仅在两条 commit 正文**、9 个文件内容 `U+FFFD` 计数 **0** ⇒ 判装饰性、非缺陷。
- 合并后我**独立复跑**五门（driver set `all` = 17 个驱动 crate、含仅 all 才有的 `victoriametrics`；`drivers-registry.json` sha 与 main 逐字节一致）：`cargo metadata --no-deps` EXIT=0 / stderr 空（**无 Cargo 重复键**）；runtime 33 条结果行、**843 passed / 0 failed**；data-transfer **213 passed**；host `--lib` **1733 passed / 6 ignored**；`cargo check` EXIT=0；typecheck `error TS`=**0**。门禁前后 HEAD 与工作区均未变（`7ec0ca315`，`--porcelain` 空）。合并无冲突，`hub.md` 未被卷入合并 stat。
- 收尾已执行：该分支**从未创建 `progress.md`**（feature 分支上的 `hub.md` 来自分支基点，非本轨交付物）；worktree `.worktrees/datazen-p5-cancel-hardening` 已移除，`git worktree prune` 已跑，`feature/p5-cancel-hardening` 已在**确认血缘后**删除。**`main` 未被触碰。**

### ⚠️ Wave-R 预告：`main` 正在并发改 `packages/runtime`，但冲突面比想象小（已实测）

- `main` 已推进到 `ec0444e8c`（`chore(runtime): 消掉 35 条 warning…`），**领先 P5 集成 79 个提交**，merge-base `ec857bdd26`。它改了 **60 个** P5 语义范围内的文件——但**全部落在** `packages/runtime/src/{application,connection,registry,resource,gateway}/`。
- P5 的 runtime 改动全部在 `packages/runtime/src/job/` 与 `packages/runtime/tests/` ⇒ **与 main 那 60 个文件零重叠**。这是「按文件冲突面分波」这条分轨规则的实际收益。
- **双向都改的文件只有 8 个**：`Cargo.lock`、`hub.md`、`src-tauri/Cargo.toml`（已知，注释级）、`packages/runtime/src/lib.rs`、`src-tauri/src/bootstrap/run.rs`、`src-tauri/src/bootstrap/app_state.rs`、`packages/backend-client/src/{client.ts,types/jobs.ts}`。
- ⚠️ 值得盯的是 **`packages/backend-client/src/types/jobs.ts` 与 `client.ts`**：main 侧也在动 jobs 类型，若语义重叠则最终合并需人工裁定，而非机械取一侧。
- 结论：Wave-R 全量回归必须**以当前 main 为新基线重跑**，不能沿用任何旧计数。

### 📦 交付待验：p5-cancel-hardening → HEAD `f440acf91`（Tester `aacf7430` 独立复验中）

- 分支 `feature/p5-cancel-hardening`，基线 `105494c14`，tree `fe050cffaeb4c5a94a75926302d34dd2b121b807`，工作区 clean（门禁前后各记一次 HEAD/tree/status）。9 文件 +436/−17，**`.ts/.tsx` 改动 = 0**，未触任何禁区。
- 提交（每项一提交，未使用 `--amend`）：`c3a509ad0` D-C → `e16b43415` D-A → `0627bdb0c` D-B → `ea53469be` D-E → `d4ebea68d` 补 EOF 换行 → `f440acf91` 补 CTRL-2 实测出处。
- **交付方采用「对照变异」取证，本阶段首次**：同一变异分别在**修复前**与**修复后**的提交上各跑一次。
  - **CTRL-1** @`c3a509ad0` 删 `get_calls.fetch_add` → `ok. 8 passed; 0 failed` EXIT=0 **存活**；**MUT-1** @修复后同一变异 → `7 passed; 2 failed` EXIT=101 **被杀**（`poll_fault.rs:140`、`terminal_exit.rs:43`，正是 D-A 加的两处活性前置）。
  - **CTRL-2** @`e16b43415` 把 `terminal:` 改 `false` → `ok. 8 passed; 0 failed` EXIT=0 **存活**；**MUT-2** @修复后同一变异 → `8 passed; 1 failed` EXIT=101 **被杀**，且**只** `terminal_exit.rs:75` 失败（`left: 25, right: 18`，即冻结计数断言）。同一变异下 CTRL-2 与 MUT-2 的差异**只能**归因于新增的 D-B 用例。
  - CTRL-2 同时**废掉了 `terminal_exit.rs` 文档注释里那句原本无凭据的「全部用例依旧全绿」**，其出处已由 `f440acf91` 写进代码。
  - ⚠️ CTRL-1 的 `0.93s` 绿是**可疑时长**（同族用例的静止基线约 1.64s），coder 的辩解是「绿色 0.93s ≠ 陈旧产物，因为观察到从零 `Compiling datazen-runtime`」。**Tester 须自行判断该辩解是否成立，不得凭声明采信。**
- 门禁（driver set = `all`）：runtime 843（842 + D-B 1 条）/ data-transfer 213（211 + D-C 2 条）/ `datazen --lib` **恰好 1733**（= 基线，说明未越界改动宿主）/ `cargo check` EXIT=0 / typecheck EXIT=0 且 `error TS` = 0。
- **D-B 判据已按上文勘误改正**：coder 未迁就我写反的括号，断言改为**合取式**——`active_cancel_watchers() >= 1`（句柄未 Drop ⇒ 非 abort 回收）+ 同窗口 `get_calls()` 冻结（轮询自行停止）。缺任一半即为空洞断言，Tester 须逐半核。
- 交付方主动声明且经我裁定**均不构成缺陷**：① 两条 commit **正文**乱码（`e16b43415`、`ea53469be` 含 U+FFFD，diff 内容正确；修复需 `--amend`，故不修）——纯装饰；② 既存 rustfmt 漂移（`poll_fault.rs:68`、`repository.rs:26/33/61/348`、`runtime.rs:123/130`、`kernel_cancel.rs:45/53` 等），coder 已用基线 `105494c14` 逐文件 `rustfmt --edition 2021 --check` 比对证明**非本轨引入**，且基线本就不是 `cargo fmt --check` 干净。
- 两条**环境陷阱**已入册（会伪装成代码缺陷）：worktree 未铺 gitignored codegen ⇒ `datazen --lib`/`check` 以 EXIT=101 `resource path 'resources/builtin-ep' doesn't exist` 死、typecheck 报约 27 个 `error TS`；`/tmp` 磁盘耗尽（926 GiB 卷上只剩 985 MiB）⇒ `rustc-LLVM ERROR: IO failure on output stream: No space left on device`。均**不是**回归。coder 只清理了本轨自己的陈旧 target 目录，未动他轨；`/tmp` 现 22 GiB 空闲。
- ⚠️ **台账缺口（如实登记，不臆造）**：D-D、D-F、D-G 三者的描述在本轨分支的 `hub.md` 中不存在，且原始任务书文本在两次网络中断中丢失——**协调者手上有 D-D 与 8 条 E2E 缺口的原文（已随 Tester 任务书下发），但 D-F、D-G 的原文确实无法提供**，Tester 应如实回报该缺口，**不得编造描述**。

- 我在两份任务书里都写了括号「若看护者已退出，计数会是 0」——**这句是错的**，硬化轨实例 `7c83fa5b` 指出后已逐行核实成立：`packages/runtime/src/job/runtime.rs:318` `spawn` 做 `live.fetch_add(1)`，**只有** `impl Drop for CancelWatch`（`:335`、`:342`）才 `fetch_sub(1)`；而 `run_stage_watched`（`:280`）在 `:288` 以 `let watch = CancelWatch::spawn(...)` **把句柄留在 dispatch 栈上，并未 move 进 tokio 任务**。故 `active_cancel_watchers() == 1` 只表示 **dispatch 仍持有句柄**，轮询循环自行 `terminal => return` 退出时计数**仍是 1**。
- 唯一正确的判据是**组合式**：在阶段仍被 hold 的同一窗口内，`active_cancel_watchers() >= 1` 证明句柄未被 Drop ⇒ **不是 abort 回收**（`runtime.rs:275` 注释亦确认 Drop 正是「只 abort 不 await」那条回收路径）；同时 `get_calls()` **冻结** ⇒ 轮询**自己停了**。二者缺一即为空洞断言。
- **教训**：计数器的语义必须由**谁持有、在哪一步增减**反推，不能由名字猜。`active_*_watchers` 命名上像在数"活着的任务"，实际数的是"未 Drop 的句柄"。

### 🔍 Tester `5324a984` 复验 ds-r2 中途进度（已独立裁定部分，尚未给最终判定）

**🔶 勘误一：warning 基线是 34，不是 31，更不是任务书写的 30。**
Tester 在 `--drivers=all` 下自拉基线 `25d336ece` 实测 **34 → HEAD 32**，且做的是 **warning 集合 diff 而非计数**：基线确有、HEAD 确已消失的两条为 `fields target_driver and target_handle are never read`、`function is_cancelled is never used`，**引入集合为空**。
**我引用的 31 与写的 30 均错，同一坑根**：都是 `--drivers=basic` 下的旧测量。**再次印证 `--drivers=basic` 与 `--drivers=all` 的宿主 `DB_REGISTRY` 不是同一注册表。**
⚠️ 方法论：只报「计数 34→32」**不足以**支撑「增 0 减 2」——计数相同而集合不同是可能的。Tester 主动做集合 diff 是对的，以后此类基线争议一律要求给集合。

**🔶 勘误二：新增测试数与测试数增量精确相等，顺带解掉 842 vs 843 之谜。**
host `--lib` 1741 → **1745**（+4 = `select.rs` 3 条 + `jobs_cancel_contract.rs` 2 条 − 删除的 `jobs_contract` 1 条）；data-sync 200 → **201**（+1 = `cm44_batch_cancel.rs` 1 条）；runtime **842 → 842（+0）**，且 diff 确实不含 `packages/runtime/**`。
⇒ 集成侧 843 与 ds-r2 侧 842 的差，不是回归也不是丢测试，**是 hardening 轨 D-B 恰好多出的那 1 条**。合并时以集成分支自身重新基线化。

**✅ 最高风险项关闭：`Option<Arc<AtomicBool>>` → `Option<String>` 不是 IPC 可见变更。**
`submit_prepare`（`jobs.rs:85`）与 `submit_apply`（`:125`）是 `pub(crate) async fn`，**无** `#[tauri::command]`；`commands/sync/` 下 17 处 `tauri::command` 全在 `mod.rs`，真正对外的取消命令是 `mod.rs:279`。diff 不含任何 `.ts/.tsx`。
Tester 另做了我要求的**替代核查**（是否有别的生产调用方在传已删的 `Arc<AtomicBool>`）：生产侧唯一存活的 `AtomicBool` 是 `services/job_registry.rs:9` 的**accept 之前**的信箱，不是被 stage 轮询的桥；`host/state.rs` 零 `AtomicBool`，`host/recording.rs` 只按引用透传。

**✅ R4「钉 20s 上界」的裁定：断言了等待物确实发生，不是「不再等」。**
`jobs_cancel_contract.rs:398-402` 的超时被转成 `assert!(in_flight, "… nothing reached the gate within {PATIENCE:?}")`——是**会失败**的断言，不是静默吞掉。且第二个用例开头即 `gate.open()`，是把挂死**设计掉**而不只是设上界。
精度补充：文件里有两个 duration 常量，但绿色路径上**只有 500ms 的 `SETTLE` 必然消耗**（= 内核 50ms `CANCEL_POLL_INTERVAL` × 10），`PATIENCE` 只在失败路径上耗时 ⇒ CI 成本约 500ms 而非 20s。

**已复现变异（每个全新空 `CARGO_TARGET_DIR`、一次一个、用完即删，日志均含 563 条 `Compiling`）**：M1 / M2 / M3b / M4 **全部 KILLED**，影响面均 1。R4、R1/R2/R3 的持续被杀确认、以及 M3 在 `c56b2e17` 的存活臂，尚未跑完。

**⚠️ G5 的偏离待消解**：Tester 把 `node_modules` 做成 symlink，致 pnpm 运行器以 EXIT=1 `workspace hoist directory is not a real directory` 失败，遂改跑脚本展开的 4 条等价命令（全 EXIT=0，`error TS`=0）。**已要求用 `pnpm --config.verify-deps-before-run=false typecheck` 重试**，以取得字面绿灯。类型检查不受驱动集影响，故此处不涉及 basic/all 之争。

**❌ 一条被推翻的"流程缺陷"指控（登记以免二次误采）**：硬化轨实例报「D-A/D-B/D-E 提交作者是 `probe`（验证方身份），违反提交方/验证方不得混同，建议改流程」。**实测为误判**：`git log --format='%an <%ae>'` 显示**本仓库所有提交**（含 main 的 `adb0fd045`、`eeb78e591`）作者均为 `probe <probe@local>`，那是本机全局 git config，与提交者身份无关。**不采纳该建议。**

**⚠️ 8 条 E2E 缺口只活在台账里——本轮新增的 Wave-R 硬任务。**
硬化轨任务书要求「只列不建」，故这 8 条未变成代码/测试事实；`hub.md` 不在 main、验收时按纪律**必须销毁**。AGENTS.md 要求缺陷结论最终必须能从代码与测试读出，否则即视为没做完。
⇒ **决定**：Wave-R 收口时，必须把「当前尚未被证明的行为」改写为 `docs/architecture/platform/data-migration-jobs.md` 中一节**验证边界**（陈述"已实现"与"尚未证明"的边界），而不是留成 Bug List——后者是文档纪律明令禁止的新增文档类型。清单原文：UI 真实取消延迟、50ms 常量实测、`StructureAndData` 三阶段**阶段间**取消、真实驱动往返、取消后续跑不重复提交、重复点取消、端到端读故障注入、看门狗在真实阶段内 panic 下的存活性。

**⚠️ 台账缺口状态未变**：D-F、D-G 的原始描述仍无法提供（两次网络中断丢失）。**不臆造**，Tester 如实回报即可。

**📌 我的失误（登记）**：Tester 汇报与我的孤儿清理动作同时发生，我误把它仍在使用的 4 棵 `/tmp/p5ds-vfy-*` 临时 worktree 当作残留 `worktree remove --force` 删掉。target 目录本已自行销毁、冷编译成本照付、日志未受影响，代价可控，但**在子代理仍在报活时清理其工作区是不该发生的**。教训：清理前先确认该实例已终止。

**📌 `main` 再次推进**：`ec0444e8c` → **`adb0fd045`**，走在另一批「注释规格引用清理」提交上。Wave-R 基线需再次重取。

### 🔴 新增阻塞缺陷：端点重叠把两个不同物理库当成同一个端点（缺口 k，已开修复轨）

**来源**：frontend-cutover coder `fd466b08` 报出，我**独立逐行复核属实**。

- `packages/runtime/src/job/budget.rs` 的 `detect_endpoint_overlap` 以 **`(service_key, object)`** 为键：先收集 `SourceReader` 的 `objects` 进 `reads`，再逐个查 `TargetWriter` 是否命中，命中即 `Err(EndpointOverlap)`；另有 `any_unprovable && has_writer` 的 fail-closed 分支。
- `src-tauri/src/commands/data_transfer/job_api/runtime.rs:40` `const TRANSFER_SERVICE_KEY: &str = "data-transfer";`，且 `endpoint_refs()`（`:150-170`）给 **reader 与 writer 共用该常量**。
- ⇒ 源表 `public.users` → 目标表 `public.users`，**即便发生在两个不同物理库**，也必然命中 `("data-transfer","public.users")` 而被判"既读又写"。

**🔴 更深一层（协调者顺带查出，coder 未发现）**：同一 `endpoint_refs()` 里 `connection_id` 被硬编码为 `ConnectionId::new("local-source")` / `ConnectionId::new("local-target")`，**与用户实际选择的连接毫无关系**。而 `EndpointRef` **带有** `connection_id` 字段，`detect_endpoint_overlap` **根本未使用它**，只看 `service_key`。⇒ **端点身份整个是伪造的**，探测器在结构上就没有能力区分两个物理端点。仅改 `service_key` 等于把假身份换成另一个假身份。

**判定**：这是**新路径对 legacy 的功能性倒退**——同名前缀跨库复制是最常见的迁移操作之一，legacy `execute_data_transfer` 照跑，Job 路径被拒。coder 在前端加的 fail-closed 拦截是**止血，不是修复**，且**UX 代价真实存在**：合法操作在前端被挡死。

**⇒ 已开修复轨 `p5-endpoint-overlap`**：分支 `feature/p5-endpoint-overlap`，基线 `4b782750dd`，coder `71d90884`。与 frontend-cutover **文件零重叠**（`packages/runtime/**` + `src-tauri/src/commands/data_transfer/**` vs `src/**` + `e2e/**`），并行跑。
规格（总纲优先于细节）：① **不得比 legacy 更严**——legacy 能接受的计划 Job 路径必须接受；② 不同物理端点 + 相同对象名 → 放行；③ 同一物理端点既读又写同一对象 → **继续拒绝**（保留安全属性）；④ 端点身份必须由**真实连接身份派生**，且已明确排除"把常量拆成 `data-transfer:source`/`:target`"这种假修法（会让不同 Job 的不同物理端点共用键，原子预留误冲突或漏冲突）；⑤ 保留 §6.2 全有或全无预算预留；⑥ SQL-file 无 writer 端点路径不变；⑦ fail-closed 只保留在真正无法证明身份处。

⚠️ **后续必须项**：后端按真实端点区分后，coder 的前端谓词**只比表名**，`A库.users → B库.users` 仍会误报冲突 ⇒ **前端拦截在后端修好后仍然是错的**，需一次前端对齐（届时要比较连接身份，而非只比表名）。已要求 frontend coder 在报告里显式登记，**不得默默留着**。

### ✅ Tester `5324a984` 终裁 data-sync R2：**TEST_PASSED**（独立复验，10 变异零存活）

**溯源**：交付树开档/收档 HEAD=`f3f4ec3e0`、tree=`02346f346`、porcelain 空，**首尾一致**；Tester 全程未在交付树内改动，变异全在 detached 临时树内，收尾已全部清理。

**变异表（10 有效 + 1 无效）**：M1 KILLED / M2 KILLED / M3b KILLED / M4 KILLED / R1 KILLED / R2 KILLED / **R3b KILLED** / R4 KILLED / **M3-pre SURVIVED @ `c56b2e17`** / **R3a 无效**。
- **头条是 M3-pre 的「修前存活 / 修后被杀」对照**：同一处 pre-job cancel 结转被删，在 `c56b2e17` 上 EXIT=0、**1744 全绿**（无任何测试发现）；在 `f3f4ec3e0` 上 EXIT=101、被新用例 `a_cancel_that_arrives_before_the_job_exists_still_stops_the_apply` 杀死。⇒ **新用例非空转**，实证成立。
- **Tester 自曝两处陷阱并作废其自身结果**：① R3 首个变异 `Ok(_) if false` **编译不过（E0004）**，Tester 明确判定「编译不过的变异不是被杀」，丢弃后重造语义等价版本；② 我清理临时树时它正在后台跑 R4，`cd` 失败导致**变异脚本从未执行**，cargo 却因 `&&`/`;` 断裂在主仓库跑了未变异代码报 2287 passed——该次 R4 **作废**，重建三棵临时树（registry 逐字节校验通过）后重跑。**该纪律正确，保持。**
- 反陈旧证据：5 个 host 变异各 563 条从零 `Compiling`，3 个 data-sync 变异各 189 条；无一次出现「0.92s 绿」形状，绿色运行均在 12–25s。
- 5 个 host 变异的 `--lib` 只产出**一条** `test result:` 行（单二进制）⇒ M1/M2/M3b/M4/R4 的失败清单**是完整的，不是下界**（此豁免仅对本次成立，不可推广）。

**门禁 @ `f3f4ec3e0`，driver set `--drivers=all`**：host `--lib` EXIT=0 **1745 passed / 0 failed / 6 ignored**；data-sync EXIT=0 **201**；runtime EXIT=0 **842**；`cargo check -p datazen` EXIT=0 **32 warnings**；typecheck **EXIT=0 / `error TS` 0**（用我给的 `--config.verify-deps-before-run=false` 拿到**字面绿灯**，此前 4 条等价命令展开的偏离**已消除**；依据是脚本定义非记忆——pnpm 自回显 `$ pnpm check:restore-guards && tsc --noEmit && pnpm typecheck:scripts && pnpm typecheck:pack-ep`）。
- Tester 诚实标注：G2/G3/G4 复用了 G1 的 target 目录（同 commit 同树，合法复用）。

#### 🔴 Tester 推翻了我四处 —— 我的说法作废，以 Tester 为准

| 我的说法 | 实测 | 裁定 |
| --- | --- | --- |
| warning **30** | 基线 **34** → HEAD **32** | **我错**。且 Tester 做的是 **warning 集合 diff 而非计数**，`base-check.log:763/:773` 两条在基线存在、HEAD 消失，**引入集合为空** |
| R3 影响面 **3** | **1** | **我错**（R3b 只杀 `cm43_review_conflict_…`） |
| R4 影响面 **3** | **6** | **我错**（6 个，含 `exec::tests::test_tester_plan_preview_and_execution_use_the_paged_path` 等） |
| `cm46_pipeline.rs` / `execute_bounded_table` / `kernel_cancel.rs` 属 **data-sync**，且 `cm46_pipeline.rs` 单测用手写 `Arc<AtomicBool>` | 三者**全在 `packages/data-transfer/`**；`cm46_pipeline.rs:61` 传入的是 **`cancelled: None`**，根本没有手写 flag | **我错，crate 与细节都错** |

**派生更正**：所谓「`cm46_pipeline.rs` 用手写 `Arc<AtomicBool>` 测 `execute_bounded_table`」这条「已接受限制」**整条撤销**。**D10（生产路径超预算测试）正确归属在 data-transfer 侧**，且 data-sync 的 `packages/data-sync/tests/` 下**根本不存在任何 budget/over-budget 测试文件** ⇒ D10 在两个 crate 中都仍敞开。
**`cancel.flag()` 实际行号 `:620/:682`**（非 `:621/:683`）。**`host/recording.rs` 是生产文件非测试辅助**（零 `#[cfg(test)]`，由 `jobs.rs:242` `recording_host()` 构造，被 `submit_prepare`/`submit_apply` 两条生产路径调用）。
**变异点 `host/mod.rs:731` 已失效**：HEAD 该文件 721 行，该点已被 `select.rs` 取代，后续按此点复现会得到「没有这一行」。
**§8 裁定**：`PATIENCE` 超时被转成**会失败的断言**（`jobs_cancel_contract.rs:398-402`），是「断言等待物确实发生」而非「不再等」；绿色路径**只有 500ms `SETTLE` 必然消耗**（50ms `CANCEL_POLL_INTERVAL` × 10），`PATIENCE` 仅失败路径耗时。

**⚠️ Tester 自己点明的未检验假设（必须入册）**：`CANCEL_POLL_INTERVAL`（`runtime.rs:41`，50ms）**始终只被注释引用、从未被测量**，`SETTLE` 只是隐式依赖它 ⇒ 本次绿灯依赖一个**未检验的常量**。且新测试的**取消路径**不可注入 `Unknown`（handler 无该终态）。
**Tester 正确拒绝的两项**：① `GatedDriver` 是 mock，其契约测试**不构成**「无真实数据库验证」这一限制的关闭；② diff 触及 **0** 个 `e2e/` 文件 ⇒ 12 项注册 E2E/WDIO 缺口**无一关闭**，8 项项目级缺口亦全敞开。

**MEDIUM（流程，非代码）**：`hub.md` 在 HEAD **被跟踪（35,858 字节）**而 `main` 没有 ⇒ 按现状合并会把 AGENTS.md 明令禁止的台账带入 main。**收口时必须删除**（本就是既定收尾步骤）。`progress.md` 确认在 HEAD **不存在**。

**⇒ 已合并**：`feature/p5-data-sync-r2` → `codex/p5-integration`，merge `31af2b8fb8`，无冲突。合并后第一道 sanity `cargo metadata --no-deps` EXIT=0（25 packages / 17 `datazen-driver-*` / `victoriametrics` 存在）；`drivers-registry.json` sha256 与 main **逐字节相同**。

**📌 另一条待澄清的阻塞级疑点**：frontend coder 自列缺口 (c)「apply 阻塞到终态故无可寻址 jobId」。若属实，则 `apply` 返回前前端拿不到 `jobId` ⇒ **取消按钮无法寻址**、无中间进度 ⇒ Job 路径 UX **实质劣于 legacy**。这比缺 `list_jobs` 更要紧，直接决定本次切主路径是否真的可用。已要求给出**实测证据**（非读码推断）并按阻塞级处理。
