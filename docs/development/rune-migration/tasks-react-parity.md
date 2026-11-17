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

基座**不再手搓**：`rune-kit` 已接入（根 `Cargo.toml` 的 `rune-kit` path 依赖 + `crates/datazen-ui/src/ui/kit.rs`
的 `OnceLock<Kit>`，`Kit::neutral(dark)` 由 `rune::tok::theme` 推导），Phase 0 只剩把现有面收敛到 kit
以及补 kit 缺的那几个件。状态就是 `☐` 列的一个字符，改它只动一行。

| # | ☐ | task | 落点 | 验收 |
| --: | :-: | --- | --- | --- |
| T0-00 | ☑ | kit 基座接入：workspace 依赖、`kit(dark)` 单例、对话框页脚改用 kit `Button` | `Cargo.toml`、`crates/datazen-ui/src/ui/kit.rs`、`crates/datazen-ui/src/views/modals.rs` | 已落地 `4054d0ada`：`gate: PASS`，页脚按钮 36pt、标签中心与盒中心差 0.00pt |
| T0-01 | ☐ | Kit 主题贯通：`ui/theme.rs` 的 `tok::Theme` 与 `Kit::Palette` 合并，收敛 74 处 `oklch/oklcha` 硬编码 | `crates/datazen-ui/src/ui/theme.rs` | 组件源码内不再出现 `oklcha(` 字面量，色值全经 `kit(dark)` |
| T0-02 | ☐ | 文本输入归一：`ui/field.rs` 收敛到 kit `TextInput`（label/hint/error 态） | `crates/datazen-ui/src/ui/field.rs` | 错误态出现时字段高度不跳变 |
| T0-03 | ☐ | i18n：zh-CN 表 → `t()` 运行时（2,940 处 key / 2,222 唯一） | `crates/datazen-ui/src/i18n.rs` | 缺键回显 key 且不 panic |
| T0-04 | ☐ | 图标接 `Kit::glyph`（check/chevron/close/copy/search/lock/eye…） | `crates/datazen-ui/src/icons.rs` | 同一语义不再有两套 SVG |
| T0-05 | ☐ | Dialog 基座：封装 kit `Dialog`（scrim/标题/关闭/页脚槽/Esc/焦点归还） | `crates/datazen-ui/src/ui/dialog.rs` | 960×640 内完整可见，Esc 关闭，焦点归还。已落地 `9efb4a567`；独立 tester 判 TEST_FAILED（scrim 30%→要 50%、重开保留滚动、无 panel 阴影），修复中 |
| T0-06 | ☑ | Select：封装 kit `Select`（popup list、键盘上下、placeholder、禁用） | `crates/datazen-ui/src/ui/select.rs` | 键盘选中与鼠标选中的结果一致。已落地 `59c783199` + `af66e5148`（label→value 映射、`on_close` 才挂 dismissal 层），tester 判 TEST_PASSED |
| T0-07 | ☐ | Menu：封装 kit `Menu` / `context_menu`（右侧 [0,0] 锚定、边缘翻转） | `crates/datazen-ui/src/ui/menu.rs` | 视口边缘不裁切。coder 完成 `dd65b96d7`（184 行基座 + 6 例），opencode tester 复核中 |
| T0-08 | ☑ | 开关类：`Checkbox` / `Switch` / `RadioGroup` 封装 | `crates/datazen-ui/src/ui/toggle.rs` | 焦点环可见，整行可点。已落地 `59c783199` + `af66e5148`（Checkbox 半选态），tester 判 TEST_PASSED |
| T0-09 | ☐ | Table / Tree 采用 kit（`Table` + `Tree`/`FileTree`） | `crates/datazen-ui/src/data_grid/`、`crates/datazen-ui/src/ui/tree.rs` | 1 万行滚动帧率不退化 |
| T0-10 | ☐ | Toast / Tooltip / HoverCard | `crates/datazen-ui/src/ui/overlay.rs` | 与 kit 动画时长一致。opencode coder 进行中（T0-10 轨 `feature/rune-overlay`） |
| T0-11 | ☐ | kit 缺失件：Slider、CopyableError、ErrorBanner、JsonViewer/BinaryViewer、PathInput、TemporalValueInput、虚拟树行算术 | `crates/datazen-ui/src/ui/*.rs` | 每件一条几何/行为断言 |
| T0-12 | ☐ | 基座几何回归：每组件 960×640 + 1440×816 | `crates/datazen-ui/src/tests/components.rs` | 越界节点为 0 |

Phase 0 合计约 12 个工作日。它做完之前不碰 §4 的台账任务，否则每个页面都会再手搓一遍组件。

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
