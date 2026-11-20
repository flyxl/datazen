# Rune 迁移并行协调中心（跨轨汇总）

> 协调者维护；每轨 coder/tester 不写本文件。全部轨道验收合流后删除本文件并提交清理。

## 台账 wave 编排（2026-11-18，脚本 `6961302d3`）

- tasklist 改为 **wave 排序**：T 编号即执行顺序，相邻任务不同 area（不同 `mod.rs`）；状态以 React 文件集为身份，main 合并重排编号也不错位（实测 18 done 无缝保留）。
- 取下一波：`node scripts/rune-parity.mjs --next-wave`（默认 4，贪心取不同 area，尾部自动缩波）。编号小的先完成。
- `--progress` / `--tasklist` 已改内容键控。Wave 1 的 18 个 done 已按此重排。

## Wave 2（2026-11-18）：按 --next-wave 前 4 个，4 轨并发

| 轨道 | worktree | 分支 | task（新编号） | React 源 |
| --- | --- | --- | --- | --- |
| rune-w2cell | `.worktrees/datazen-rune-w2cell` | `feature/rune-w2cell` | T002 | `components/DataTable/CellRenderer.tsx` |
| rune-w2hist | `.worktrees/datazen-rune-w2hist` | `feature/rune-w2hist` | T005 | `components/history/GlobalQueryHistoryDialog.tsx` |
| rune-w2conn | `.worktrees/datazen-rune-w2conn` | `feature/rune-w2conn` | T007 | `components/connection/ConnectionAdvancedSettings.tsx` |
| rune-w2chart | `.worktrees/datazen-rune-w2chart` | `feature/rune-w2chart` | T008 | `components/chart/ChartCanvas.tsx`（H 类，仅分发+容器+配置，renderer 范围外） |

### Coder / Tester 会话（Wave 2）
- Coder `ses_e1233dceafferyhFCC3JMvX2Ue` — T002 cell（rune-w2cell）
- Coder `ses_e1233dceaffd6BgPK0VTS3bcrP` — T005 history（rune-w2hist）
- Coder `ses_e1233dce9ffdhHr4kIaZ8wnXye` — T007 connection（rune-w2conn）
- Coder `ses_e1233dce8ffeJAD6nvb1KNtkns` — T008 chart（rune-w2chart）
- Coder `ses_e1233dce8ffeJAD6nvb1KNtkns` — T008 chart → `52647824d` READY_FOR_TEST（gate PASS，792 tests）
- Tester `ses_e122387b1ffeb3LBsuW7E4MbkU` — T008 chart，审计中
- 其余 3 coder（T002/T005/T007）仍在运行

## Wave 1（2026-11-18）：逐文件台账第二批，4 轨并发

基准分支：`feat/rune-native-ui-migration`（启动时 HEAD `7288aa6ce`，10/629 done）。

| 轨道 | worktree | 分支 | task | React 源 | 状态 |
| --- | --- | --- | --- | --- | --- |
| rune-schema | ~~removed~~ | ~~deleted~~ | T027 | `components/schema/SchemaDiffPanel.tsx` | ✅ 合流 `241c192bf`，audit `3fc801ef0`，T027 done |
| rune-query | ~~removed~~ | ~~deleted~~ | T030 | `components/query/ExplainPlanTree.tsx` | ✅ 合流（首个）`20cdc2cd6` 前，audit `730c7c86a`，T030 done |
| rune-migration | ~~removed~~ | ~~deleted~~ | T068 | `components/migration/MigrationJobFailureNotice.tsx` | ✅ 合流 `f1bac8c84`，audit `1486b380f`，T068 done |
| rune-datatable | ~~removed~~ | ~~deleted~~ | T007 | `components/DataTable/DataTableSelectionBar.tsx` | ✅ 合流（自动）`a1d0ec8bd` 前，audit `e697d375c`，T007 done |

**Wave 1 收口**：4 轨全部 TEST_PASSED → 合流 → 台账 done（14/647）；4 个 worktree + 分支已清理（`--force` 绕过 submodule 删除限制），磁盘 109Gi → 184Gi 空闲（释放 75Gi）。合流后完整 gate PASS（90/107/92 warnings）。合并 origin/main 引入的 mod 注册加法冲突均手工保留全部行。

### Wave21 阈值合流 push（2026-11-21）

- 触发：Wave 20 关闭时合计 ahead 43（seg1 26+seg2 7+seg4 10，seg3=0）> 30 阈值。
- 3 个真 merge commit：`4777b13bd`(seg1, 41dea64af) → `566e533d2`(seg2, aff5295a8) → `2b9e5a907`(seg4, c00eb860e)；seg3 ahead=0 跳过。父分支 `af0da69ec → 2b9e5a907`（46 commits，fast-forward）。
- 零冲突（新提交为加性）；三 crate check 全 PASS；gate 仍是本机 ld `arm64_b26` 分支范围超限（链接期失败，非代码），按已授权模式接受风险 push。无遗留孤儿进程。
- push 后残余合计 10（seg4），低于阈值。

### 第二段合流 push（2026-11-20，用户设阈值规则后首轮）

- 用户规则：**每个 seg1 wave 完成后检查合计 ahead，>30 commits 才合**。Wave 19 关闭时合计 96（seg1 14+seg2 13+seg3 3+seg4 32+后续增量）→ 触发合流。
- 4 个真 merge commit：`111dbf9b3`(seg1, f544297b8) → `1bebffd58`(seg2, 3a27e7125) → `b546f20c8`(seg3, e219b7ccf) → `f32675e9f`(seg4, 28af7ea64)；父分支 `ac6770e3f → f32675e9f`（78 commits，fast-forward）。
- 验证：三 crate check 全 PASS；gate test 步骤又是本机 Apple ld `arm64_b26` 分支范围超限（330 warnings 后链接失败），按用户已授权模式接受风险 push。零冲突、零断言弱化、无遗留孤儿进程。
- push 后残余 ahead 合计 22（seg1 12 + seg4 10），低于阈值 30，暂不合，下个 wave 后复查。

### 四段最新合流 push（2026-11-20，用户明确指示 push）

- seg1/2/3/4 最新代码以 4 个真 merge commit 合入父分支：`521af02dd`(seg1, tip c75292a05) → `2c2673de0`(seg2, e392bc177) → `d75c40f17`(seg3, 28856c392 — 首次纳入) → `cd55d8279`(seg4, 1823af18b)；父分支 `44f875d8b → cd55d8279`（122 commits，fast-forward，无覆盖）。
- 冲突解决：seg2 tests/mod.rs 双方 mod 声明合并去重；seg4 ai/mod.rs/view/mod.rs/tests/mod.rs 合并 + 可见性 union；target_claim.rs add/add 保留 seg1 T162 无重复 API + 追加 seg4 内联测试；基线修复仍在（workflow-workspace、无 ordered.reverse）。零断言弱化，零冲突标记。
- **验证**：三 crate `cargo check` 全 PASS；各段 tester 在各自 worktree gate 全绿；本轮 gate test 步骤编译完成（293 warnings、无 error）但 Apple ld 在链接 ~818MB __TEXT 二进制时报分支范围超限（`B/BL out of range`），ld-classic 同样失败、opt-level=z 被 OOM kill——纯本机 toolchain 尺寸限制，非代码/测试失败。用户明确指示接受风险 push。
- 各段仍在活跃推进（合流为快照），seg3/seg4 合流后又有新提交，后续按节奏再合。环境恢复（重启/换工具链）后应补跑全量 gate。

### 三段合流 push（2026-11-20，用户裁决选 1：接受风险）

- seg1（50→59 commits）/seg2（8→24）/seg4（20→40）以 3 个真 merge commit 合入父分支：`96176166b`(seg1) → `79f6c92de`(seg2) → `ef9f18aac`(seg4)，父分支 `ef70b6f11 → ef9f18aac`（81 commits，fast-forward，无覆盖）。
- 冲突解决（均已按 React parity 核实）：chart mod 加 axis_configurator；connection_page 保留 5-param layer 贯穿 + seg1 T337 document host（单一 modal layer）；3 个测试 call-site 传 throwaway layer；connection_page 测试基准 `workflows-page → workflow-workspace`（T199）；execution_result_panel 移除 reverse（React `steps.map` report order 已核实）。
- 上一轮记录的 3 个回归已在本轮合流冲突解决中修复：Berlin DST、connection_page Wapps key、workflow 面板 error 行。
- **验证状态**：`cargo check -p datazen-ui` 与三 crate check 全 PASS；各段 tester 在各自 worktree gate 全绿；但本机全量 gate 的 test 步骤因 2.1GB 测试二进制触发 Apple ld/dyld 崩溃（`__eh_frame` 过大 + 共享缓存映射失败），反复重编（ld-classic symlink / cc wrapper `-no_compact_unwind -s` 两种配置）均无法稳定产出可运行二进制，用户裁决接受风险 push。
- 本机根因：uptime 53 天 + 构建脏状态；期间清理了 2 个遗留孤儿编译进程（合流 subagent COMPLETED 后残留，非 buildq 串行队列）。环境恢复（重启）后应补跑全量 gate。

### 风险合流 + 待修回归（2026-11-20，用户裁决选 3）

- 用户明确选择「接受风险直接 push」：seg1/2/4 的 46 commits 合流 `05f4df187` 已 push 到父分支（fast-forward，未覆盖任何人工作）。
- **验证状态**：`cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline` 三 crate 全过；但 gate 测试步骤因本机 dyld 共享缓存映射崩溃（`map cache into shared region failed` + SIGSEGV/SIGABRT）无法稳定验证，`DYLD_SHARED_REGION=private` 下曾跑出一次结果。
- **已知未修回归**（健康二进制跑出 4920 passed / 3 failed，待后续修复）：
  1. `support::formatters::tests::last_connected_resolves_berlin_dst_at_the_instant` — Berlin DST 时区
  2. `tests::connection_page::every_section_stays_reachable_after_the_port` — Wapps key 期望
  3. `tests::workflow_execution_panel::the_workflows_page_mounts_the_panel_over_a_real_run` — workflow 面板
- 这 3 个是合流叠加产生（各段单独 gate 全绿），**不是**弱化/删除断言所致。环境恢复后须优先修复并重跑全量 gate。
- dyld 崩溃已排除代码/路径/target/负载因素，疑似本机 uptime 52 天的系统态，重启 Mac 可根治。

### 合流方向裁决（2026-11-19，用户指示）

- **合流方向维持子→父单向**：主会话只把各 segment 合到父分支，`merge-to-parent` 后不再把父分支快进/回灌到各 segment。
- **各段落后父分支时，由该段机器自己 merge 父分支并自行解冲突**，主会话不代劳、不强推（曾把 seg4 快进到父分支被用户纠正，已 force-push 回退到 seg4 自己的 `c8fff490f`；seg4 机器本地与远端重新一致）。
- seg4 机器操作：`git fetch origin && git merge origin/feat/rune-native-ui-migration`，解决 `ui/dialog.rs`/`views/modals.rs`/`Cargo.toml`/`tests/mod.rs` 等共享文件冲突后提交并 push 自己分支；确认 `gate: PASS` 后再由主会话合回父分支。
- 其余段（seg1/2/3）同规则：落后时各自 merge 父分支、自己解冲突。seg1 由本机 coordinator 处理。

### 全量合流到父分支（2026-11-19，主会话执行）

- 4 个 segment 成果首次全量合入 `feat/rune-native-ui-migration`：
  - seg1 `836ce3b94`（Wave A/B） / seg2 `0e6c4d095`（15 task）/ seg3 `c271ba598`（10 task）/ seg4 `007380f87`（9 task）。
  - 合流冲突已全部解决：app.rs（保留 `views::app_root` 委托，删除 seg3 过时内联 view）、state_def.rs（running_close_* + ai_* 字段加法）、views/mod.rs（双方 mod 行全保留）、ui/field.rs（disabled 语义合并）、icons.rs（三方重复 `ICON_ALERT_TRIANGLE` 去重）、workspace.rs（page_body 4 参数签名统一）、main_page.rs（补 layer 参数 + children）。
  - 合流后 gate 最终 PASS（check 103 / 三 crate 120 / test 103 warnings）。第一次后台 gate 的 17 failed 是竞态（在冲突解决中途启动读到中间态），稳定后重跑全绿。
  - 父分支已 push `836ce3b94..007380f87`。
- segment-1 已 reset 到含全部 4 段成果的基线 `836ce3b94`，交给独立 coordinator `ses_e0e4dc6ad…` 自治推进（用户指令）。主会话不再操作 segment-1 分支，只做全局合流/协调。

### 执行模型裁决（2026-11-19，用户选 A）

- 依赖分析（`task-dependencies.md` 2722 边）+ 拓扑排序提案（`task-order-topological.md` 203 波）已完成；拓扑 wave 与静态 segment 分段不兼容（每波混段、1602 跨段边）。
- 用户裁决：**方案 A — 维持现状**（静态 segment + 按号 wave），不改现有流程。理由：大部分反向依赖是假阻塞（Phase-0 基座已落地），少量返工可接受。
- 拓扑两份文档仅作参考提案保留，`tasklist.md` / `shards/*.md` 仍是唯一真相源，取任务继续用 `--shard N --next-todo`（按 T 编号）。

### 本机 segment-1 Wave B（2026-11-19，4 轨）

Wave A 全部收口（26/647）后继续，从 `--shard 1 --next-todo` 取前 4 个不同 area：
- `ses_e0e824cd7ffe2bhaM7yQk2Dma6` — T036 structureEditor helpers（rune-s1b-struct，lib/structureEditor，3 小 .ts）
- `ses_e0e824cd7ffdGwpgzBsgczDML6` — T045 platform/web（rune-s1b-web，14 行，可能 re-export 壳）
- `ses_e0e824cd7ffbu1bH7fKxhB7r79` — T049 ai/grantState（rune-s1b-grant，281 行）
- `ses_e0e824cd5ffeIN0Q47Vwwj2hhd` — T053 shell.ts（rune-s1b-shell，17 行，可能 re-export 壳）→ `70633e1a8` X-class 壳判定，tester `ses_e0e71b2b…` 审计中
- T036 struct `938c89192`（21 tests）tester `ses_e0e6e658…` 审计中；T045 web `3e0215bff` X-class 壳判定 tester `ses_e0e6e389…` 审计中；T049 grant coder 运行中
- 依赖分析文档 `task-dependencies.md`（5938 行 / 2722 边）已提交 `22e87ebfa`；拓扑排序新划分 `ses_e0e71e552…` 进行中（输出到 `task-order-topological.md`，不改现有 tasklist/shards）

队列 worker pid 38092（目录 `.../T/rune-buildq`，与 coder 的 `${TMPDIR}/rune-buildq` 一致）。合流路径同 Wave A：先合 segment-1 + 翻 shard-1 + push，整波收口后 merge-to-parent 1 + refresh 其余段。

### 本机 segment-1 Wave A（2026-11-18，4 轨）

从 `feat/rune-native-ui-migration-segment-1` 的 `--shard 1 --next-todo` 取前 4 个不同 area task，worktree 已 bootstrap、队列绑定已配：
- `ses_e11c3b7feffeuA2HmE50n4AU4n` — T010 MigrationEndpointsBar（rune-s1mig，components/migration）
- `ses_e11c3b7feffdCMJ2DR6aSZ4dz8` — T016 BatchExportDialog（rune-s1batch，windows/connection，2.0d split）
- `ses_e11c3b7fdffdKhnkMYAYdTkUo7` — T032 metadataCache（rune-s1meta，lib/relationMetadata，.ts 基础层）
- `ses_e11c3b7fbffeqS073noxizG8uX` — T034 counters（rune-s1counter，lib/session，.ts 基础层）✅ 合到 segment-1 `e1575c72e`
- Tester：`ses_e11a57ce7…` T034 TEST_PASSED `a05f1b30c` ✅ / `ses_e11a0a440…` T016 审计中 / `ses_e1192ccc…` T032 审计中 / `ses_e118eaf15…` T010 审计中
- Wave A coder 提交：T010 `343053bfd` / T016 `b0720b90d`（738+247 拆分）/ T032 `7e5fd1e0f`（6 子模块）/ T034 `74d4c8799`
- 队列路径事故：coder 的 `${TMPDIR}/rune-buildq` 与最初 worker 的 `${TMPDIR}/opencode/rune-buildq` 不一致导致 4 个 gate job 空等；已把 worker 迁到 coder 目录（`/private/var/.../T/rune-buildq`，pid 38092）并复制 bindings，job 正常消费。

合流目标：先合到 segment-1 分支（本机开发段），再由协调者用 `merge-to-parent` 合到父分支。注意：与 Wave 1/2 不同，本机现在的 task 合流到 segment-1 而非直接父分支。

### 机器分工（2026-11-18，用户裁决）

- **本机锁定 segment-1**（`feat/rune-native-ui-migration-segment-1`）作为其开发段；其他 3 段（segment-2/3/4）交给别的机器，按 [segment-runbook.md](segment-runbook.md) 执行。
- 本机双重角色：① 在 segment-1 上编码；② 继续做协调者——汇总 4 段 shard 状态、合流代码到主分支 `feat/rune-native-ui-migration`、push。
- Wave 2 在途的 T002（segment-3）/ T005（segment-4）照常收尾并移交：tester 跑完 → 审计提交合到主分支 → 翻台账 → push → refresh 4 个 segment 分支到主分支最新。代码成果随主分支带走，其他机器 checkout 时已包含。
- 后续本机新开的 coder worktree 从 segment-1 分支建，task 从 `node scripts/rune-parity.mjs --shard 1 --next-todo` 取。

### 纪律：合流即 push（用户要求）

- 每个 task 合流到 `feat/rune-native-ui-migration` 并完成台账翻状态后，**立即 `git push origin feat/rune-native-ui-migration`**，不等整波结束。
- 首个 push 已落地 `7288aa6ce..92349b69f`（Wave 1 全部合流 + main 合并 + 台账重编排）。上游分支已建立跟踪。
- push 是用户明确授权的外部操作；仍禁止其它外部副作用（建 PR / 发版 / 部署）。

### 冲突面控制

- 四轨各自新建 `components/{schema,query,migration}/` 目录；仅 T007 在既有 `components/data_table/` 加一个 `pub mod` 行，与其他轨不重叠。
- `components/mod.rs`、`tests/mod.rs` 各轨只加一行注册，合流时做纯加法冲突解决。
- 统一禁用：`icons.rs` / `app.rs` / `i18n.rs` / `build.rs` / locale 包 / `tasklist.md` / `progress.md`，从源头消除共享文件冲突。
- 所有 task 的 i18n key 已核实存在于 3,217 键表；图标全部复用现有 `ICON_*`（Trash2→Trash 为记录在内的偏差）。

### 合流流程（每轨）

1. Coder `READY_FOR_TEST` → 立即派发全新 Tester（只测不修，复现 coder 数字 + 变异反证）。
2. Tester `TEST_PASSED` → 协调者 `git merge --no-ff` 到集成分支 → `node scripts/rune-parity.mjs --status T0xx=done` → 门禁健全性校验 → `git worktree remove` + `git branch -d`（磁盘优先，逐轨及时清理）。
3. `TEST_FAILED` → resume 原 Coder 仅修所列缺陷 → 全新 Tester 复测；同轨最多 5 轮。
4. 四轨全部合流后跑 R 阶段全量回归。

### 环境变更（2026-11-18，已全轨暂停→修复→恢复）

应协调者指令，4 个 coder 全部回复 `PAUSED` 并冻结现场；期间完成：
- 本机 stable 工具链 1.90.0 → **1.100.0**（`rustup check` 确认 up to date），1.99.1 亦在（Rune web build 脚本固定用该版本）。
- `wasm32-unknown-unknown` target 分别装到 stable 与 1.99.1。
- 主检出 `vendor/rune` 子模块补齐（`git submodule status` 前缀由 `-` 变空格）。
- 主检出实测 `CARGO_TARGET_DIR=$PWD/target bash scripts/rune-migrate-gate.sh` → **gate: PASS**（已切到 1.100.0）。
- 4 个 coder 已发「继续」恢复，要求从暂停现场接着做、不 revert；warning 数字因换工具链上升不算回归。

### origin/main 合并（2026-11-18 15:5x，协调者执行）

- `git fetch origin main` → `git merge --no-ff origin/main` → `bec2d85ff`（+102 main commits，冲突仅 Cargo.lock）。
- Cargo.lock 冲突解法：不能用三方并集（会保留 wasm-bindgen 0.2.127/0.2.129 双版本导致解析冲突）；改为恢复 ours 的干净 lock 后 `cargo metadata` 在线解析，让 Cargo 自动纳入 main 的 parquet/arrow 依赖 → 版本统一 0.2.129、parquet 58.4.0 入锁。门禁复验 gate: PASS（89/106/92 warnings）。
- main 带来的 React 源码变动：`--tasklist` 刷新把台账 629 → **647 tasks**（+18）：valuePresentation 全套（lib 11 + hooks 3 + settings 窗口 2 + DataTable 的 ColumnPresentationDialog）、useColumnPresentationSession 等；10 个 done 状态按 T 编号保留，现算进度 10/647、11.7/641.9 人日。刷新提交 `71d1654a7`。
- 注意：4 个 in-flight worktree 基于合并前的 HEAD（7288aa6ce），它们合回时需以合并后分支为 base 做冲突解决；本合并未触碰任何 `crates/datazen-ui` 文件，预期无代码冲突。

### 串行编译测试队列（2026-11-18，替代各轨自行 cargo）

为避免 4 轨并发编译抢 CPU，所有 `cargo` / gate / test 收口到一个专属 build subagent 名下的单一串行 worker。

- 队列目录（绝对路径，跨 worktree 共享）：`/private/var/folders/lr/_2jh91ps6f5318t_f64jb9000000gn/T/opencode/rune-buildq`，含 `submit.sh` / `worker.sh` / `README.md`。
- 调用方（coder 与 tester）统一：`bash <Q>/submit.sh <requester> <workdir> '<原命令>'`；原子入队、阻塞等结果、原样输出、以 job 退出码退出。`running/` 中遗留 job 在 worker 重启时自动回收。
- 专属 build subagent 会话 `ses_e127a113dffekO0rk82JLQan69` 独占 worker，禁止第二实例；活性看 `heartbeat` 与 `worker.pid`。
- **纪律**：coder/tester 禁止直接跑 cargo/gate/test；在途的旧编译允许自然结束，不得新开。所有后续 tester 简报必须包含 submit.sh 调用方式。
- **worktree 绑定加固**：`submit.sh` 把 `rune-<track>*` requester 强制绑定到 `datazen-<track>` worktree，传错 workdir 直接 exit 2；worker 执行前记录真实 `pwd` 到结果的 `workdir_actual`，调用方校验不一致则 exit 3。因此编译必然发生在请求方的 worktree、覆盖其未提交改动，不会编到别的轨或主检出。新增纪律：job 在途时不得改源码（提交 job 后等 submit.sh 返回）。已通知全部 4 coder + T027 tester，并要求 build agent 热重启 worker 加载新脚本。

### Tester 会话

- `ses_e12702aabffe6IBfXaVv8t0pGa` — T027 schema，审计中（只测不修 + 变异反证，所有编译走串行队列）。
- `ses_e12643fd9ffeIuL2anSLQmyKkY` — T068 migration，审计中（同上）。
- `ses_e1262f261ffexYarPyVb5OfxW2` — T030 query，审计中（同上）。
- `ses_e12614c3affeCmhrcAFsbkFH47` — T007 datatable，审计中（同上）。
