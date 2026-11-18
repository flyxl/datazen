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
| T0-01 | ☑ | Kit 主题贯通：`ui/theme.rs` 的 `tok::Theme` 与 `Kit::Palette` 合并，收敛 74 处 `oklch/oklcha` 硬编码 | `crates/datazen-ui/src/ui/theme.rs` | 组件源码内不再出现 `oklcha(` 字面量，色值全经 `kit(dark)`。已落地 `9490c10ae`（`ui/palette.rs` 集中语义色 + `tests/tokens.rs` 走查），tester 判 TEST_PASSED |
| T0-02 | ☑ | 文本输入归一：`ui/field.rs` 收敛到 kit `TextInput`（label/hint/error 态）。已落地 `e6e9a3a0f`（合流 `5acff13f3`；外框 padding 可聚焦、28/36pt 居中差 0.0pt、23 处生产链全带 `on_edit`），tester `rune-i18n-test` 判 TEST_PASSED（审计 `9db4e510e` + `c5cc56bf9`）；遗留 `centered_pad` 已无调用者、`settings_state::dark_theme()` 仅测试用 | `crates/datazen-ui/src/ui/field.rs` | 错误态出现时字段高度不跳变 |
| T0-03 | ☑ | i18n 基座 + 三批接管：zh-CN 表 → `t()` 运行时（3,217 键，`build.rs` 生成，`crates/datazen-ui/src/i18n.rs` 二分查）；批次一连接视图 `cb8e3f2b0`+`ad766ba5c`+`da19183ad`，批次二设置/rail/窗口标题 `dfbe30895`+`3b6646164`+`b28a4d36d`，批次三 10 个视图 71 处 / 58 键 `62c4e00ae`+`1536d28b6`+`e17da8c34`→修复 `e8b5c1090`（Explain 用 `explain.title`、首页计数/空态、`connWin.fields`、`common.save`/`selectAll`/`newQuery`、`Disconnected`→「未连接」）→复测 `cb1c97f44`→合流 | `crates/datazen-ui/src/i18n.rs`、`crates/datazen-ui/src/tests/{i18n_usage,i18n_sweep,i18n_batch3_audit}.rs` | 三道守卫（缺键回显不 panic；反查表按 token 扫 `t("…")` 实参并可计数；按 `(file,key,次数)` pin 调用点）；残留：12 条中文无等价 key、`Refresh` 6 处与 `conn.disconnected` 待新增通用 key |
| T0-04 | ☑ | 图标唯一来源：`icons.rs` 25 个 lucide 24-view `ICON_*`，守卫 `tests/icon_registry.rs` 三条（除 `icons.rs`/`src/tests/` 外出现 `Icon {` 即失败、body 不得重复、`ICON_*` 必须 24-view 且非空 body）（`21721f792`→`f0e611439`→`3aef8c9ce`，审计 `TEST_PASSED`）；chrome 不换 kit 16-view 画风（kit glyph 只用于 kit 组件内部）；spinner 的 `ICON_LOADER2`/`ICON_RING_ARC` 搬进注册表（`b408f2e9b`） | `crates/datazen-ui/src/icons.rs` | 守卫覆盖词边界，`-> &'static MyIcon` 不误报；文本级别名/字符串为已文档化残差 |
| T0-05 | ☑ | Dialog 基座：封装 kit `Dialog`（scrim/标题/关闭/页脚槽/Esc/焦点归还） | `crates/datazen-ui/src/ui/dialog.rs` | 960×640 内完整可见，Esc 关闭，焦点归还。已落地 `9efb4a567` + `6ba7eedc6`（scrim 0.5、`<key>-content-<epoch>` 重置滚动、kit overlay 阴影、标题截断），tester 判 TEST_PASSED |
| T0-06 | ☑ | Select：封装 kit `Select`（popup list、键盘上下、placeholder、禁用） | `crates/datazen-ui/src/ui/select.rs` | 键盘选中与鼠标选中的结果一致。已落地 `59c783199` + `af66e5148`（label→value 映射、`on_close` 才挂 dismissal 层），tester 判 TEST_PASSED |
| T0-07 | ☑ | Menu：封装 kit `Menu` / `context_menu`（右侧 [0,0] 锚定、边缘翻转） | `crates/datazen-ui/src/ui/menu.rs` | 视口边缘不裁切。已落地 `dd65b96d7` + `c5dd84922`（子菜单、icon/shortcut/badge/description/checked/danger 透传、`close_on_select`），tester 复测通过；长菜单超窗为 kit 限制，记 `#[ignore]` |
| T0-08 | ☑ | 开关类：`Checkbox` / `Switch` / `RadioGroup` 封装 | `crates/datazen-ui/src/ui/toggle.rs` | 焦点环可见，整行可点。已落地 `59c783199` + `af66e5148`（Checkbox 半选态），tester 判 TEST_PASSED |
| T0-09 | ☑ | Table（`1c5d8e9d4`，合流 `14948e4f8`）+ Tree（`abb1ef2cf`，复测 `12bb3f1f9`，合流 `1279b9197`）已落地；都借 kit 的样式/模型自绘（kit `Table`/`Tree` 的行键私有硬编码），Tree 有虚拟化、Table 暂无；Tree 的 a11y 用 `active_descendant` 指向可见行、页高按实例存 `S` | `crates/datazen-ui/src/data_grid/`、`crates/datazen-ui/src/ui/{table,tree}.rs` | 1 万行滚动帧率不退化 |
| T0-10 | ☑ | Toast / Tooltip / HoverCard | `crates/datazen-ui/src/ui/toast.rs`、`ui/tooltip.rs` | 与 kit 动画时长一致。已落地 `271a95b48` + `86818558a`（`ToastHost` 持 `ToastStack` 走 `tick`，HoverCard 锚点贴合触发器），tester 复测 TEST_PASSED；`ToastHost` 尚无真实调用点，动效与帧时钟待接线 |
| T0-11 | `~` | kit 缺失件：CopyableError+ErrorBanner（`c02669bd9`→`28fba0cef`；颜色/a11y 补强 `4170cb0b8`→`e3e99478c`；1500ms copied 反馈 `b51a39b81`，时长与 wake 由审计 `28bed6d04` 用 `wake_deadline` 钉住）、Slider/PathInput（`df6e8853b`→`4773d96b0`/`c0f11bbb1`）、JsonViewer/BinaryViewer（`1f2e43aef`→`b811a5f01`）、compact Tooltip 封顶 320pt（`270554897`→`549a823b9`→合流 `6eb53e53f`）、Spinner/Badge/Tabs/ToolbarButton/ToolbarShell/Label（`8754d97f8`；审计 `18bac0795` 判 Tabs 缺选中语义，修法是用 `El::focus_selected`——`aria-controls` 属 rune 上游缺口，`el/a11y.rs` 无 `controls()`）已落地；TemporalValueInput（`464bbe786`→D1 修复 `1adb84aac`→审计 `ad72deb76` 通过→合流 `a0c81ff1a`）、虚拟树行算术（`9abc9cb8f` 三函数 + 外部 select reveal 修复 `abfda0e7e`→`47f01f8d0` 心跳使能，审计 `be4194461` 判实现正确并证伪「build 快照是因果」，合流 `87652aa52`） | `crates/datazen-ui/src/ui/*.rs` | 每件一条几何/行为断言 |
| T0-12 | ☑ | 基座几何回归：16 个基座 × {960×640, 1440×816} × {dark, light} = 64 次渲染，越界 0、同父兄弟无重叠（`23e0db186`，合流 `3ec5a6ba8`）；顺带发现 compact Tooltip 长单行文本不换行会横向出窗（`ui/tooltip.rs:105` → kit `overlay.rs:255` 的 `max_w: None`），记为待修 | `crates/datazen-ui/src/tests/base_geometry.rs` | 越界节点为 0 |

Phase 0 合计约 12 个工作日。它做完之前不碰 §4 的台账任务，否则每个页面都会再手搓一遍组件。

## 3.5 台账盲区：`.ts` 基础层

§2 的口径只统计非测试 `.tsx`（310 文件 / 70,059 行），**不含 `.ts`**；而 `.tsx` 大量依赖 `.ts`，
所以台账跑到 DataTable / 连接页主体时会被基础层卡住。实测（2026-11-18）：

| 区域 | 文件 | 行数 | 说明 |
| --- | --- | --- | --- |
| `lib/` | 168 | 27,019 | 纯函数与工具，DataTable 的直接阻塞点 |
| `windows/connection/` | 45 | 10,064 | 连接页的 hook / 选择器 / 派生状态 |
| `stores/`（含 `stores/tableData`、`stores/ai`） | 58 | 12,652 | 状态机，Rune 侧并入 `AppState` 的 slice |
| `hooks/` | 26 | 3,542 | 视图胶水，多数在 immediate mode 下消失或并入 slice |
| `commands/` | 23 | 3,311 | IPC 外壳，Rune 侧走 `datazen::application` |
| `components/sql-editor/` | 19 | 3,338 | 补全/高亮配置，属 CodeMirror 引擎批次 |
| `types/` | 10 | 2,041 | 契约镜像，`backend-client` 已有 Rust 版本 |
| 其它 | 41 | 8,354 | `platform/`、`windows/{data-sync,schema-diff}` 等 |
| 合计（不含 locale 包） | **495** | **80,321** | 另有 10 个 locale 包约 36k 行，只喂 `build.rs`，不迁 |

建议顺序（每一批都要能在门禁绿的前提下独立收尾）：

1. **`lib/` 里的纯函数子集**（约 1.6k 行）：`formatters.ts`、`cellTypes.ts`、`cellEditorKind.ts`、
   `dataTypeColors.ts`、`editors/toEditString.ts`。这五个是 `CellRenderer` / `GridCell` / `DetailPanel`
   的前置条件，没有它们只能先做 `ColumnStatsDialog` 这类不依赖类型表的叶子组件。
2. **`stores/` 按 slice 归位**：`tableDataStore`(935) → `schemaStore`(727) → `panelStore`(766) →
   `aiStore`(1,209)。Rune 侧已有的 8 个 slice（`settings_state`/`workspace_state`/…）是落点，
   不需要一一对应新 slice。
3. **`commands/` 与 `types/`**：多数在 Rune 侧已有对应 `application` 用例与 `shared` DTO，
   属于「接线核对」而非迁移，只在发现的缺口处补。
4. **`hooks/`**：随所属视图一起做，不单独成批（`useMemo`/`useCallback` 直接消失）。
5. **`components/sql-editor/`**：并入 CodeMirror 引擎批次（与 §2 的 24 个 `H` 类文件同批）。

编号方式：`scripts/rune-parity.mjs` 现在扫两遍，先 `.tsx`（沿用 T001–T252 的历史编号，不重排），
再 `.ts`（T253 起，按 `lib → types → stores → hooks → commands → platform` 排）。
`.ts` 的第一批五项 `lib/formatters.ts`、`lib/cellTypes.ts`、`lib/cellEditorKind.ts`、
`lib/dataTypeColors.ts`、`components/DataTable/editors/toEditString.ts` 被钉在最前（T253–T256，约 2.5 天），
因为 `CellRenderer`/`GridCell`/`DetailPanel`/`VirtualBody` 没有它们无法开工。
状态仍只改方括号一个字符；行尾由 `--tasklist` 刷新，已有 T 编号的状态保留。

## 4. 逐文件任务台账

台账已移出本文，单独维护在 [`tasklist.md`](tasklist.md)：**一个 task 一行，状态就是该行方括号里的一个字符**，
所以推进一个 task 只改一行，不会牵动任何汇总数字或其它行。

- 刷新行尾（React 源码变了才需要）：`node scripts/rune-parity.mjs --tasklist`，已存在的 T 编号保留原状态。
- 改状态：直接改该行 `- [ ]` → `- [x]`，或 `node scripts/rune-parity.mjs --status T001=done`。
- 看进度：`node scripts/rune-parity.mjs --progress`（现算，不写回文件）。

口径、分类与工作量估算见 §2。台账分两层：T001–T252 是 `.tsx`，T253 起是 §3.5 的 `.ts` 基础层。
2026-11-18 快照是 **629 个 task / 626.7 工作日单轨**（`.tsx` 252 个 / 259.4 天，`.ts` 377 个 / 367.3 天）。

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
