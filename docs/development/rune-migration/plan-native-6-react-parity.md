# 阶段六计划：React 前端逐文件对齐（可行性评估与迁移协议）

目标（用户指示）：把 `packages/frontend/src` 的 UI 逐文件迁到 Rune，**组件与页面结构不变、代码文件目录结构不变**；新增页面与组件一律以 `rune-kit` 为基座。

## 一、规模盘点

### React 侧（`packages/frontend/src`，1,677 个 TS/TSX 文件、343,528 行）

| 类别 | 文件 | 行数 |
| --- | --- | --- |
| UI 组件（非测试 `.tsx`） | 310 | 70,059 |
| 逻辑（非测试 `.ts`：stores/lib/hooks/commands/platform/ai） | 495 | 80,368 |
| 测试（`__tests__` + `*.test.ts(x)`） | 706 | 158,115 |
| 语言包（`locales/`） | 167 | 37,027 |

按页面与组件组（`.tsx` 行数）：

| 区域 | 文件 | 行数 | 区域 | 文件 | 行数 |
| --- | --- | --- | --- | --- | --- |
| `windows/connection` | 74 | 24,343 | `windows/data-transfer` | 10 | 3,226 |
| `windows/settings` | 28 | 7,228 | `windows/workflow` | 7 | 2,914 |
| `components/DataTable` | 25 | 5,578 | `windows/schema-diff` | 14 | 2,607 |
| `components/ai` | 13 | 4,461 | `windows/data-sync` | 20 | 2,567 |
| `windows/dashboard` | 6 | 2,166 | `windows/onboarding` | 10 | 1,255 |
| `components/connection` | 16 | 2,090 | `components/chart` | 11 | 1,288 |
| `components/filter` | 8 | 1,462 | `components/migration` | 4 | 989 |
| `components/history` | 3 | 675 | `components/sql-editor` | 1 | 667 |
| `windows/backup` | 2 | 661 | `windows/workspace` | 7 | 657 |
| `windows/wapps` | 2 | 622 | `components/query` | 5 | 592 |
| `components/MenuBar.tsx` | 1 | 467 | `components/ui` | 18 | 309 |
| `windows/main` | 2 | 186 | `components/schema` | 1 | 152 |

### Rune 侧（`crates/datazen-ui/src`，56 个 `.rs`、20,675 行，其中测试 7,153 行）

| 模块 | 行数 | 对应 React |
| --- | --- | --- |
| `views/`（12 文件 / 11 页） | 6,211 | 各 `windows/*` 页面壳 |
| `data_grid/` | 1,574 | `components/DataTable`（部分） |
| `sql_editor/` | 1,416 | `components/sql-editor` + CodeMirror 粘合层 |
| `ui/`（`theme/nav/field/mod`） | 536 | `components/ui` + 设计 token |
| `tests/`（13 文件） | 7,153 | 706 个前端测试的替代 |

已完成 UI 面约占 React `.tsx` 的 **19%**（13.5K / 70K 行），页面覆盖为 11 页壳 + 部分表格与编辑器。

## 二、逐文件映射规则

| React | Rune | 可行性 |
| --- | --- | --- |
| `windows/<area>/Foo.tsx` | `crates/datazen-ui/src/windows/<area>/foo.rs`（目录名可保留 kebab，模块名 `data_transfer`） | ✅ 物理 1:1 |
| `components/<group>/Bar.tsx` | `crates/datazen-ui/src/components/<group>/bar.rs` | ✅ 物理 1:1 |
| `index.ts`（barrel） | `mod.rs` + `pub use` | ✅ 1:1 |
| 一个文件多个小组件 | 同文件多个 `fn` | ✅ 1:1 |
| `styles.css` / Tailwind class | `ui/theme.rs` 常量 + `Kit` token | ✅ 无需逐文件 |
| `__tests__/Foo.test.tsx` | 内嵌 `#[cfg(test)] mod tests` 或按主题的 `tests/*.rs` | ⚠️ 不能 1:1（Rust 测试形态不同，706 文件不映射） |
| `Foo.tsx` 的 PascalCase 文件名 | Rust 必须 `snake_case.rs`（`non_ascii_idents = deny`，另受 `non_snake_case` 约束） | ⚠️ 需一张映射表 |
| `stores/*.ts`（zustand） | `AppState` 字段与方法（`state_def.rs`、`*_state.rs`） | ⚠️ 语义映射，不是文件映射 |
| `hooks/use*.ts` | 无 hooks；改 `AppState` 方法或 `Cx` 上的回调 | ⚠️ 语义映射 |
| `commands/*.ts`（20 文件） | 已由 `datazen-ui-api` 适配器 + `crates/datazen` 内核承担 | ✅ 无需迁移 |
| `locales/*`（167 文件） | `vendor/rune/crates/rune-i18n`（未接入） | ⚠️ 需先定 i18n 方案 |

## 三、三档分类

**T0 直接可迁（约 3.5K 行，5%）**：`components/ui`(309)、`MenuBar`(467)、`StatusBar`、`SqlCodeBlock`(187)、`components/schema`(152)、`components/admin`(278)、`components/history`(675)、`windows/main`(186)、`windows/onboarding`(1,255)。纯展示、无第三方库，kit 的 `Button/Badge/Card/List/Menu` 基本够用。

**T1 kit 可替换（约 20K 行，29%）**：`windows/settings`(7,228)、`components/DataTable`(5,578)、`components/connection`(2,090)、`components/filter`(1,462)、`components/query`(592)、`components/migration`(989)、`windows/wapps`(622)、`windows/workspace`(657)、`windows/backup`(661)。控件都能在 kit 找到对应物（`Table`/`Select`/`Switch`/`Checkbox`/`Dialog`/`Toast`/`Tree`），但 DataTable 的单元格编辑、列宽拖拽、右键菜单、剪贴板是自研，kit 只覆盖虚拟化与骨架。

**T2 换引擎（约 45K 行，64%）**：`windows/connection`(24,343，含 `TableView`/`QueryPanel`/`ConnectionPage`/`ImportDialog`/`PrivilegeView`/navigator 上下文菜单)、`sql-editor` 与 CodeMirror 粘合、`components/chart`(1,288)、`windows/workflow`(2,914)、`windows/dashboard`(2,166)、`windows/schema-diff`(2,607)、`windows/data-transfer`(3,226)、`windows/data-sync`(2,567)、`components/ai`(4,461)。文件可保留，但内部实现必然重写。

## 四、第三方依赖替换表

| React 依赖 | Rune 侧 | 结论 |
| --- | --- | --- |
| `@codemirror/*`（7 个包，peer） | rune `TextEdit` + `text_field`，kit `Highlighter`（`rune-highlights`） | ⚠️ 补全/诊断/搜索面板需自建；`sql_editor/` 现 1,416 行只到基础编辑 |
| `@tanstack/react-virtual` | `el/virtual_list.rs`、`el/virtual_rows.rs`、kit `Table` | ✅ 有对应物 |
| `recharts` | `rune::canvas(Shape)`（`vendor/rune/crates/rune/src/el.rs:325`） | ⚠️ 无图表库，坐标轴/tooltip/legend 自绘 |
| `@xyflow/react` + `@dagrejs/dagre` | 无 | ⚠️ 画布与布局算法自建（或移植 dagre） |
| `marked` | kit `Markdown`（`markdown` feature） | ✅ 更好用 |
| `lucide-react` | `crates/datazen-ui/src/icons.rs` 内联 SVG / kit `Glyphs` | ✅ 已有路径 |
| `zustand` | `AppState`（每帧重建，无 hooks/selector） | ⚠️ 派生值改成方法，注意重算成本 |
| `react-dom`（portal/context/suspense） | 无对应物；kit 浮层收 `&mut layer` | ⚠️ 架构差异，需按页面设计 |
| `xlsx` / `fflate` / `yaml` / `sql-formatter` / `html-to-image` | 多数属数据层，走内核或 Rust crate | — 非结构性 UI |

## 五、结构约束冲突（必须提前裁决）

1. **命名**：Rust 文件与模块必须 `snake_case`，React 的 `Foo.tsx` 只能映射成 `foo.rs`；需要一个映射清单（脚本生成）而不是靠人记。
2. **单文件 ≤800 行**（AGENTS.md）：React 侧有 **14 个 `.tsx` 与 8 个 `.ts` 超过 800 行**（`DataTransferWindow.tsx` 1,901、`TableView.tsx` 1,419、`WorkflowPage.tsx` 1,392、`AiChatPanel.tsx` 1,322、`DataTable.tsx` 1,008…）。Rust 版本只会更长，这些文件**必然要拆**，即 1:1 结构在这些点上会破——需要在计划里显式列出例外清单。
3. **测试形态**：706 个前端测试不能逐文件搬；建议保留「每页 1 个 Rust 模块 + 用例名沿用」的策略，用 headless 取证补视觉断言。
4. **公共入口门禁**：`@datazen/frontend` 的 `check-frontend-public-entry` 不适用于 rune crate；镜像目录要让 Rust 模块可见性有同等约束（否则等于把 `src/` 内部随意互相引用）。
5. **样式**：React 用 Tailwind 工具类，Rune 用 `Kit` token + `theme.rs`；迁移时不能逐 class 翻译，要按语义 token 归一。

## 六、分批与验收

| 批次 | 范围 | 规模 | 验收 |
| --- | --- | --- | --- |
| B1 | kit 可用 + parity 脚本 + T0 小件（`components/ui`、`MenuBar`、`StatusBar`、`SqlCodeBlock`、`schema`、`admin`） | ~1.5K 行 | 新目录骨架成型；`ui/` 手写控件被 kit 组件替代；每件一个测试 |
| B2 | `windows/settings` + `components/migration` | ~8.2K 行 | 12 个 section 与 React 逐项对齐；960×640 无越界 |
| B3 | `windows/connection` + `DataTable` + `connection`/`filter`/`query` | ~34K 行 | 导航树、表格、查询面板、导入/权限对话框逐页取证 |
| B4 | `components/ai` + `history` | ~5.1K 行 | 流式聊天、Markdown 渲染、历史面板 |
| B5 | `workflow` + `dashboard` + `chart` | ~6.4K 行 | 画布拖拽、图表自绘 |
| B6 | `data-transfer` + `data-sync` + `schema-diff` + `backup` | ~9.1K 行 | 向导步骤、差异视图 |
| B7 | `onboarding` + `wapps` + `workspace` + `main` | ~2.7K 行 | 首次启动、扩展页、工作区标签 |

统一口径：每批完成后跑 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib`、`cargo check` 三 crate、headless `--snap/--tree` 取证（1440×816 dark/light + 960×640），并让 parity 脚本报告「只剩 T2 未迁文件」。

## 七、工作量粗估

`70K` 行 TSX → Rust 约 `0.9–1.4×`，即 **65–100K 行**；当前已完成 13.5K 行。以单人节奏（含测试与取证）粗估 **18–22 周**；kit 组件复用可省 25–35%，主要省在 T1 的控件与 T2 的浮层/表格骨架。

## 八、前置基础设施

1. **kit 可用**：优先真依赖 `rune-kit`（需预取 `gungraun`/`resvg`/`rune-theme-opencode` 与 `wasmparser 0.245.1`）；本机离线不可解析时，同源复制所需模块（`kit.rs`/`actions.rs`/`text_input.rs`/`badge.rs`/`card.rs`/`field.rs`/`list.rs`，约 2.0K 行）并保持 API 一致。
2. **`ui/kit.rs` token 映射**：`tok::theme(dark)` → `Kit`（Palette/Elevation/Type/Radius/Motion/Glyphs），让 kit 组件一落地就是 DataZen 的配色。
3. **`scripts/rune-parity.mjs`**：把 `packages/frontend/src/**/*.tsx` 与 Rune 镜像目录做对照，输出未迁清单（供 CI 与计划跟踪），并把 PascalCase → snake_case 的映射固化下来。
4. **镜像骨架**：`crates/datazen-ui/src/{windows,components}/<area>/` 随迁移逐页长，**不预建空文件**（仓库禁止空壳）。
