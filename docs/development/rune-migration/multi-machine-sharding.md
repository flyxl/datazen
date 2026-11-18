# React → Rune 迁移：4 机分片并行开发方案

> 把 647 个 task 切成 4 段，4 台机器各领一段真正并行；本机（协调者）统一汇总。
> 本文是执行方案，不是过程台账。状态同步用各段独立文件，故跨机零冲突。

## 1. 可行性结论

**可以分片并行。** 依据（2026-11-18 实测）：

1. **task 之间基本无横向依赖。** 每个 task 是把一个 React 文件 1:1 翻成同路径 Rune 文件（`.tsx` → `views/*.rs`，`.ts` → `support/*.rs`），依赖的是「基座 + 基础层」，而不是别的待办 task。
2. **共享依赖已 100% port 在主分支**：`ui/{dialog,select,banner,spinner,toggle,menu,table,tree,…}`（Phase 0 十三项基座）、`support/{formatters,cell_types,cell_editor_kind,data_type_colors,to_edit_string}`、`i18n::t`、`AppState` 各 slice。任何一台机器 checkout 含这些基座的分支即可开工。
3. **少量同 area 内上游已落在同一段**：例如 `GlobalQueryHistoryDialog` 依赖已 port 的 `HistoryEntryCard/FilterBar`；`ChartCanvas` 依赖 5 个 renderer（同在 `components/chart` 段）。按 area 分段保证这类依赖不被拆散。

跨 area import 统计（顶部边）也印证：所有页面只依赖 `components/ui` / `hooks/useI18n` / `types` / `lib/cn` 这类共享底层，没有页面→页面的横向依赖环。

## 2. 分段原则

- **按 area（React 二级目录）切，不按 T 编号区间切。** wave 排序让相邻编号落在不同 area，按编号切会把同一 area 拆散到多台机器，反而制造跨机冲突；按 area 切则每段占用互不重叠的 `mod.rs` 注册块。
- **整 area 分配**：一个 area 的所有 task 归同一段（不拆 area），保证同 area 内上游不被分到不同机器。
- **按工作日均衡**：贪心把 area 分给当前负载最低的段，4 段均为 ~160.5 人日。

## 3. 四段划分（647 tasks / 641.9 人日）

| 段 | 分支 | task 数 | 人日 | 覆盖 area |
| --: | --- | --: | --: | --- |
| 1 | `feat/rune-native-ui-migration/segment-1` | 153 | 160.6 | `windows/connection`(101), `components/connection`(15), `components/filter`(7), `components/migration`(4), `lib/session`, `ai`, `lib/structureEditor`, `lib/relationMetadata`, `platform/web`, `lib/connectionViews`, `components/export` |
| 2 | `feat/rune-native-ui-migration/segment-2` | 162 | 160.5 | `lib`(117), `components/chart`(9), `windows/workflow`(9), `types`(10), `stores/ai`, `lib/relationPrediction`, `lib/metadata`, `windows/wapps`, `windows/main`, `platform/shared`, `lib/dashboard`, `components/schema`(1) |
| 3 | `feat/rune-native-ui-migration/segment-3` | 166 | 160.5 | `windows/data-sync`(37), `stores`(32), `components/DataTable`(23), `components/ai`(13), `stores/tableData`(16), `components`(11), `windows/data-transfer`(11), `platform`, `lib/chart`, `windows/backup`, `components/query`(4), `components/ui`(3), `components/admin`, `shell.ts`, `App.tsx` |
| 4 | `feat/rune-native-ui-migration/segment-4` | 166 | 160.3 | `components/sql-editor`(16), `windows/settings`(32), `hooks`(26), `commands`(23), `windows/schema-diff`(21), `windows/onboarding`(12), `lib/valuePresentation`(11), `windows/dashboard`(7), `windows/workspace`(7), `lib/sqlDialects`, `components/history`(3), `bootstrap`, `extensions`, `index.ts`, `app.ts` |

18 个已完成任务按 area 归属：段1（`components/filter`, `components/migration`）、段2（`lib`, `components/schema`）、段3（`components/ui`, `components/DataTable`, `components/query`）、段4（`components/history`）。这些基座/已完成件随主分支分发给各机器，不必重做。

## 4. 状态同步：各段独立文件（零冲突）

- 主台账仍为 `docs/development/rune-migration/tasklist.md`（**只由本机协调者汇总写**）。
- 每段一个状态文件：`docs/development/rune-migration/shards/segment-{1..4}.md`，**只有该段所属机器写自己的文件**。格式：每行一个 task，状态 + T 编号 + area + React 文件集；状态以 React 文件集为身份（与主编排一致，源码变动重排也不错位）。
- 机器只写 `shards/segment-N.md`，`git push` 自己的 segment 分支；永不动主 tasklist。
- 本机协调者定期 `--aggregate`：读 4 个 shard 文件的状态，按 React 文件集映射回主 tasklist，写回主文件。**因为各机器不碰同一文件，合流零冲突。**

## 5. 分支与合流流程

```
feat/rune-native-ui-migration            （本机主分支，含全部基座 + 主 tasklist）
├── feat/.../segment-1                   （机器 A checkout，领段 1）
├── feat/.../segment-2                   （机器 B，段 2）
├── feat/.../segment-3                   （机器 C，段 3）
└── feat/.../segment-4                   （机器 D，段 4）
```

- **开局**：4 台机器各自从最新的 `feat/rune-native-ui-migration`（含基座与 18 done）建自己的 segment 分支，独立开发。
- **机器侧循环**：取本段最小编号 todo → 单 worktree coder → 独立 tester → 合流到自己 segment 分支 → 写 `shards/segment-N.md` → push 自己分支。编译测试走串行队列（见下）。
- **本机协调者循环**：定期 fetch 4 个 segment 分支 → `--aggregate` 汇总状态到主 tasklist → 合流 4 段代码到主分支 → push 主分支。
- **收尾**：4 段全部完成后，主分支跑 R 阶段全量回归，删除 4 个 segment 分支与各段状态文件。

## 6. 跨机复用的纪律

- **环境**：stable Rust 1.100.0；`wasm32-unknown-unknown`（stable + 1.99.1）；`vendor/rune` 子模块 `git submodule update --init --recursive`；`node_modules` 软链，禁 `pnpm install`。
- **串行编译队列**：单机内所有 cargo/gate/test 走该机的 `rune-buildq/submit.sh`（requester↔worktree 绑定 + workdir_actual 校验），避免本机多轨抢 CPU。跨机器各自独立队列，不共享目录。
- **提交**：`git commit --no-verify`；只 `git add` 自己的文件；禁 `git add -A`；禁 `cargo fmt --all`；不建 PR/发版/部署。
- **合流即 push**（用户要求）：每个 task 合流后立即 push 自己分支；本机汇总后 push 主分支。
- **不变量**：React 页面与组件保留，Rune 是并行的另一套界面；严格 TypeScript 仅针对原前端，Rune 侧 Rust 禁裸 unwrap/expect。

## 7. 仍需本机先做的实现工作

本文先定义分段方案；以下脚本能力待实现后再分发给各机器：

1. `node scripts/rune-parity.mjs --shard N --of 4`：输出第 N 段的 task 清单与统计。
2. `node scripts/rune-parity.mjs --shard-status T###=done --file shards/segment-N.md`：机器写本段状态文件。
3. `node scripts/rune-parity.mjs --aggregate`：本机把 4 段状态汇总回主 tasklist。
4. 从主分支创建 4 个 segment 分支并 push，作为各机器起点。
