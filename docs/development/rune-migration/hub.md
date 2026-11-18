# Rune 迁移并行协调中心（跨轨汇总）

> 协调者维护；每轨 coder/tester 不写本文件。全部轨道验收合流后删除本文件并提交清理。

## Wave 1（2026-11-18）：逐文件台账第二批，4 轨并发

基准分支：`feat/rune-native-ui-migration`（启动时 HEAD `7288aa6ce`，10/629 done）。

| 轨道 | worktree | 分支 | task | React 源 | 状态 |
| --- | --- | --- | --- | --- | --- |
| rune-schema | `.worktrees/datazen-rune-schema` | `feature/rune-schema` | T027 | `components/schema/SchemaDiffPanel.tsx` | CODING（coder `ses_e12ab0de5ffdInW1OaWIS7bOpr`） |
| rune-query | `.worktrees/datazen-rune-query` | `feature/rune-query` | T030 | `components/query/ExplainPlanTree.tsx` | CODING（coder `ses_e12ab0de4ffeuHnGvy1eq4nlCV`） |
| rune-migration | `.worktrees/datazen-rune-migration` | `feature/rune-migration` | T068 | `components/migration/MigrationJobFailureNotice.tsx` | CODING（coder `ses_e12ab0de2ffdwZRpqeloaoqIE2`） |
| rune-datatable | `.worktrees/datazen-rune-datatable` | `feature/rune-datatable` | T007 | `components/DataTable/DataTableSelectionBar.tsx` | CODING（coder `ses_e12ab0de1ffehj0xPNzl1MGppf`） |

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

### Tester 会话

（待 coder 返回后填入）
