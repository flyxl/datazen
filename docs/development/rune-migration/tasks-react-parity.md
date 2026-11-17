# React → Rune 逐文件迁移台账

本文件是执行台账，不是设计文档：任务由 `scripts/rune-parity.mjs` 从
`packages/frontend/src` 现况生成，改代码后重新生成即可，不要手改表。

## 1. 三条硬约束怎么落地

用户要求：① 每个 task ≤ 1 天；② 逐文件迁移；③ 迁移期允许短暂编译失败，但不能长期编译不过。

| 约束 | 机制 | 证据 |
| --- | --- | --- |
| ≤ 1 天 | 台账按工作量估算贪心装桶，单桶 ≤ 1.0 天；超过 1 天的单文件单独成 task 并标 `split`，按子组件再拆 | `node scripts/rune-parity.mjs` 的 252 个桶 |
| 逐文件 | 一 task 对应一到三个 React 文件，Rune 侧落在同一路径（`Foo.tsx` → `foo.rs`），提交信息里逐条列出 | 表内 `react file` 列 |
| 不长红 | 新文件在挂 `mod` 之前不参与编译；每 task 收尾跑 `scripts/rune-migrate-gate.sh`，红则当场修或整 task revert | 门禁 2.4s（check）/ 6.5s（tests），全环含构建截图约 16s |

编译红灯窗口的边界：

- 工作区允许红，**提交必须绿**。一个 task 的红窗上限是它的耗时（≤1 天），实测门禁 9 秒内出结果，所以实际操作中红窗是分钟级。
- 新页面先写文件、后挂 `mod`。`crates/datazen-ui/src/views/mod.rs` 与 `crates/datazen-ui/src/app.rs` 的 `Section` 分发是唯一开关点，切换提交里同时删除旧实现。
- 同时最多一个 task 处于「文件已写、未过门禁」状态；第二个 task 开工前必须把第一个推过门禁或 revert。

## 2. 台账口径

- 统计口径：`packages/frontend/src` 下非测试 `.tsx`，310 个文件 / 70,059 行。
- 分类：`P` 纯展示（可 1:1 直译）、`C` 连接型（文件 1:1，hook 重接 `AppState`）、`H` 依赖第三方引擎（CodeMirror / xyflow / recharts / xlsx / dagre / marked / react-virtual，实为同路径重写）、`X` 纯 re-export 壳（迁移后消失）。
- 工作量估算：P 0.3–0.8 天（按行数）、C 0.9–1.3 天、H max(2.0, 行数/250) 天、X 0 天。H 的天数只算「引擎之上的接线」，引擎本身在 §3 单独立项。
- 单轨总计 **252 tasks / 259.4 工作日**。

## 3. Phase 0：基座（不属于台账，必须先做）

台账假定设计系统已经存在。Rune 侧现在只有 `crates/datazen-ui/src/ui/{mod,theme,nav,field}.rs` 共 536 行，
而 React 页面通过 `packages/frontend/src/components/ui/*` 这层薄壳调用 `@datazen/ui` 的组件，
实测被引用次数：Button 92、Spinner 39、CopyableError 28、Dialog 27、Select 25、Input 22、ErrorBanner 10、
Checkbox 10、useCopyFeedback 7、Badge 6、ToolbarButton 5、ResultMessageDialog 2、ToolbarShell 2、Tabs 2、
LimitationsDialog 2、WebContextMenuHost 1、ConfirmDialog 1、Slider 1、Radio 1。

所以先补这 22 个基座 task（每个 ≤1 天），完成后再进 §4：

| # | task | 落点 | 验收 |
| --: | --- | --- | --- |
| T0-01 | Button（primary/secondary/ghost/danger × size × icon × disabled × loading） | `crates/datazen-ui/src/ui/button.rs` | 已落地 `d91ed3001`：四变体共用 36pt 高与同一文字基线，`the_button_variants_keep_one_height_and_baseline` 覆盖 |
| T0-02 | Spinner（小/中，tone，可 label） | `crates/datazen-ui/src/ui/spinner.rs` | 动画帧不改变占位矩形 |
| T0-03 | 图标集对齐 lucide 实际用例 | `crates/datazen-ui/src/icons.rs` | 缺失清单清零，未用图标删除 |
| T0-04 | CopyableError + 复制反馈定时 | `crates/datazen-ui/src/ui/copyable.rs` | 复制后 1.5s 复位 |
| T0-05 | Dialog 基座（scrim/标题/关闭/页脚槽/Esc/焦点归还） | `crates/datazen-ui/src/ui/dialog.rs` | 960×640 内完整可见，Esc 关闭 |
| T0-06 | Select（popup list、键盘上下、labels、无匹配） | `crates/datazen-ui/src/ui/select.rs` | 键盘选中与鼠标一致 |
| T0-07 | Input（label/hint/error 态包 TextField） | `crates/datazen-ui/src/ui/input.rs` | 错误态高度不跳变 |
| T0-08 | ErrorBanner（variant） | `crates/datazen-ui/src/ui/error_banner.rs` | 长文本换行不越界 |
| T0-09 | Checkbox + Radio | `crates/datazen-ui/src/ui/toggle.rs` | 焦点环与点击命中区 |
| T0-10 | Badge + Divider + Label | `crates/datazen-ui/src/ui/badge.rs` | 主题色取自 `tok::theme` |
| T0-11 | ToolbarShell + ToolbarButton | `crates/datazen-ui/src/ui/toolbar.rs` | 窄窗换行 |
| T0-12 | Tabs（受控 tab + 渲染回调） | `crates/datazen-ui/src/ui/tabs.rs` | 选中态与键位 |
| T0-13 | ConfirmDialog + ResultMessageDialog + LimitationsDialog | `crates/datazen-ui/src/ui/confirm.rs` | 三者共用 Dialog 基座 |
| T0-14 | Slider | `crates/datazen-ui/src/ui/slider.rs` | 拖拽到两端取值 |
| T0-15 | WebContextMenuHost（右键菜单宿主） | `crates/datazen-ui/src/ui/context_menu.rs` | 视口边缘翻转 |
| T0-16 | i18n：zh-CN 表 → `t()` 运行时（2,940 处 key / 2,222 唯一） | `crates/datazen-ui/src/i18n.rs` | 缺键回显 key 且不 panic |
| T0-17 | PathInput（路径补全 + PathPicker 回调） | `crates/datazen-ui/src/ui/path_input.rs` | 6 个调用点的键位 |
| T0-18 | TemporalValueInput | `crates/datazen-ui/src/ui/temporal.rs` | 日期/时间/日期时间三型 |
| T0-19 | JsonViewer + BinaryViewer（hex 行） | `crates/datazen-ui/src/ui/viewers.rs` | 大 payload 不爆栈 |
| T0-20 | VirtualTree + 树行算术（TREE_TOP_LEVEL/effectiveExpanded/searchedRowLevels/ariaLevelOf/indentOf） | `crates/datazen-ui/src/ui/tree.rs` | 1 万行滚动帧率与展开状态 |
| T0-21 | `cn()` 审计（257 处）确认在 Rune 侧无需对应物 | 文档 + `ui/theme.rs` | 逐处给出等价 token 或删除理由 |
| T0-22 | 基座回归：每组件 960×640 + 1440×816 几何断言 | `crates/datazen-ui/src/tests/components.rs` | 越界节点为 0 |

Phase 0 合计约 22 个工作日。它做完之前不碰 §4 的台账任务，否则每个页面都会手搓一遍组件。

## 4. 逐文件任务台账

台账已移出本文，单独维护在 [`tasklist.md`](tasklist.md)：**一个 task 一行，状态就是该行方括号里的一个字符**，
所以推进一个 task 只改一行，不会牵动任何汇总数字或其它行。

- 刷新行尾（React 源码变了才需要）：`node scripts/rune-parity.mjs --tasklist`，已存在的 T 编号保留原状态。
- 改状态：直接改该行 `- [ ]` → `- [x]`，或 `node scripts/rune-parity.mjs --status T001=done`。
- 看进度：`node scripts/rune-parity.mjs --progress`（现算，不写回文件）。

口径、分类与工作量估算见 §2；252 个 task、259.4 工作日单轨是 2026-11-17 的快照。

## 5. 执行规约

每个 task 的定义（DoD）：

1. Rune 文件落在与 React 同名路径（`windows/settings/SettingsPage.tsx` → `views/settings/settings_page.rs`），可以按 800 行上限拆模块，但不改目录语义。
2. `scripts/rune-migrate-gate.sh` 输出 `gate: PASS`。
3. 新页面挂进 `Section` 分发，`--tree` 快照里关键 `key` 存在。
4. 960×640 与 1440×816 两档无横向越界（复用 `crates/datazen-ui/src/tests/chrome_metrics.rs` 的断言模式）。
5. 一个 task 一个提交：`feat(rune-ui): port <react path>`；文档/台账变动单独 `docs(rune):`。

节奏：

- 每 5 个 task 跑一次全量：`scripts/rune-migrate-gate.sh` + headless 六页扫描。
- 每个 area（表内 `area` 列）收尾更新 `docs/development/rune-migration/progress.md` 一行。
- task 状态只改 [`tasklist.md`](tasklist.md) 里该行方括号的一个字符；行尾由脚本刷新，不手改。
