# 产品需求文档 (PRD)：DataZen 全景查询体验（Query Experience）增强规范

| 文档属性 | 详情说明 |
| :--- | :--- |
| **文档名称** | DataZen 全景查询体验（Query Experience）增强规范需求规格说明书 |
| **存放路径** | `docs/todo/query-experience-prd.md` |
| **所属版本** | v0.1.x 持续落地 → v0.2.0 形成闭环收敛 |
| **产品主题** | 汲取 DataGrip（语言智能、执行策略）与 Navicat（片段、美化、参数与运维工具）之长，构建开发者优先的 SQL-first 工作台 |
| **架构基准** | Tauri v2 + React 18 + CodeMirror 6 + TypeScript + Tailwind CSS 4 + Zustand；遵循 `AGENTS.md` 单文件限制与双版本架构 |
| **双版本划分** | **Host（开源社区版）**：基础体验、Snippet、Beautify、参数门控、执行策略、显式刷新、资产中心<br>**Pro（特权增强插件）**：深度 AST 诊断（波浪线 Linter）、外键智能 JOIN、语句高亮边框/Gutter、跳转定义 |
| **文档状态** | 正式设计发布版 (Approved PRD) |
| **发布日期** | 2026-09-08 |

---

## 目录
1. [背景、现状与竞品汲取策略](#1-背景现状与竞品汲取策略)
2. [总体架构与双版本正交边界](#2-总体架构与双版本正交边界)
3. [特权扩展点（Extension Points）契约演进](#3-特权扩展点extension-points契约演进)
4. [模块一：SqlEditor 编辑器核心与人体工学（Host 社区版）](#4-模块一sqleditor-编辑器核心与人体工学host-社区版)
5. [模块二：执行门禁、参数安全与执行监控（Host 社区版）](#5-模块二执行门禁参数安全与执行监控host-社区版)
6. [模块三：轻资产中心与结果集运维（Host 社区版）](#6-模块三轻资产中心与结果集运维host-社区版)
7. [模块四：深度语言智能与 AST 诊断（Pro 插件版）](#7-模块四深度语言智能与-ast-诊断pro-插件版)
8. [子代理实施分工与执行指南 (Playbook)](#8-子代理实施分工与执行指南-playbook)
9. [验收标准与质量门禁](#9-验收标准与质量门禁)

---

## 1. 背景、现状与竞品汲取策略

### 1.1 现状诊断与痛点矩阵
DataZen 当前的查询编辑工作区具备了执行、多结果集、参数绑定横条以及基于 AI 的行内诊断/解释能力。但在开发者高频编写与排错场景下，相比成熟工具存在以下 10 大体验断层：

| 痛点 ID | 现状问题描述 | 对标竞品差距 | 严重级别 |
| :--- | :--- | :--- | :--- |
| **Q1** | 常用模版重复输入，无法使用前缀代码块展开 | 缺少类似 Navicat Snippets 的模版与 `$1` 占位跳转 | P0 |
| **Q2** | 格式化硬编码为全部大写，不可配置且无法仅美化选区 | 缺少 Navicat 的缩进、大小写、折行可配及选区美化 | P0 |
| **Q3** | 未赋值参数在执行时被底层静默替换为 `NULL` 发送 | 严重隐患；缺少 Navicat 执行前未填参强制拦截弹窗 | P0 (Bug) |
| **Q4** | 执行逻辑单一，快捷键或按钮容易误执行整个大脚本 | 缺少 DataGrip 的执行策略（当前语句/最大语句/询问/全脚本） | P0 |
| **Q5** | 自动补全缺乏外键约束感知，编写 JOIN 关联条件费时 | 缺少 DataGrip 基于 Schema 关系的智能 JOIN 补全 | P1 |
| **Q6** | 写错表名/字段名在执行前无任何感知，依赖后端报错 | 缺少 DataGrip 的写时静态波浪线诊断（Unknown Object） | P1 (Pro) |
| **Q7** | 无法直接从 SQL 编辑器跳转到表结构或 DDL | 缺少 DataGrip 的 `Cmd+Click` / `F12` Go to Definition | P1 (Pro) |
| **Q8** | 外部表结构变更后，编辑器补全缓存没有显式刷新按钮 | 依赖隐式重启或切库；缺少 Navicat 显式“刷新补全”入口 | P0 |
| **Q9** | 批量执行多条 SQL 时仅返回整体总耗时，无法排查慢语句 | 缺少 DataGrip 的多语句分段独立耗时反馈 | P1 |
| **Q10** | 历史记录与收藏分散，无法直接打开新 Tab 或 Pin 结果 | 缺少 Navicat/DataGrip 的轻量查询资产中心与 Pin 标签保护 | P1 |

### 1.2 竞品汲取策略
- **汲取 Navicat**：低心智负担的常用操作与生产力工具（代码片段 Snippet、可配置美化 Beautify、参数化查询安全拦截、显式刷新 Schema 缓存、结果 Tab 固定 Pin）。
- **汲取 DataGrip**：精准的语言智能与执行控制（语句执行策略配置、写时波浪线静态诊断、外键关系推导、跳转到定义 Go to、多语句分段耗时）。
- **保持 DataZen 差异化**：本地优先、轻量开源（< 5ms 键入预算）、AI 行内诊断与原生 EXPLAIN 闭环、Driver Command 抽象架构。

---

## 2. 总体架构与双版本正交边界

DataZen 坚持“开源社区版保障核心人权与安全底线，Pro 插件沉淀特权深度语言智能”的架构正交原则：

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                            DataZen 查询体验架构全景                          │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. 顶部操作栏 (QueryEditorSection / ToolbarShell)                            │
│    ├─ [执行] 策略下拉 (当前语句 | 全脚本 | 询问)                             │
│    ├─ [格式化] 按钮 (调用 editorRef.formatDocument)                          │
│    ├─ [片段] 下拉菜单 (选择常用 SQL Snippet 并展开)                          │
│    └─ [刷新] 按钮 (显式清理元数据缓存 metadataCache.invalidateSession)        │
├─────────────────────────────────────────────────────────────────────────────┤
│ 2. 执行与参数安全门闸 (useQueryExecutionGate)                                │
│    ├─ 参数拦截：检测到未赋值参数时阻断执行，弹出填参引导 (防静默 NULL)       │
│    └─ 策略分发：计算 Target Snapshot (Selection > Statement > Whole Script) │
├─────────────────────────────────────────────────────────────────────────────┤
│ 3. SqlEditor 核心编辑器 (CodeMirror 6 Compartment 架构)                      │
│    ├─ [Host 基础层]                                                         │
│    │  ├─ CodeMirror Snippet 引擎 ($1 占位与 Tabstop 跳转)                    │
│    │  ├─ 可配置 Beautify 格式化 (选区/全文替换)                              │
│    │  ├─ 剪贴板 Paste as IN 与连接树拖拽 Drop Caret                          │
│    │  └─ 别名感知基础补全 (FROM/JOIN 别名列过滤)                             │
│    │                                                                         │
│    └─ [Pro 插件扩展层 (@datazen/extension-sql-editor-pro)]                  │
│       ├─ S4-A: 当前语句发光边框 (StatementFrame) & 行号运行按钮 (Gutter)     │
│       ├─ S4-B: 基于外键图谱的智能 JOIN 补全推荐                              │
│       ├─ S4-D: 悬停元数据 Tooltip & Cmd+Click 跳转对象定义 (Go to)           │
│       └─ S4-E [新增]: 写时静态波浪线诊断 (Unknown Table/Column Linter)       │
├─────────────────────────────────────────────────────────────────────────────┤
│ 4. 底部执行结果与资产中心                                                    │
│    ├─ 结果工作区：多语句分段耗时展示、结果 Tab Pin 钉住保护                  │
│    └─ 资产中心：收藏夹一键打开至新 Tab、历史记录全文过滤、Snippet 导入导出   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 为什么 Snippet 和 Beautify 归属 Host（社区版）？
1. **开发者体验底线**：现代代码编辑器将“常用模版展开（`sel*`）”和“代码缩进美化”视为基础文本编辑体验，放在 Host 可确保开源版口碑与生产可用性。
2. **零 AST 外部依赖**：Snippet 仅依赖 CM6 原生 `@codemirror/autocomplete`，Beautify 仅依赖 `sql-formatter`，完全不涉及数据库元数据缓存与复杂作用域解析，代码高内聚且极其轻量。
3. **Pro 专注 AST 级深度语言智能**：Pro 插件应集中攻坚写时波浪线诊断、外键图谱推导、语法装饰层等高技术壁垒能力。

---

## 3. 特权扩展点（Extension Points）契约演进

所有扩展点契约必须严格保持向后兼容（Non-breaking Change）。

### 3.1 `packages/extension-points/src/sqlEditorProEP.ts` 契约扩充
在 `SqlEditorProFeatures` 中新增写时波浪线诊断扩展插槽：

```typescript
export interface SqlEditorProFeatures {
  /** S4-A: statement frame + gutter run button extensions. */
  createStatementDecorations?: (opts?: SqlEditorProOptions) => Extension[];
  /** S4-C: Alt+Enter intentions + INSERT/function inlay hints. */
  createIntentionExtensions?: (opts: SqlEditorProOptions, refs: SqlEditorProOptions) => Extension[];
  /** S4-D: table hover tooltip + Mod/Cmd+Click navigation. */
  createHoverExtensions?: (opts: SqlEditorProOptions, refs: SqlEditorProOptions) => Extension[];
  /** S4-B: function signature help tooltip. */
  createSignatureHelpExtensions?: (databaseType?: string) => Extension[];
  /** S4-B: FK-aware JOIN completion source (returns null when not applicable). */
  createJoinCompletionSource?: (
    opts: SqlEditorProOptions,
    refs: SqlEditorProOptions,
  ) => CompletionSource | null;
  /** S5-A: paste-as-IN keymap + schema-tree drop caret. */
  createPasteExtensions?: (opts: SqlEditorProOptions) => Extension[];
  /** S5-A: context-menu group for paste-as-IN (null when unavailable). */
  createPasteAsInContextMenuItems?: () => SqlEditorProOptions | null;
  /** S5-B: Bind parameter panel renderer. */
  renderBindParamPanel?: (props: SqlEditorProOptions) => any;
  /** S5-B: Bind parameters hook. */
  useBindParameters?: (sql: string, options?: any) => any;
  /** Pro settings contributions. */
  settingsContributions?: ExtensionSettingsContribution[];

  // ── [新增] S4-E: 静态写时波浪线诊断 (Unknown Table / Column Linter) ──
  /**
   * 创建 Pro 深度写时诊断扩展 (返回 @codemirror/lint 插件).
   * 仅在 Pro 模式激活，提供毫秒级未知表名/字段名静态检查.
   */
  createLinterExtensions?: (
    opts: SqlEditorProOptions,
    refs: SqlEditorProOptions,
  ) => Extension[];
}

// fallbackFeatures 默认兜底：
const fallbackFeatures: SqlEditorProFeatures = Object.freeze({
  // ...既有项...
  createLinterExtensions: () => [],
});
```

### 3.2 `packages/extension-points/src/sql-editor/contracts.ts` 契约扩充
在 `SqlEditorHandle` 中新增命令式操作句柄，供顶部工具栏调用：

```typescript
export interface SqlEditorHandle {
  getSelection: () => string;
  toggleLineComment: () => void;
  insertAt: (text: string, pos?: number | null) => void;
  getDocumentVersion?: () => number;
  getDocument?: () => string;
  rawInsert?: (text: string, pos: number) => void;
  focus?: () => void;

  // ── [新增] 外部工具栏命令式控制 ────────────────────────────────
  /** 格式化当前选区（若无选区则格式化全文） */
  formatDocument?: () => void;
  /** 在当前光标处插入代码片段模版，并激活 Tabstop 占位符跳转 */
  insertSnippet?: (template: string) => void;
}
```

---

## 4. 模块一：SqlEditor 编辑器核心与人体工学（Host 社区版）

### 4.1 SQL Snippets 模版系统与 Tabstop 跳转 (P0)
- **核心逻辑**：
  - 新建模块 `src/components/sql-editor/snippets/`。
  - 定义 `SqlSnippetItem` 结构（`id`, `prefix`, `label`, `description`, `template`）。
  - 内置首期通用高频模版库：
    - `sel*`: `SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};`
    - `selc`: `SELECT ${2:columns}\nFROM ${1:table_name};`
    - `ins`: `INSERT INTO ${1:table_name} (${2:columns})\nVALUES (${3:values});`
    - `upd`: `UPDATE ${1:table_name}\nSET ${2:column} = ${3:value}\nWHERE ${4:condition};`
    - `del`: `DELETE FROM ${1:table_name}\nWHERE ${2:condition};`
    - `join`: `JOIN ${1:table_name} ON ${1:table_name}.${2:id} = ${3:other_table}.${4:fk_id}`
    - `count`: `SELECT COUNT(1) AS total\nFROM ${1:table_name};`
  - 使用 `@codemirror/autocomplete` 的 `snippetCompletion` 包装为 CompletionSource，与 Schema/Keyword 补全源协同生效。
  - 用户输入前缀并回车/Tab 展开后，光标落在 `${1:...}` 并高亮默认值；按 `Tab` 跳至 `${2:...}`，按 `Shift+Tab` 反向跳回。

### 4.2 可配置 Beautify 格式化与选区美化 (P0)
- **配置持久化**：
  - 在 `src/stores/settingsStore.ts` 扩展 `sqlFormatOptions`：
    ```typescript
    export interface SqlFormatOptions {
      keywordCase: 'upper' | 'lower' | 'preserve';
      indentStyle: '2spaces' | '4spaces' | 'tab';
      breakBeforeBooleanOperators: boolean; // AND / OR 换行
      linesBetweenQueries: number;         // 语句间空行数 (默认 1)
    }
    ```
- **选区与全文格式化引擎**：
  - 重构 `src/lib/sqlFormat.ts`：将上述选项与当前连接的 `databaseType` 映射传递给 `sql-formatter`。
  - 导出 `formatEditorDocument(view: EditorView, options: SqlFormatOptions)`：
    - **存在选区时**：仅格式化选区文本，保留选区高亮；
    - **无选区时**：格式化全文，并通过字符偏移计算锁定当前光标所在的相对语句行，防止视图跳动。
  - 支持快捷键：`Shift+Alt+F` (Windows/Linux) / `Shift+Option+F` (macOS)。

### 4.3 顶部工具栏命令式联动 (P0)
- **组件落点**：`src/windows/connection/query/QueryEditorSection.tsx`（或 ToolbarShell）。
- **新增交互控件**：
  1. **【格式化】按钮**：
     - 图标呈现排版样式，点击直接调用 `editorRef.current?.formatDocument()`。
     - Tooltip 显示快捷键（如 `Format SQL (Shift+Alt+F)`）。
  2. **【代码片段】下拉按钮**：
     - 点击弹出菜单，展示内置片段列表（带前缀与描述）。
     - 点击菜单项直接调用 `editorRef.current?.insertSnippet(item.template)`。
  3. **【刷新补全】按钮**：
     - 点击触发 `metadataCache.invalidateSession(dbSessionId)`，同步触发 `queryClient` 或连接元数据增量重拉，弹出成功 Toast。

### 4.4 剪贴板与拖拽人体工学 (P1)
- **Paste as IN Condition**：
  - 检测剪贴板内容（支持 Excel 单列复制、制表符分隔、换行分隔）。
  - 右键菜单提供 “Paste as IN Condition” 或快捷键触发。
  - 自动转义单引号并格式化为 `('val1', 'val2', 'val3')` 插入光标位置。
- **Drop Caret 与智能引号**：
  - 从连接树拖拽表名到编辑器时，渲染插入标线（Drop Caret）；
  - 根据连接方言适配引号（PostgreSQL/SQLite 使用 `"..."`，MySQL 使用 `` `...` ``，SQL Server 使用 `[...]`）。

---

## 5. 模块二：执行门禁、参数安全与执行监控（Host 社区版）

### 5.1 执行策略四模态（Execution Strategy - P0）
在 `settingsStore` 中增加 `sqlExecutionStrategy`，并在顶部工具栏提供快速切换器：
1. `current_statement`（默认推荐）：执行光标所在的当前语句。
2. `entire_script`：总是执行编辑区内的整段脚本。
3. `largest_statement`：当光标处于空白行或过渡区时，自动推导最邻近的主语句执行。
4. `ask`（交互确认）：当编辑器存在多条语句且无显式选区时，弹出轻量 ActionSheet 询问“执行当前语句 (Line X-Y)”还是“执行全脚本 (共 N 条)”。

### 5.2 参数安全校验与未赋值拦截（P0 Bug 修复）
- **现状风险**：`src/lib/sqlBindParams.ts:297` 将未填写的参数静默置为 `NULL`，直接导致错误数据或意料之外的删改。
- **门禁重构**：
  - 修改 `src/windows/connection/query/useQueryExecutionGate.ts`。
  - 在 `buildSnapshot` 阶段，解析待执行目标 SQL 所包含的所有参数 Descriptor。
  - 比对当前传入的 `paramValues`（或 BindParam 状态）。
  - 若存在必填参数未赋值：
    - **严禁静默执行**；
    - 阻断执行流程，弹出提示：“存在未赋值的查询参数：`:param`，请先填入参数值”；
    - 自动展开底部的 `BindParamPanel` 并将焦点设置至首个缺失参数的输入框。

### 5.3 多语句分段耗时（Per-statement Duration - P1）
- **数据通道**：
  - Rust 后端 `QueryExecutor` 在批量执行多语句时，记录每个 Statement 的开始与结束纳秒时间戳。
  - 在 Tauri IPC 返回的 `QueryResult` 中新增 `statement_timings: Vec<StatementTiming>`（含 `index`, `sql_snippet`, `elapsed_ms`, `affected_rows`）。
- **界面展示**：
  - 底部结果栏不仅展示总耗时（如 `Total: 120ms`），多语句批处理时在 Messages / Results Tab 头上悬停展开各分段耗时柱状列表，一目了然排查慢查询。

---

## 6. 模块三：轻资产中心与结果集运维（Host 社区版）

### 6.1 收藏夹（Favorites）直达新 Tab (P0)
- 在侧边栏“收藏”列表中，点击任意收藏项，提供默认主动作：直接以该收藏 SQL 在当前会话下开辟新的 `QueryPanel` Tab。
- 自动提取收藏标题作为 Tab 标题，并保持编辑器聚焦。

### 6.2 查询历史（History）全文检索与过滤 (P1)
- 历史抽屉新增即时搜索输入框，支持根据 SQL 关键字、执行时间范围、数据库名称进行模糊过滤。
- 解决历史与连接绑定的隔离性：侧边栏历史抽屉明确标识当前过滤范围为“当前连接”或“全部连接”。

### 6.3 结果集 Tab 钉住保护（Result Pin - P1）
- 在结果集 Tab（Result 1, Result 2）右侧提供 📌 Pin 图标或右键 “Pin Tab”。
- 被 Pin 的 Tab 在后续执行新查询时不会被自动回收或覆写，而是强制开辟新的 Result Tab，防止关键分析数据被冲刷丢失。

### 6.4 Snippets 与配置导入导出 (P2)
- 设置面板提供 `Export Settings & Snippets (JSON)` 与 `Import` 按钮，便于跨设备或团队内部分享常用 SQL 片段。

---

## 7. 模块四：深度语言智能与 AST 诊断（Pro 插件版）

本模块位于独立仓库/目录 `@datazen/extension-sql-editor-pro`，通过扩展点注入。

### 7.1 写时静态波浪线诊断（Unknown Object Linter - S4-E, P1）
- **机制与性能**：
  - 基于 `@codemirror/lint` 实现，设置 `delay: 300`（300ms 异步防抖）。
  - 严格遵守 **< 5ms 打字 CPU 耗时预算**：单文档超过 5,000 行或 100KB 时自动降级，仅诊断光标前后 200 行。
- **未知表名诊断（Unknown Table Diagnostic）**：
  - 从 `refs.modelRef.current` 提取当前语句处于 `FROM`, `JOIN`, `UPDATE`, `INTO` 子句中的表标识符。
  - 建立白名单过滤：CTE 别名、系统/虚拟表（如 `DUAL`）、临时表（`#temp`）。
  - 与 `refs.metadataSnapshotRef.current` 比对，若目标表在当前 Schema 确信不存在，标记黄色警告波浪线：
    - `Unknown table or view "${tableName}". Check schema or refresh cache.`
- **未知列名诊断（Unknown Column Diagnostic）**：
  - 仅在存在确定表限定符（如 `u.col_name`）时触发。
  - 解析出 `u` 对应 `users` 表后，若 `col_name` 不在 `users` 字段列表内，标记黄色警告波浪线。
  - 无限定符的裸列名不予粗暴报错，避免复杂 JOIN 场景下的误报。

### 7.2 基于外键图谱的智能 JOIN 补全推荐（S4-B, P1）
- **实现契约**：`createJoinCompletionSource`。
- **推荐算法**：
  - 当光标位于 `JOIN ` 或 `LEFT JOIN ` 之后时，从 `metadataSnapshot` 读取当前查询中已存在表的 `foreignKeys`。
  - 检索哪些表引用了当前表，或当前表引用了哪些表。
  - 优先在补全列表置顶推荐关联表名；
  - 用户选中表名后，自动使用 `snippet` 补齐 `ON target.id = source.target_id` 关联子句。

### 7.3 当前语句发光边框与行号运行按钮（S4-A, P1）
- **StatementFrame**：
  - 利用 CM6 `ViewPlugin` 与 `DecorationSet`，根据当前光标所在的语句范围 `from` ~ `to`，在代码行外围绘制轻微发光外框（Accent 色高亮）。
- **Statement Gutter**：
  - 在语句的首个非空有效行（Gutter）显示绿色运行小图标（▶）。
  - 点击仅触发该单条语句的执行门闸。

### 7.4 符号定义跳转与元数据探针（S4-D, P1）
- **Go to Definition**：
  - 用户按住 `Cmd/Ctrl` 悬停在表名/视图名上时，文本变为超链接下划线样式。
  - 点击或按下 `F12`：触发 `onNavigateToTable` 或 `onNavigateToDdl`，宿主弹出 DDL 抽屉或在连接树中选中该节点。
- **Hover Tooltip**：
  - 鼠标静止悬停在表名上 500ms，弹出浮窗展示表注释、预估行数、字段简表。

---

## 8. 子代理实施分工与执行指南 (Playbook)

为确保多 Agent 并行开发时不产生架构冲突，按仓库与目录划分两套独立的实施轨道：

### 轨道 A：Host 宿主实施方案（由主仓库 Agent 执行）
- **工作目录**：`datazen/`
- **任务清单**：
  1. **[EP 契约更新]**：在 `packages/extension-points/src/sqlEditorProEP.ts` 扩充 `createLinterExtensions`，在 `contracts.ts` 扩充 `SqlEditorHandle`。
  2. **[Snippets 引擎]**：在 `src/components/sql-editor/snippets/` 创建通用模版库与 CM6 补全适配。
  3. **[Beautify 增强]**：在 `settingsStore` 增加格式化配置，在 `src/lib/sqlFormat.ts` 支持选区/全文格式化及快捷键绑定。
  4. **[命令式句柄暴露]**：在 `SqlEditor.tsx` 的 `useImperativeHandle` 挂载 `formatDocument` 与 `insertSnippet`。
  5. **[顶部工具栏联动]**：在 `QueryEditorSection.tsx` 添加格式化、代码片段下拉、刷新补全缓存三大按钮。
  6. **[执行门闸安全修复]**：在 `useQueryExecutionGate.ts` 加入未赋值参数阻断校验，杜绝静默 NULL。
  7. **[挂载 Pro Linter Compartment]**：在 `editorExtensions.ts` 接入 `pro.createLinterExtensions`。

### 轨道 B：Pro 插件实施方案（由独立 Agent 在插件目录执行）
- **工作目录**：`packages/pro-extensions/sql-editor-pro/`（或独立插件代码仓）
- **任务清单**：
  1. **[Linter 模块创建]**：新建 `src/linter/createLinterExtensions.ts`，基于 `@codemirror/lint` 实现 300ms 防抖调度。
  2. **[未知表名检查]**：实现 CTE 白名单过滤与 Schema 元数据比对，输出 Warning Diagnostic。
  3. **[未知列名检查]**：实现限定符别名反查与列名校验。
  4. **[导出契约注册]**：在 `sqlEditorProEP` 的实现对象中挂载 `createLinterExtensions`。
  5. **[旅程与防抖单测]**：编写测试验证击键中间态不闪烁、大脚本安全降级、CPU 耗时 < 5ms。

---

## 9. 验收标准与质量门禁

| 验收项 | 验证方式 | 成功判定标准 |
| :--- | :--- | :--- |
| **Snippet 展开与跳跃** | 输入 `sel*` 并按回车展开 | 模版自动展开，光标聚焦表名，按 `Tab` 顺畅跳至条件，按 `Shift+Tab` 可回跳 |
| **Beautify 可配与选区** | 划选一段 SQL 按快捷键或工具栏按钮 | 仅选中部分按配置（如 2 空格缩进）美化，其余未选中代码保持原状 |
| **未赋值参数拦截** | SQL 含 `:id`，参数条留空并点击运行 | 执行被拦截，弹窗明确提示参数未赋值，未向后端发送任何 NULL 请求 |
| **显式刷新缓存** | 外部添加新表后点击顶部“刷新补全”按钮 | 弹出刷新成功提示，无需重启或重连，输入新表名即可被自动提示补全 |
| **执行策略分发** | 设置为 `ask` 模式并执行多语句脚本 | 弹出确认选项，可明确选择“仅执行光标所在语句”或“执行全脚本” |
| **写时波浪线诊断 (Pro)** | 输入 `SELECT * FROM table_not_exist` | 表名下方呈现黄色波浪线；在输入合法表名或刷新元数据后波浪线立刻消失 |
| **打字性能门禁** | 5,000 行大 SQL 脚本输入基准测试 | 持续击键过程中打字延迟稳定在 5ms 预算内，无明显卡顿或主线程阻塞 |
| **双版本兼容性** | 编译开源社区版（Community） | 无任何 TypeScript 报错，Pro 特性优雅退化为 Fallback 空数组，功能零异常 |
