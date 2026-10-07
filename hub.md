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
| `p5-frontend-cutover` 前端切 Job 路径（D9） | feature/p5-frontend-cutover | ✅ MERGED `7edfbd2e6`，Tester `10b025d7` 独立复验 **TEST_PASSED**（3 位 Tester、前 2 位 TEST_FAILED）。基线 typecheck 0 / vitest 572 文件 6031 用例全绿 / data-transfer 209 / host `--lib` 1733。**M1 控制变异（恢复预填）KILLED**；M2/M3/M4/M5 全 KILLED —— **M5 只在前端单侧恢复预填仍杀 2 条**，证明 D-2 门闸前后端两侧都受保护，不单靠后端。zh-CN blob 与 merge-base 逐字节相同，只改 `en.ts`。**A2 预警的"合并会撞 zh-CN"经实测不成立**：集成分支上该文件 blob 就是 `21d210d0…`，那些 key 从未在集成侧存在，合并零冲突。worktree/分支已清理 |
| `p5-endpoint-overlap` 端点身份修复 | feature/p5-endpoint-overlap | ✅ MERGED `7909c7cd3`，worktree/分支待清理。Tester `08a136ed` **TEST_PASSED**：M1 控制变异 KILL、D1/D2 双向钉死、`database` 进 digest、17 驱动逐 id 枚举、范围收敛。postgres 红门禁用**等价性证明**结案（见纪律区），不依赖实机复现。其发现的两条 clippy 告警由 `b88b1d95` 修复（redis 未用 import + 新文件 doc 缩进列）。**合并提交独立复验**：独立 detached 树、`--drivers=all`、首尾 HEAD 一致且 DIRTY=0 ⇒ clippy 两目标文件 `is_primary` 告警 **0 / 0**（非空洞：JSON 内 `src-tauri/` 主 span 460 个）、redis `397 passed; 0 failed; 3 ignored`、runtime 34 个 `test result:` 全 ok、host `--lib` **`1774 passed; 0 failed; 6 ignored`** EXIT=0。`CODE_SHA`=`e545e46198e19459c4b5edb6c8bbccbe2cdaaa6acff4def2b42e3d09ae4bb4d3`。`hub.md` blob 与父提交逐字节相同 ⇒ 轨道未篡改台账；`progress.md` 不存在于 HEAD |
| `p5-job-addressable` job 运行期可寻址 | feature/p5-job-addressable | ⏳ **coder 并行中**（基线 `d6423f9d1`）。范围仅 `commands/data_transfer/job_api/**`。三缺陷：apply 阻塞到终态才返 jobId ⇒ 取消按钮中途不可寻址且无中间进度；客户端已声明 `listJobs`/`getJob` 且直接 `transport.call` 但宿主命令零命中（半接线）；`claim_plan` 先于 run 内的 `lookup_receipt` ⇒ 丢回执后重试永远拿不回原结果。附带重写该文件 5 处引用文档章节号的注释 |
| `p5-transfer-target-tables` target tables 真正到达 inspect_tables（D9） | feature/p5-target-tables | ✅ coder 已交付 `b92e1da4e`，**待独立 Tester**。**D-9 所述缺陷在本轨基线不存在 —— 撤回。** 我独立复核四个调用点：`inspect_data_transfer_impl` 传的是 `&tgt_tables`（真实目标目录）；另外三处传 `&[]` 的**同时**把 `target_schemas` 传 `&HashMap::new()`，即作用域内根本没有目标库，不是漏传。**我先前台账上的"四个生产调用点全传空切片"是错的**，据此我还向用户报过一次，纠正。coder 6 个变异体全 KILL、零存活、零编译失败丢弃；**M1（库到库传回 `&[]`）杀掉既有 19 条宿主测试** ⇒ 该行为早有覆盖，不是新接线。门禁：data-transfer `216 passed`、host `--lib` `1777 passed / 6 ignored`、rustfmt 0/0、clippy 警告集差 `comm -13`/`comm -23` 均空。`create_new = true` 连带裁定**不是缺陷**：M2 去掉它会让既有测试变红，M5 证明顺序也承重 —— 它与"文件目标传空目录"是同一件事的两半，删任一半都让文件目标路径报错。`mapping.rs` 生产改动仅一条注释（把 `§8.4` 换成自足理由），其余为新增测试。**两条遗留**：① `e2e/specs/data-transfer-job-window.ts` 里写着同一条假说法（"传空 target-table list 所以自动匹配永不进入"），本轨禁改 e2e，**需要人接手**；② `datazen-platform-api` 有 `E0004` non-exhaustive patterns，是除已知 redis 两条外的**第三个基线 error**，本轨未修 |
| `p5-comment-docrefs` 清注释里的文档引用 | — | ⏸ 待开（低，改动面广但无语义风险）。**必须排在 `p5-job-addressable` 与 `p5-transfer-target-tables` 之后**：实测 P5 diff 里含 `§` 注释、且落在在飞两轨范围内的有 10 个文件（`job_api/` 下 8 个源文件 + 2 个测试文件 = **A 的全部范围**，另加 `mapping.rs` = **B 的**）⇒ 现在开必然冲突。**P5 自己带进来 246 行注释含章节号**（既有文件同类密度 26–33 行/个，说明是沿用旧惯例）。须重写为自足理由，**不是删 `§` 了事**，注释里的判断依据要留下。全仓既有约 2100 行属存量，**明确排除在 P5 外**，不顺手刷。纯注释改动 ⇒ 变异测试不适用，门禁为注释内容 diff + host/runtime 全套件 |
| `p5-clippy-approx-constant` | feature/p5-clippy-approx | ✅ coder 已交付 `feb0748b8`（`f04669bd4` 修复 + 台账），**待独立 Tester**。基线是 redis lib-test 目标 2 条 `clippy::approx_constant` **error** 使 `cargo clippy --all-targets` EXIT=101。**coder 推翻了我两个前提**：① `approx_constant` 是上游 deny-by-default（在 `clippy::all` 内），不是 allow-by-default，所以"加 `allow` 属性"从来就不是可行解，错误等级与本仓配置无关（无 workspace `deny`、无 `RUSTFLAGS`、无 `.cargo/config.toml`、无 `clippy.toml`、无 CI clippy job）；② 这**不是** TTL/容差测试（无容差常量、无 TTL、无超时），所以我 brief 里"证明容差仍能挡住回归"那条要求**没有指称对象**，已改为"证明断言仍活着"。两处字面量语义本就不同，分别修：`exec.rs` 的 `result_type_of` 只按 variant 判别、payload 绑到 `_`，`3.14` 纯占位 ⇒ 换 `10.5`（RESP3 浮点计数器实际返回值）；`ops/mod.rs` 的 `value_to_string` **无 `Double` 分支**、落到 `other => format!("{other:?}")`，payload 承重 ⇒ 期望值改为 `assert!(s.contains(&format!("{payload:?}")))` 从构造值导出，不会漂移。**我自己独立复现**：clippy EXIT=0、error 级诊断 0 条、warning 32 条、`--lib` `397 passed / 0 failed / 3 ignored`；M1（改回 `3.14`）EXIT=101 killed，与 coder 报告一致。**⚠️ M6 存活（coder 报，我已独立复现）**：把字面量换成 `std::f64::consts::PI` 后 clippy EXIT=0、`approx_constant` 归零、测试照样 397 全绿。**这说明我原来把「clippy EXIT=0」当交付信号是错的 —— lint 分不清"真修"和"用常量骗过它"**。该修复的正当性只能来自语义（Redis 浮点计数器不会等于 π），不能来自 linter 放行；非同义反复的证明改由生产代码变异（M4/M5）承担。coder 另自报：crate 内仍有 32 条 warning 级诊断跨 9 个文件，本轨只清 error 那一半；redis 里 6 个重复的 `value_to_string` 定义未动 |
| `p5-datasync-test-flakiness` data-sync 套件可靠性 | — | ⏸ 待开（2026-10-07 开）。超时用例在 `src/windows/data-sync/__tests__/DiffDetail.test.tsx`（**我先前误记为 `src/windows/data-sync/DiffDetail.test.tsx`，无 `__tests__/` 层级，已更正**），跨页反选用例 `20_000` ms 上限。**已证与前端切轨无关**：归属 `src/windows/data-sync/`、本轨零改动、该文件自 `358a17fdf`(09-30) 未变，且同树 A/B 两次逐字节相同却一次过一次挂 ⇒ 内容不是判别变量。隔离成本实测 **1862ms ⇒ 10.7× 余量**，余量十倍仍抖 ⇒ 不是"差一点"，是超时口径/机器负载二选一，根因未确诊。**P5 收口前必须落地**，否则套件不可靠后无法区分后续回归。**须在机器安静时开** |
| `p5-schema-diff-endpoint-identity` schema-diff 侧端点身份（D3） | feature/p5-schema-diff-endpoint-identity | ⏸ **排在 `p5-job-addressable` 之后**，非独立轨（裁定见跨轨裁定）：修复要用 `endpoint_identity::identify`，而该函数现住 A 轨独占的 `job_api/` 且为 `pub(crate)`；两条出路都要动 A 的文件，裁定走提取为共享位置、由 A 轨顺手做 |
| `p5-redis-rustls-flake` redis 套件 flaky | — | ⏸ 待开（低）。**`connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses` 在 `packages/drivers/redis/src/connect/mod.rs`（我先前误记为 `live.rs`，已更正）**，在 endpoint-overlap HEAD 上观察到失败 2 次、随后连跑 5 次全绿，panic 落在第三方 `rustls-0.23.43/src/crypto/mod.rs:249`（进程级 CryptoProvider 未自动确定）。**是否由本轨新增 redis 测试改变调度时序而提高触发概率，Tester 明示不确定**；不得据此断言因果。**须在机器安静时开** |
| `p5-file-cap-debt` 800 行上限 | feature/p5-file-cap | ⏳ **coder 开工中**（基线 `d6423f9d1`）。`traits.rs` 1772→1795(+23)、`sqlserver.rs` 2566→2643(+77)，**两者基线时即已超限**，本轨只加剧未制造。可拆性见"跨轨裁定"末条。**纯重构 ⇒ 变异测试不适用**，门禁改为测试名精确对照 + 行数实测 + API 未变证据 + clippy 集合差分 + 行数守护测试的反向验证。**只要求"让大文件变小"而产出新的超大文件等于没做** |
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
- **D3（schema-diff 端点身份）并非独立轨，必须排在 `p5-job-addressable` 之后。** 我原先把它当"零文件冲突"排期，错了：它自己的 `commands/schema_diff/job.rs` 确实与在飞两轨零重叠，但那两处 `service_key: format!` 是手搓薄 key，要换成物理身份就必须调 `endpoint_identity::identify`，而该函数现在住在 `commands/data_transfer/job_api/endpoint_identity.rs` 且是 `pub(crate)`——**正压在 A 轨独占范围内**。两条出路都要动 A 的文件：(a) D3 跨模块 import data_transfer 的私有模块 ⇒ schema_diff 反向依赖 data_transfer，层级倒置；(b) 把 `endpoint_identity.rs` 提取为共享位置。**裁定走 (b)，且由 A 轨顺手做这次提取**，比让 D3 反向 import 更省。教训：排期前查的应是"**要用的能力住在哪**"，不只是"**自己要改的文件撞不撞**"。
- **两个 flake 轨不得与编译负载并行**：这是判据有效性前提，不是优先级问题。抖动轨的判据是"抖动出现/不出现"，多轨并发编译时观察到的失败可能只是并发负载、通过也可能只是没被压到——**数据不可归因**。`p5-datasync-test-flakiness` 与 `p5-redis-rustls-flake` 须留到机器安静时开。反之 `p5-clippy-approx-constant`（只碰 redis 两个 ops 文件）与 `p5-file-cap-debt`（只碰 driver-api + sqlserver）与在飞两轨零重叠，可并行；后者虽动 `driver-api`（A/B 门禁的构建依赖），但搬方法体不改公开 API，且各轨门禁跑在**各自分支**而非集成分支 ⇒ 实测零耦合。**工作并行，合并要排序**：大重构与两处正确性修复同时落地会让冲突归因变难。
- **`p5-comment-docrefs` 必须排在 A、B 之后**：实测 P5 diff 里含章节号注释、且落在在飞轨范围内的有 10 个文件——`job_api/` 下 8 个源文件 + 2 个测试文件（**A 的全部范围**）加 `mapping.rs`（**B 的**）。现在开等于把注释改写叠在两条尚未落地的重构上，必然冲突。
- **800 行上限的可拆性：Tester 的成本结论我要改一半**（她只报事实、未裁定）。实测顶层跨度：`traits.rs` 1795 行里 `pub trait DatabaseDriver` 占 **24–1134（1111 行，单个 trait）**、`KeyValueDriver` 1784–1795（12 行）、测试模块 1264–1783；`sqlserver.rs` 2643 行里 `impl SqlServerDriver` 33–662（630）、`impl DatabaseDriver for SqlServerDriver` 1000–1880（**881，单个 impl**）、测试 1883 起。Tester 写"Rust 不允许 trait 跨文件拆分，会牵动 `inventory` 注册与所有 impl 路径"——**后半句不成立**：`inventory` 注册与各 impl 路径都在驱动 crate，都不动。真正可行的是把**方法体**搬到子 `mod`（`fn foo(&self) -> X { helpers::foo(self) }`），trait/impl 签名原地不动。**但原作者计划的 `traits/{ddl,query,admin,meta}.rs` 按 trait 拆文件行不通**——这个文件里只有一个大 trait，拆不出四个。⇒ `p5-file-cap-debt` 的正确形态是"搬方法体"而非"按 trait 拆文件"。**不预先设计方案**（同 §8.1 纪律）

## 敞口项

**阻塞**

- **端点身份（缺陷 k / D-1）— 修复轨回归，仍未闭口**。原始缺陷：`TRANSFER_SERVICE_KEY` 被 reader/writer 共用、`connection_id` 是硬编码常量，导致同名跨库复制被误判重叠。**这是本轮 2 例 WDIO 回归的真实根因，不是前端谓词**（Tester `3cdb27ef` 已证伪 coder 归因：谓词只 gate `canNext`，若它触发则用户根本点不到 Execute，症状不同；且 `inspect.rs:385` 给 target 传 `&[]`，谓词自身条件不成立）。
  `051a228a4` 的修法（`service_key = "data-transfer:"+sha256(物理摘要)` + `connection_id` 取自真实 `ConnectionConfig`，身份键取**并集**）方向正确，但 Tester `10f3ae68` 判 **TEST_FAILED**，两个阻塞缺陷：
  - **D1 误拒**：`budget.rs::identity_keys` 把 `Connection(connection_id)` 也当重叠键。同一已保存连接指向两个不同库（staging→prod）是 UI 一等状态（`ensureDedicatedSession` 即产出此形态），被判重叠且报错误文案「同一物理端点」。**根因是范畴错误**：`budget/ledger.rs:41/91-94` 的 `ensure_service` 按 `ConnectionId` 记账，全文件无 `service_key`/digest/物理服务概念——记账粒度 ≠ 身份粒度。该键只能把「接」变「拒」，不能反向，且它拒的全是物理不同的合法端点。
  - **D2 漏判（数据销毁级）**：`endpoint_identity.rs:82-88` 摘要**原始** config，而驱动默认 host 在**连接时**才解析（`connections.rs:41`）⇒ 省略 host 与显式写默认 host 产生不同 digest ⇒ 同一物理端点漏判。**当前只有 Connection 键在拦它**，故只删 Connection 键会把 D2 从误拒恶化为漏判，两条必须同一次改动一起修。
  修复要求：① 删检测侧 `Connection` 键；② 让 digest 吸收「省略 vs 显式 host」的不对称。**不得以削弱检测器的方式修 D1**，否则原始缺陷回归。
- **schema-diff 侧端点身份（D3，非阻塞，待开轨）**：`commands/schema_diff/job.rs:562-567/574-579` 构造了第三种更薄的 `service_key`（`dialect|host|database`，无 port/schema/tunnel/方言归一），叠加 `owner_connection_id(..).unwrap_or_default()` 可静默产出空 id。因 `service_key` 被**逐字**使用，这份字符串的权威被放大：`host=localhost,port=5432,db=app` 与 `port=5433` 同键 ⇒ 误拒两台不同服务器（D2 镜像）；`postgres` 与 `PostgreSQL` 不同键 ⇒ 漏判。**既有缺陷，非本轨引入**；Tester 仅代码阅读、未端到端执行。排在 endpoint-overlap 合入后（需复用其已解析身份的产物）。
  **已闭口的相近问题**：`service_key` 逐字使用（无前缀/形状检查），故 `packages/schema-diff/tests/job_handler.rs:360-371`（reader `conn-src`/writer `conn-tgt` 共享 `"svc-1"`）**仍正确拒绝，不是回归，无需开轨**——Tester 实测 `"svc-1"` 跑 `cm41_self_cover_endpoint_overlap_is_rejected` EXIT=0。
- **前端 §6.2 谓词已裁定删除**：谓词要求 `enabled === true`，生产链路产不出这种行 ⇒ 运行时 no-op；其文档注释声称与后端键一致是**假的**；现有测试用人工造的 `enabled: true` 数据，等于给不可达接线写了保护。§6.2 真正执行点是后端 `budget.rs::detect_endpoint_overlap`。**端点身份落地后，前端谓词才可按「连接身份 + 库名」重写**（原文把 D-9 也列为前提，现撤回 —— D-9 不存在，见敞口项）。
- **缺口 (c)（D-3/D-4，§10 阻塞级）**：`apply_data_transfer_job` 用**单次阻塞** `runtime::run` 直到终态才返回 ⇒ jobId 只在终态后可得；前端 `applyJobIdRef.current` 又在 `await` **之后**才写、applying 期间强制置 null ⇒ **取消按钮在 apply 中途结构性不可寻址，且无中间进度**。且 `packages/backend-client/src/client.ts` 声明的 `listJobs`/`getJob` **没有对应宿主命令**。已从"证据链不足的怀疑"升级为**从生产调用点证实的缺陷**。根因全在后端 ⇒ 需**单开一条后端轨**，与前端轨 `src/**`+`e2e/**` 范围边界无关。
- **待开后端轨 `p5-job-addressable`**（endpoint-overlap 已于 `7909c7cd3` 合入，冲突面已释放，本轨现在可开）：
  1. `apply_data_transfer_job` 准入即返回 jobId，运行转异步；补 `list_jobs` / `get_job` 宿主命令——客户端 `backend-client` 已声明 `listJobs`/`getJob`，宿主侧**全仓零匹配**，故可寻址与中间进度都不成立
  2. `commands/data_transfer/**` 补 `info!`/`debug!` 进度事件。**口径**：整目录 `info!`/`debug!`/`trace!` 均为 0，但 `commands/schema_diff/**` **同样 0/0** ⇒ 这是**整个 commands 层的日志缺口**，不是 data-transfer 独有，修的时候别只改一处
  3. **回执顺序**：`apply_data_transfer_job` 里 `plans::claim_plan` 先于 `runtime::run`，而回执查找在 run 之内 ⇒ **回执丢失后重试只会拿到 "already consumed"，永远拿不回原结果**，恢复路径结构性不可达。属功能缺陷而非覆盖缺口，须把回执查找提到 claim 之前
  4. **D-9：撤回 —— 所述缺陷在基线不存在。**（我先后写过两版错误说法：先是"吞错误"，再是"四个生产调用点全传空切片"，两版都被读码证伪。）实测四个调用点：库到库的 `inspect_data_transfer_impl` 传的是真实目标目录 `&tgt_tables`；另外三处传空切片的同时把 `target_schemas` 传 `&HashMap::new()`，作用域内**根本没有目标库**，即文件目标路径无目录可读 —— 是正确适配，不是漏传。`create_new` 强制同理：变异证明删掉它或调换判定顺序都会让既有测试变红。**教训**：我把"按参数名 grep 零命中"直接当成"参数是空的",中间跨了一个未经验证的假设。零命中只说明寻址方式用错(该参数按位置传递),推不出实参为空。

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
- **工作区指纹不能用 `git write-tree`**——它哈希的是 **index**,不是工作树。index 未更新时它会给出"字节没变"的假象(实测:只剩一个文件 staged,其余 unstaged,前后 `write-tree` 完全相等)。要证明 tsc/vitest 读到的字节没变,必须逐文件哈希工作区内容本身。仪器测的不是被测对象时,"首尾相等"什么都没证明。**协调者自己又踩了一次**:用 `read-tree` + `write-tree` 造 squash 提交时,`rm -f progress.md` 只删了工作树副本、index 里还在,于是 progress.md 进了提交。同一形状的错误同一个会话犯两次,说明它必须写成硬步骤而不是"注意事项":**要剔除文件,用 `git rm --cached`,不要 `rm`**。
- **做 squash / 构造提交一律在独立临时树里做**,不要在集成分支的工作树上试。`git merge --squash` 会写入工作树,随后的 `git checkout -- .` **不删除未跟踪文件**,残留会挡住正式合并(报"请在合并前移动或删除")。
- **给 Tester 的 brief 里的"必须存在的缺陷"要先自己核实。** 协调者 brief 断言 `src/lib/transferEndpointOverlap.ts` "必须已删除",Tester 实测该文件在本分支**从未存在**,其删除提交在另一条 ref 上——没有可验证对象。brief 也会过期成错误事实,Tester 推翻时以其实测为准。

- **警告集合必须用 JSON 流判定，不能 grep human 格式的 `-->`。** cargo 对同 crate 的 lib / lib-test 告警去重，谁先编译谁打印 `-->`，于是 human 格式的差集是**编译顺序产物**：本轮实测会凭空报出 3 个"BASELINE 独有文件"（`vector/.../lifecycle.rs`、`victoriametrics/.../{resource_provider,victoriametrics}.rs`），而 JSON 流里 `comm -23` 是空集、两侧 per-crate 计数完全相同。唯一可靠法：`cargo clippy --workspace --all-targets --keep-going --message-format=json`，取 `level=="warning"` 且 `span.is_primary` 的 `file_name`，去重排序后 `comm -13` / `comm -23`。**推论：绝对告警条数在本仓库不可复现**（实测 985 / 987 条、246 / 248 路径；限定改动文件则是 24 / 26 条、7 / 9 路径），任何"17 条 vs 16 条"之争都无法用数字裁定，只认集合。**今后代理自报告警数一律要求它同时报口径**。
- **存量红门禁无法复现时，用等价性证明结案，不要停在"证据不足"。** 本轮 postgres `postgres_cross_database.rs:305` 红门禁在 Tester 的新工作树上不出现（无 `.env` ⇒ live 用例静默 SKIP，**"复现不出"不等于"已修复"**，也不是矛盾）。改证：生产 diff 只有两处 `unwrap_or("localhost")→unwrap_or(DEFAULT_HOST)` / `unwrap_or(5432)→unwrap_or(DEFAULT_PORT)`，两个常量**逐字节等于**被替换的字面量，测试目录 diff **0 个文件** ⇒ 新旧编译产物行为可证等价，该红门禁不可能由本轨引入（PG15+ 对非属主角色只给 public schema USAGE 无 CREATE，42501 于 `CREATE TABLE` 处，与本轨无关）。**此证明不依赖任何实机环境，比复现和结构论证都强。**

- **注释里不得引用文档内容**（章节号、`§x.y`、`CM-xx`、PRD 段落指针一律不行）。文档纪律要求方案落地后被改写为「已实现」事实并入架构文档，**章节号会随改写整体失效**，于是代码注释里的引用变成悬空指针，而注释是唯一不会随文档一起被审阅的地方。正确写法：把**理由本身**写进注释，让它脱离文档独立成立。实测 P5 带进来 **246 行**这类注释，而既有文件同类密度是 26–33 行/个 ⇒ 是**沿用旧惯例**而非凭空发明，所以这条必须显式写进每个 brief，靠默认约定不管用。全仓既有约 2100 行是存量，**不在 P5 偿还范围**。
- **行号是易失事实，不要在 brief / 注释 / 台账的**论断**里依赖它**（作为「现读现用」的定位当然可以）。行号一合并就漂移，协调者已因此两次把错行号写进 brief。**锚点用「文件 + 函数名 / 符号名」表述**，让接手的人自己重新定位。
- **「0 告警」这类否定读数必须先证明它非空洞。** 本轮我写了 `Checking datazen v` 出现次数 ≥1 当护栏，它报了 0，差点据此把 `endpoint_identity.rs` 的零告警当成通过——**cargo 会重放缓存单元的诊断**，`Checking` 行没打印不代表没分析。正确护栏是看 JSON 流里**主 span 的路径分布**（实测 `src-tauri/` 主 span 460 个 ⇒ 宿主确实被分析了）。和纪律区「不可复现的告警数」是同一类错误的镜像：一个假阴性、一个假阳性。
- **`/tmp` 下的 detached 验证树必须软链 `node_modules` 到主仓**，否则 `resolve-drivers` 直接 `ERR_MODULE_NOT_FOUND: fflate`，codegen EXIT=1，后续所有门禁跑在**残缺 codegen** 上却照样报绿。已踩一次：门禁跑到 clippy 才被我发现，**整轮作废重来**。symlink 进 gitignored 路径，不污染 `git status`。
- **`//!` 文档列表的续行必须落在第 2 列**（先剥掉 `//! ` 前缀再数空格，`* item` 从第 0 列起）。落在第 1 列是错位，落在第 3 列会触发 `doc_lazy_continuation`。**数空格前必须先剥前缀**：直接对 `//!` 开头行跑 `sed 's/[^\ ].*$//'` 会因为首字符是 `/` 而**每行都得 0**。本轨曾把全部续行改到 1 列，`doc_lazy_continuation` 从 1 条涨到 6 条。
- **BSD `sed -i` 不接受 `-i 's///'` 写法**（缺后缀参数），会报 `extra characters at the end of p command` **且该行之后的流程照跑**，于是一条**根本没施加的变异**配上了一个漂亮的 EXIT=0，看起来像"变异存活"或"变异被杀掉"。**任何门禁脚本改完文件后必须回读内容确认它真的变了**，不能只看退出码。我已因此报废过一次自造变异，后续一律用 python 改写 + `git diff --stat` 回显确认。
- **变异存活有三种含义，混为一谈就会把结论做反**：① 断言没覆盖到该行为 ⇒ **覆盖缺口**；② 变异本身指错了符号（本 crate 有 **6 个同名 `value_to_string` 定义**，绑错哪一个会让"杀掉"变成假象）⇒ **先怀疑变异**；③ 该行为**按设计就无法被这道门禁区分**（例：`clippy::approx_constant` 放行 `f64::consts::PI`，linter 判不了"真修"和"骗过它"）⇒ **既非缺陷也非缺口，是门禁信号不足**，只能靠语义论证其正当性。③ 类只有换一条独立判据（生产代码变异）才能证伪，不能拿"门禁绿"当证据。
- **「lint / 门禁转绿」不是交付信号。** 凡是以"消掉一条告警"为目标的轨道，编译器给的绿灯**只能证明没有该类告警**，不能证明值改对了；固定做法是让断言从**被构造的值**导出（如 `assert!(s.contains(&format!("{payload:?}")))`），而不是把期望值写死成另一个字面量——后者只是把一个魔数换成了另一个魔数。
- **⚠️ brief 里不得携带结论 —— 这是我（协调者）最贵的一类错误。** 协调者是项管，不是开发/测试专家。我写给 Tester 的 brief 里**反复夹带自己的判读**（"某调用点传的是什么"、"A 已被我独立复核认可"、"X 已被证伪"），把待验证命题当成了前提灌给验证方；实测后果是 Tester 若照单全收，整条验收链的独立性就归零。**brief 只应包含三类内容**：① 验收范围（哪些提交、哪些文件是禁区）；② 门禁的机械要求（跑什么命令、退出码怎么算、必须落临时文件、必须首尾各记一次 HEAD）；③ 纪律（不得 fmt、不得读 .env 内容、变异必须回读确认已施加、编译失败不算 KILL、长输出截断处理）。**任何形如"实测/证明/已知/应当"的句子都不属于 brief**，属于报告。派发后若发现已夹带，**立刻发更正消息把它降级为"民间说法"**，并在下一份 brief 里剔除。
- **争议走第三方，不走裁决。** coder 与 Tester 结论冲突时，协调者**不裁定谁对**（我裁定过一次，直接把错结论写进了台账）。做法是另起一个独立 subagent 只做仲裁，输入是双方各自的证据原文，输出是"哪条可被代码/实验复核"。**没有可复核证据链的裁定一律不采纳**。
- **协调者不碰实现细节。** 不去定位编译错误、不去核对某个函数有没有某个分支、不去复算测试数字——这些属于 coder/Tester 的工作输入，越界会同时产生两个坏结果：给出错的判读污染下游 brief，以及把本该并行的时间占掉。

## 收尾义务

`hub.md` 与各轨 `progress.md` **一律删除**，不得存活到 `main`。P5 全绿并合入 main 前，本文件是唯一跨轨台账。