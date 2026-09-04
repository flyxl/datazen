# SQL 编辑器模块设计与参考资料

本目录包含 DataZen IDE 级智能 SQL 编辑器的需求规格说明书（PRD）以及用于技术实现的参考代码库（来自对标开源项目 [t8y2/dbx](https://github.com/t8y2/dbx)）。

## 目录结构

```text
docs/todo/sql-editor/
├── README.md           # 本说明文件
├── prd.md              # DataZen IDE 级智能 SQL 编辑器系统需求规格说明书 (PRD)
├── implementation-plan.md # 面向协调代理、编码代理与测试代理的分阶段实施方案
└── dbx/                # dbx 仓库中与编辑器相关的核心源码参考
    ├── components/
    │   └── editor/     # 编辑器核心组件、搜索面板、AI 助手、参数与危险确认对话框
    ├── lib/
    │   ├── editor/     # CodeMirror 6 拓展体系（行内提示、语句边框、快捷键、悬停预览等）
    │   └── sql/        # 纯前端 SQL 语义 AST 模型、方言适配、分词器、格式化、执行目标计算
    └── types/          # 数据库、执行目标、参数等相关 TypeScript 类型定义
```

## 实施入口

- 产品范围与验收目标：[`prd.md`](prd.md)
- 阶段拆分、接口契约、子代理轨道与质量门禁：[`implementation-plan.md`](implementation-plan.md)
- `dbx/` 仅用于研究可观察行为、边界条件与性能策略；禁止在生产代码或测试中引用、复制或机械翻译其中的实现。

## 参考源码模块说明

### 1. `dbx/components/editor/`
- `QueryEditor.vue`: 主编辑器组件，集成 CodeMirror 6、语句边框、右键菜单、拖拽插入等。
- `AiAssistant.vue`: 智能 AI 助手侧栏，支持 Ask 模式与 Agent 自主模式。
- `EditorSearchPanel.vue`: 贴顶高级查找/替换面板，支持选区搜索与切片统计保护。
- `SqlParameterDialog.vue`: 参数化 SQL 变量绑定对话框与历史值记忆。
- `DangerConfirmDialog.vue`: 高危操作与生产环境静态安全拦截二次确认弹窗。
- `MultiDbExecuteDialog.vue`: 跨多数据库/Schema 批量分发执行对话框。
- `DelimitedListDialog.vue`: 剪贴板文本转换为 SQL `IN (...)` 列表配置。
- `ThemeCustomizerDialog.vue`: 编辑器 Token 语法高亮调色盘。

### 2. `dbx/lib/editor/`
- `codemirrorCurrentStatementFrameLayer.ts`: 当前语句发光/视觉外边框层。
- `codemirrorStatementGutter.ts`: 行号栏单语句独立运行按钮。
- `codemirrorInsertValueHints.ts`: `INSERT INTO ... VALUES` 列名内联提示（Inlay Hints）。
- `sqlIntentionActions.ts`: `Alt+Enter` 意图操作（`SELECT *` 展开为列列表、字段别名限定）。
- `hoverTableSql.ts`: 鼠标悬停显示表 DDL、字段元数据与内嵌快速过滤。
- `queryEditorTableDrop.ts`: 侧边栏对象拖拽进编辑器的插入光标指示线（Drop Caret）与方言智能引号。

### 3. `dbx/lib/sql/`
- `semantic/`: 纯前端容错语义 AST 模型（`model.ts`, `tokens.ts`, `dialect.ts`, `completion.ts`, `references.ts`），负责作用域推导、别名绑定、投影可见性。
- `sqlFormatter.ts`: 多方言代码格式化与压缩。
- `sqlRisk.ts`: 语句风险等级分类（Read / Write / DDL / Transaction）。
- `dmlChangePreview.ts`: DML 变更前自动转生成只读 SELECT 的 Diff 预览。
- `sqlInListPaste.ts`: 剪贴板数据一键智能组装为 `IN ('...', '...')`。
