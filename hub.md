# P5 Hub Ledger（集成分支 `codex/p5-integration`，基线 `ec857bdd2`）

**本文件只记开发进度**：轨道状态、合入 commit、跨轨裁定、未闭口项。缺陷的行级证据、变异表、验收逐条结论**不入本文件**——已合入的缺陷结论必须是代码与测试里的事实；未闭口项必须迁到 `docs/architecture/platform/data-migration-jobs.md`。

用户范围：P5 = JobRuntime + 数据迁移三件套。**所有轨道只合并到 `codex/p5-integration`，整个 P5 完成前不合并 main。**

## 轨道状态

| Track | 分支 | Status |
| --- | --- | --- |
| `p5-job-core` JobRuntime/契约/预算 | feature/p5-job-core | ✅ MERGED `bb2cc5606`（3 轮验收） |
| `p5-domain-extract` 三件套抽 packages/* | feature/p5-domain-extract | ✅ MERGED `97885d6e0` |
| `p5-schema-diff` handler | feature/p5-schema-diff | ✅ MERGED `4ba769fc9`（2 轮验收） |
| `p5-client` 任务中心/订阅/回执 | feature/p5-client | ✅ MERGED `5f46a0262` |
| `p5-i18n` migrationJob key | feature/p5-i18n | ✅ MERGED `0db23e6d0` |
| `p5-data-transfer` handler | feature/p5-data-transfer | ✅ MERGED `3e28d49b8`（2 轮验收） |
| `p5-runtime-cancel-watcher` 内核级 stage 内取消 | feature/p5-runtime-cancel-watcher | ✅ MERGED `7d6a0f68d` |
| `p5-cancel-hardening` 取消硬化 | feature/p5-cancel-hardening | ✅ MERGED `f440acf91` |
| `p5-data-sync` handler | feature/p5-data-sync | ✅ 已由 R2 取代 |
| `p5-data-sync-r2` D1/D2/D5 修复 | feature/p5-data-sync-r2 | ✅ MERGED `31af2b8fb8`，Tester 独立复验 **TEST_PASSED**（10 变异零存活，含修前存活/修后被杀对照），worktree/分支已清理 |
| `p5-frontend-cutover` 前端切 Job 路径（D9） | feature/p5-frontend-cutover | 🧪 TESTING（`e57c4855e`，Tester `3cdb27ef`）；**coder 自曝 2 例本轨引入 E2E 回归** |
| `p5-endpoint-overlap` 端点身份修复 | feature/p5-endpoint-overlap | 🔨 CODING（`653f2347c`，新增 `job_api/endpoint_identity.rs`） |
| Wave-R 全量回归 | — | ⏸ NOT_STARTED |

冲突面：cutover 只碰 `src/**`+`e2e/**`；endpoint-overlap 只碰 `packages/runtime/src/job/budget.rs`+`src-tauri/src/commands/data_transfer/**`。**两轨互为禁区**，与 data-sync R2 均零重叠。

## 跨轨裁定（已决定，非待定）

- **D9** 新 Job 命令零调用方、§2.3/§7 对用户不可见 → `p5-frontend-cutover` 轨收口。**「四门禁全绿」不得读作「已交付」。**
- **stage 内取消信号不可达** → `p5-runtime-cancel-watcher` 轨收口（内核唯一实现，退役 host 侧重复 watcher），已合入。
- **新 apply 路径必须 claim 存储侧 plan**（否则违反 §9 单管理器与 §2.1）——data-transfer 已修；**data-sync / schema-diff 迁移时必须同样检查**。
- **CM-40 降级**：cancel 路径不可注入 `Unknown`（handler 无该终态、注入点在 legacy 路径），前端轨改以 admission 拒绝证明覆盖。**CM-40 未覆盖**，留作敞口。

## 敞口项

**阻塞**
- **端点身份伪造（缺陷 k）**：`TRANSFER_SERVICE_KEY` 被 reader/writer 共用，`endpoint_refs()` 的 `connection_id` 是硬编码常量且 `detect_endpoint_overlap` 根本不用它 ⇒ 同名跨库复制被误判重叠，**Job 路径出现对 legacy 的功能倒退**。修复轨 `p5-endpoint-overlap` 进行中。**修完必须回头重做前端 `transferEndpointOverlap.ts`**（它只比表名，`A库.users → B库.users` 仍会误判），不得在本轨抢跑改。
- **缺口 (c)**：若 `apply` 阻塞到终态才返回，前端拿不到可寻址 jobId ⇒ 取消按钮无法寻址、无中间进度。已要求前端轨以**实测证据**回答，按阻塞级处理。

**测试与验证边界（8 项 E2E 缺口，已登记）**：UI 真实取消延迟；`CANCEL_POLL_INTERVAL`(50ms) 从未被测量（新测试的 `SETTLE` 隐式依赖它，**属未检验假设**）；三阶段之间取消；`kernel_cancel.rs` 仅 fixture 无真实驱动往返；cancel-then-resume 不重复提交；端到端读故障注入；真实 in-stage panic 的看门狗活性；DMG 打包。另有 data-sync 12 项 E2E/WDIO 缺口。
**⇒ 全部迁入 `docs/architecture/platform/data-migration-jobs.md` 的「验证边界」章节，不得写成本仓库的 Bug List 文档。**

**低**
- **D10** 生产路径超预算测试：预算夹取使 `unbounded_stage` 防御分支生产不可达；现有测试直驱 helper。**正确归属在 `packages/data-transfer/`**（`cm46_pipeline.rs`），data-sync 侧连预算测试文件都没有 ⇒ 两 crate 都仍敞开。
- 客户端遗留：`watchJob` 的 `onUpdate` 空实现；`listJobs`/`getJob` 无对应宿主命令（非空 `onUpdate` 会落到 `hydrationError`）。
- schema-diff `read_only_verify` 仍返回 `Indeterminate`。
- **D-I18N-1**：`i18n-sync-check.mjs` 的 `LOCALE_FILES` 不含 zh-CN。P5 新增 en key 已同步写 zh-CN；其余 8 locale 与存量欠债（main 缺 138）留给发布前 i18n 收尾轨。

**台账缺口**：D-F / D-G 的缺陷描述文本已丢失，**不可裁定，不得臆造**。

## 合入记录（各轨 progress.md 已随合并删除）

`bb2cc5606` `97885d6e0` `4ba769fc9` `5f46a0262` `0db23e6d0` `3e28d49b8` → merge `105494c14`；`7d6a0f68d` 同批；`f440acf91` → merge `7ec0ca315`；`feature/p5-data-sync-r2` → merge `31af2b8fb8`。

合并后 sanity（`--drivers=all`）：host `--lib` 1745 / data-sync 201 / runtime 843，`cargo check` 32 warnings（引入集合为空），854 条从零 `Compiling` 证明非陈旧产物。

## 操作纪律（会咬人的）

- **合流 sanity 第一步必须是 `cargo metadata --no-deps`**：git 按行合并不报重复键，重复依赖会让整个 workspace 以 EXIT=101 失败且**无任何文本提示**。已发生一次。
- **驱动集必须 `--drivers=all`**：`--drivers=basic` 与 `--drivers=all` 不是同一个 `DB_REGISTRY`；**所有 basic 下测得的宿主计数一律作废**。`pnpm typecheck` 对驱动集不敏感，typecheck 绿不能证明驱动集对齐。校验用 `cargo metadata --no-deps`（JSON 落文件后 node 解析，勿直接 dump stdout）+ `shasum -a256 drivers-registry.json`。
- **禁止 `cargo fmt --all`/`-p` 跨轨**：会格式化其他轨与禁区的文件。改用 `rustfmt --edition 2021 --check <file>`。
- **`typecheck` EXIT=127 且 `error TS` 计数为 0 是工具链缺失，不是通过**：先查 `node_modules` 是否退化成空目录（需软链主仓）。
- 验证方与提交方**不得共用一棵工作树**；清理 subagent 的工作树前**先确认它真的死了**。
- 警告数之争按 **warning 集合 diff** 裁定，不比计数。
- 「失败均为存量」需要**基线跑**，不能靠断言。

## 收尾义务

`hub.md` 与各轨 `progress.md` **一律删除**，不得存活到 `main`。P5 全绿并合入 main 前，本文件是唯一跨轨台账。