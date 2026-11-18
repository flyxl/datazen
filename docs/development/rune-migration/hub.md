# Rune 迁移并行协调中心（跨轨汇总）

> 协调者维护；每轨 coder/tester 不写本文件。全部轨道验收合流后删除本文件并提交清理。

## Wave 1（2026-11-18）：逐文件台账第二批，4 轨并发

基准分支：`feat/rune-native-ui-migration`（启动时 HEAD `7288aa6ce`，10/629 done）。

| 轨道 | worktree | 分支 | task | React 源 | 状态 |
| --- | --- | --- | --- | --- | --- |
| rune-schema | `.worktrees/datazen-rune-schema` | `feature/rune-schema` | T027 | `components/schema/SchemaDiffPanel.tsx` | coder `9d72551fa` READY_FOR_TEST → 独立 tester `ses_e12702aabffe6IBfXaVv8t0pGa` 审计中 |
| rune-query | `.worktrees/datazen-rune-query` | `feature/rune-query` | T030 | `components/query/ExplainPlanTree.tsx` | coder `63f103886` READY_FOR_TEST → 独立 tester `ses_e1262f261ffexYarPyVb5OfxW2` 审计中 |
| rune-migration | `.worktrees/datazen-rune-migration` | `feature/rune-migration` | T068 | `components/migration/MigrationJobFailureNotice.tsx` | coder `f310cb424` READY_FOR_TEST → 独立 tester `ses_e12643fd9ffeIuL2anSLQmyKkY` 审计中 |
| rune-datatable | `.worktrees/datazen-rune-datatable` | `feature/rune-datatable` | T007 | `components/DataTable/DataTableSelectionBar.tsx` | coder `ad388d37d` READY_FOR_TEST → 独立 tester `ses_e12614c3affeCmhrcAFsbkFH47` 审计中 |

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
