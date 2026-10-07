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
| `p5-frontend-cutover` 前端切 Job 路径（D9） | feature/p5-frontend-cutover | ❌ TEST_FAILED（Tester `0e5f9a54` @ `787f0e6fe`）→ 修复轨 `63d27428` 进行中，tip `f19c49fc5` 全绿。D2 门闸收口、D3 退役桥删除已闭环；**D4 判为不可达缺陷**（N3 存活）；**D1 已确认成立**，裁定后端停止预填，范围扩展至 `packages/data-transfer/src/mapping.rs`。待**新鲜** Tester 复验（`0e5f9a54` 已用） |
| `p5-endpoint-overlap` 端点身份修复 | feature/p5-endpoint-overlap | ❌ TEST_FAILED（Tester `10f3ae68`）→ 修复轨 `2cca952d` 进行中，tip `2940e2530`；**首轮门禁证据已作废**（非干净态采集），须以 `HEAD`/`TREE`/`SOURCE_SHA` 首尾配对重采。`job_kernel.rs` 1023 行拆分是**合并前置条件**；回退常量 `"data-transfer"` 的控制变异仍欠 |
| `p5-schema-diff-endpoint-identity` schema-diff 侧端点身份 | — | ⏸ 待开（D3 裁定；排在 endpoint-overlap 合入后） |
| Wave-R 全量回归 | — | ⏸ NOT_STARTED |

冲突面：cutover 只碰 `src/**`+`e2e/**`；endpoint-overlap 只碰 `packages/runtime/src/job/budget.rs`+`src-tauri/src/commands/data_transfer/**`。**两轨互为禁区**，与 data-sync R2 均零重叠。

## 跨轨裁定（已决定，非待定）

- **D9** 新 Job 命令零调用方、§2.3/§7 对用户不可见 → `p5-frontend-cutover` 轨收口。**「四门禁全绿」不得读作「已交付」。**
- **stage 内取消信号不可达** → `p5-runtime-cancel-watcher` 轨收口（内核唯一实现，退役 host 侧重复 watcher），已合入。
- **新 apply 路径必须 claim 存储侧 plan**（否则违反 §9 单管理器与 §2.1）——data-transfer 已修；**data-sync / schema-diff 迁移时必须同样检查**。
- **CM-40 降级**：cancel 路径不可注入 `Unknown`（handler 无该终态、注入点在 legacy 路径），前端轨改以 admission 拒绝证明覆盖。**CM-40 未覆盖**，留作敞口。后经 Tester 复验：不仅未覆盖，而且 **claim 先于回执查找 ⇒ 丢回执后重试永远拿不回原结果**（已升级为功能缺陷，见阻塞项 `p5-job-addressable` 第 3 条）。
- **前端轨剩余缺陷（Tester `3cdb27ef` 判定，均已核实于生产调用点）**
  - D-6 **§8.4 竞态（high）**：`DataTransferWindow.tsx:946-957` `goNext` 先 `await` 再切步；`TransferMappingStep` / `ColumnMappingEditor` 的 target `<Input>` 无 `disabled`；`:773/:924` 依赖的是**点击时刻捕获的 `endpointOverlaps` 快照**。prepare 在飞期间可改成同名绕过闸门。由 `70854e7b` 修。
  - D-7 M3 存活是 D-6 的**同一根因**（守卫捕获的是快照），随 D-6 一并消除，不单列。
  - D-8 CM-40 未覆盖（见上）。D-11 `DataTransferWindow.tsx` **1889 行**——本轮接受为存量债，但 **schema-diff / data-sync 前端切 Job 落地前必须先拆**，否则会在此文件上继续叠加。
- **D-1（新建表目标名）— 协调者原裁定「不成立」已撤回，缺陷成立。** 原判据是四条"独立代码事实"（`model.rs:311-323`、`mapping.rs:229/277/304`），**读漏了路径**：实读 `packages/data-transfer/src/mapping.rs:81-99`，Structure / StructureAndData 模式目标表不存在时预填 `target_table = 源表名` + `create_new: true`，`:141-154` 将其**原样**搬进 inspect 结果；Data 模式 `:100-111` 才给空串。⇒ 后端**产得出** `createNew:true + targetTable===sourceTable`，`:687` 不改的指令已随裁定一并作废（该行至今未动）。
- **裁定：后端停止预填**（`mapping.rs:81-99` 的 `target_table` 改空串，`create_new` 保持 `true`）。四条理由按强度：① `:89-93` 注释自称"创建不存在的目标是用户的明确选择"，同一个 struct literal 却预填了名字——**代码自相矛盾**，名字既属用户，后端就无权代起；② 同函数同情形两模式两种结果，让 Structure 对齐 Data 是统一线上契约的最小改动；③ 线上格式**无"建议名 / 已确认名"标志位**，预填名与用户输入名不可区分，因而必然自动满足 D-2 门闸——**D-2 刚建的闸门会被这条预填静默废掉**；④ 反过来在前端修就得擦掉后端发来的值，正是 D-10 已踩过的歧义。已批准 `p5-frontend-cutover` 扩范围至该文件（与 endpoint-overlap 零文件冲突）。
- **D-4 未成立为可达缺陷**：`63d27428` 变异 N3（删 `SourceFilterEditor.apply` 的 guard）**存活**，62 测试全绿 ⇒ 该 guard 今天杀不了任何东西；N4（删 `ColumnMappingEditor.commit` 的 guard）被存量 §8.4 测试杀掉，链条本已闭合。可达性以**实测**判定而非推理：`DISABLED_INPUT_ONCHANGE_CALLS=1` / `DISABLED_BUTTON_ONCALLS=0`。guard 保留为纵深防御，但注释须写明**不是修复**，否则下一个人会再报一次。
- **复用成本修正**：`migrationJobVerdict.ts`(336) + `MigrationJobVerdictPanel.tsx`(311) 可原样复用；`transferJobs.ts`(182) + `useTransferJobRun.ts`(341) + `MigrationJobFailureNotice.tsx`(121) 硬编码三个 transfer 命令名、需依赖注入后才能给另两套用 ⇒ **约 768 行可直接复用，约 644 行需返工**。"另两套几乎白拿"的说法偏差接近一半工作量，禁止据此排期。

## 敞口项

**阻塞**

- **端点身份（缺陷 k / D-1）— 修复轨回归，仍未闭口**。原始缺陷：`TRANSFER_SERVICE_KEY` 被 reader/writer 共用、`connection_id` 是硬编码常量，导致同名跨库复制被误判重叠。**这是本轮 2 例 WDIO 回归的真实根因，不是前端谓词**（Tester `3cdb27ef` 已证伪 coder 归因：谓词只 gate `canNext`，若它触发则用户根本点不到 Execute，症状不同；且 `inspect.rs:385` 给 target 传 `&[]`，谓词自身条件不成立）。
  `051a228a4` 的修法（`service_key = "data-transfer:"+sha256(物理摘要)` + `connection_id` 取自真实 `ConnectionConfig`，身份键取**并集**）方向正确，但 Tester `10f3ae68` 判 **TEST_FAILED**，两个阻塞缺陷：
  - **D1 误拒**：`budget.rs::identity_keys` 把 `Connection(connection_id)` 也当重叠键。同一已保存连接指向两个不同库（staging→prod）是 UI 一等状态（`ensureDedicatedSession` 即产出此形态），被判重叠且报错误文案「同一物理端点」。**根因是范畴错误**：`budget/ledger.rs:41/91-94` 的 `ensure_service` 按 `ConnectionId` 记账，全文件无 `service_key`/digest/物理服务概念——记账粒度 ≠ 身份粒度。该键只能把「接」变「拒」，不能反向，且它拒的全是物理不同的合法端点。
  - **D2 漏判（数据销毁级）**：`endpoint_identity.rs:82-88` 摘要**原始** config，而驱动默认 host 在**连接时**才解析（`connections.rs:41`）⇒ 省略 host 与显式写默认 host 产生不同 digest ⇒ 同一物理端点漏判。**当前只有 Connection 键在拦它**，故只删 Connection 键会把 D2 从误拒恶化为漏判，两条必须同一次改动一起修。
  修复要求：① 删检测侧 `Connection` 键；② 让 digest 吸收「省略 vs 显式 host」的不对称。**不得以削弱检测器的方式修 D1**，否则原始缺陷回归。
- **schema-diff 侧端点身份（D3，非阻塞，待开轨）**：`commands/schema_diff/job.rs:562-567/574-579` 构造了第三种更薄的 `service_key`（`dialect|host|database`，无 port/schema/tunnel/方言归一），叠加 `owner_connection_id(..).unwrap_or_default()` 可静默产出空 id。因 `service_key` 被**逐字**使用，这份字符串的权威被放大：`host=localhost,port=5432,db=app` 与 `port=5433` 同键 ⇒ 误拒两台不同服务器（D2 镜像）；`postgres` 与 `PostgreSQL` 不同键 ⇒ 漏判。**既有缺陷，非本轨引入**；Tester 仅代码阅读、未端到端执行。排在 endpoint-overlap 合入后（需复用其已解析身份的产物）。
  **已闭口的相近问题**：`service_key` 逐字使用（无前缀/形状检查），故 `packages/schema-diff/tests/job_handler.rs:360-371`（reader `conn-src`/writer `conn-tgt` 共享 `"svc-1"`）**仍正确拒绝，不是回归，无需开轨**——Tester 实测 `"svc-1"` 跑 `cm41_self_cover_endpoint_overlap_is_rejected` EXIT=0。
- **前端 §6.2 谓词已裁定删除**：谓词要求 `enabled === true`，生产链路产不出这种行 ⇒ 运行时 no-op；其文档注释声称与后端键一致是**假的**；现有测试用人工造的 `enabled: true` 数据，等于给不可达接线写了保护。§6.2 真正执行点是后端 `budget.rs::detect_endpoint_overlap`。**D-9（后端把 target 表填上）与端点身份落地后，前端谓词才可按「连接身份 + 库名」重写。**
- **缺口 (c)（D-3/D-4，§10 阻塞级）**：`apply_data_transfer_job` 用**单次阻塞** `runtime::run` 直到终态才返回 ⇒ jobId 只在终态后可得；前端 `applyJobIdRef.current` 又在 `await` **之后**才写、applying 期间强制置 null ⇒ **取消按钮在 apply 中途结构性不可寻址，且无中间进度**。且 `packages/backend-client/src/client.ts` 声明的 `listJobs`/`getJob` **没有对应宿主命令**。已从"证据链不足的怀疑"升级为**从生产调用点证实的缺陷**。根因全在后端 ⇒ 需**单开一条后端轨**，与前端轨 `src/**`+`e2e/**` 范围边界无关。
- **待开后端轨 `p5-job-addressable`**（排期在 `p5-endpoint-overlap` 合入之后，两者都碰 `commands/data_transfer/**`，**零重叠不可并行**）：
  1. `apply_data_transfer_job` 准入即返回 jobId，运行转异步；补 `list_jobs` / `get_job` 宿主命令（§10 可寻址 + 进度）
  2. `commands/data_transfer/**` 补 `info!`/`debug!` 进度事件（D-5：当前整目录零日志，线上无法定位卡在哪一步）
  3. **回执顺序**：`job_api/mod.rs:246` `plans::claim_plan` **先于** `:259` 的 `runtime::run`，而回执查找 `runtime.rs:195` 在 run 内 ⇒ **回执丢失后重试只会拿到 "already consumed"，永远拿不回原结果**。这是 §10 恢复路径结构性不可达，属功能缺陷而非覆盖缺口，须把回执查找提到 claim 之前。
  4. **D-9**：`inspect.rs:385` 传 `&[]` 并吞掉 `map_err(|_| ())` ⇒ 非法目标名静默回退，且 target 表在 data 模式下永不可寻址。

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

合并后 sanity（`--drivers=all`，冷构建 854 条 `Compiling` 证明非陈旧产物）：runtime 843 / data-transfer 213、`cargo check` 32 warnings（引入集合为空）。
⚠️ 同一次记录里的 host `--lib` 1745 与 data-sync 201 **已作废**——是协调者的计数/记录错误，非真实读数。Tester `10f3ae68` 在基线 `4b782750` 与 HEAD 上独立复测得 data-sync **176/176**、host `--lib` **1733→1757**，与 coder 声称一致；warnings 两侧各 44 行 / 16 个唯一 `-->` 位置，集合差为空，coder「基线 34」被证伪。**教训：门禁数字必须来自某一次实测运行，不能凭记忆落笔。**

## 操作纪律（会咬人的）

- **合流 sanity 第一步必须是 `cargo metadata --no-deps`**：git 按行合并不报重复键，重复依赖会让整个 workspace 以 EXIT=101 失败且**无任何文本提示**。已发生一次。
- **驱动集必须 `--drivers=all`**：`--drivers=basic` 与 `--drivers=all` 不是同一个 `DB_REGISTRY`；**所有 basic 下测得的宿主计数一律作废**。`pnpm typecheck` 对驱动集不敏感，typecheck 绿不能证明驱动集对齐。校验用 `cargo metadata --no-deps`（JSON 落文件后 node 解析，勿直接 dump stdout）+ `shasum -a256 drivers-registry.json`。
- **禁止 `cargo fmt --all`/`-p` 跨轨**：会格式化其他轨与禁区的文件。改用 `rustfmt --edition 2021 --check <file>`。
- **`typecheck` EXIT=127 且 `error TS` 计数为 0 是工具链缺失，不是通过**：先查 `node_modules` 是否退化成空目录（需软链主仓）。
- 验证方与提交方**不得共用一棵工作树**；清理 subagent 的工作树前**先确认它真的死了**。
- 警告数之争按 **warning 集合 diff** 裁定，不比计数。
- 「失败均为存量」需要**基线跑**，不能靠断言。
- **对照变异的正确方向是修复前 RED、修复后 GREEN**，不是「修复前必须存活」。协调者给 Tester 的 brief 曾把对照变异写成「修前必须 SURVIVE（绿）」，语义写反；若照字面执行会反向奖励没锁住缺陷的测试。Tester 标出了这处措辞歧义并按正确方向执行——**brief 的判据本身也要能被反驳。**
- **记账粒度 ≠ 身份粒度**：连接 id 是配额记账键，不是物理端点身份。把两者混用会让检测器只能「多拒」而不能「多接」，误拒全部落在合法端点上。
- **裁定缺陷前必须亲自读被引用的代码路径,不能只核对引用是否自洽。** 协调者据四条"独立代码事实"判 D1 不成立,实际漏读 `mapping.rs:65-97` 的 auto-build 与 `:141-146` 的 `!enabled` 传播,被变异执行推翻。**变异执行/杀死的证据强度高于读码推理**;两条裁定同源同错,说明这不是偶发。
- **门禁数字必须命名来源状态**(`@commit` / `@<worktree sha>`),并首尾各采一次 `HEAD`/`TREE`/`SOURCE_SHA`(仅受跟踪文件,排除两个构建期注入产物)且要求相等。`WORKTREE_SHA` 无法对应可提交状态:`resolve-drivers --drivers=all` 会往**受跟踪**的 `src-tauri/Cargo.toml` 注入 31 行并让 cargo 重写 `Cargo.lock`,二者靠 `git checkout --` 还原(20→18)。协调者曾据 `WORKTREE_SHA` 漂移误判"有人边跑边改",已认错。
- **门禁必须跑在最终 HEAD 上；门禁跑过之后又落了代码提交,该门禁即作废。** 2026-10-07 **两条在跑的轨同时踩中**:endpoint-overlap 门禁 08:43、拆分提交 08:54;frontend-cutover 门禁 `@299b7562b`、HEAD `4091d749a`(后者改了 `SourceFilterEditor.tsx` 生产代码 + `DataTransferWindow.test.tsx` 测试文件)。测试文件参与 typecheck,故这类失效**不可见**。正确顺序:代码冻结 → 提交 → 在该提交上跑门禁。读数可以平移的唯一条件是 `git diff --name-only <门禁commit> <HEAD>` 为空或只含非门禁文件,且该文件清单必须写进台账,否则就是无依据断言。
- **工作区指纹不能用 `git write-tree`**——它哈希的是 **index**,不是工作树。index 未更新时它会给出"字节没变"的假象(实测:只剩一个文件 staged,其余 unstaged,前后 `write-tree` 完全相等)。要证明 tsc/vitest 读到的字节没变,必须逐文件哈希工作区内容本身。仪器测的不是被测对象时,"首尾相等"什么都没证明。

## 收尾义务

`hub.md` 与各轨 `progress.md` **一律删除**，不得存活到 `main`。P5 全绿并合入 main 前，本文件是唯一跨轨台账。