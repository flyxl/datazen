# DataZen IDE 级智能 SQL 编辑器实施方案

> 基于 `docs/todo/sql-editor/prd.md` v2.0 制定。本文是面向协调代理、编码子代理和测试子代理的可执行交付方案，不替代 PRD。

## 0. 文档目标与实施原则

本方案把 PRD 拆成可独立开发、可独立测试、可安全合流的工作轨道。每个轨道都明确前置条件、文件写权限、实现步骤、测试和完成定义，使子代理无需自行补猜架构决策。

实施必须遵守以下总原则：

1. DataZen 现有执行链路、结果工作区、AI Chat、Driver Command API 和 Safe Mode 是主干，只增强，不替换。
2. 所有键入期分析均在前端本地快照上完成；允许外层异步预取元数据，但 CodeMirror completion、hover、inlay 回调不得直接发 IPC。
3. 前端语句范围、轻量语义、风险识别和参数识别共享同一套 TypeScript 词法基础，禁止各写一套不一致的扫描器。Rust Host 保持独立安全扫描实现，通过共享 JSON fixture 保证行为一致，不尝试跨语言运行时复用。
4. 行为差异由 `DB_REGISTRY` / `DatabaseTypeMeta` / 方言适配器驱动，Host 代码不得按具体驱动 ID 写分支。
5. `connectionId` 仅表示持久化连接配置，`dbSessionId` 仅表示运行时会话；缓存、拖拽、执行、历史存储不得混用。
6. 单个生产源码文件必须少于 500 行，推荐 200～300 行。当前超长文件先做等价拆分，再接入新能力。
7. Host UI 的新增或变更路径必须有 E2E；驱动方言或驱动专属 UI 测试必须放到对应驱动目录。
8. 默认只改英文和中文翻译域文件；发布前再统一同步其他语言。
9. 所有安全确认只是前端体验门，Rust Host 的 `readOnly` 和 Safe Mode 仍是最终防线。
10. 不在本项目中实现跨库批量分发、独立 AI 侧栏、完整 SQL 编译器或新的结果集协议。

## 1. `dbx` 参考边界：可借鉴，但不得照抄

`docs/todo/sql-editor/dbx/` 只用于研究可观察行为、边界条件和性能策略。它不是依赖包，也不是可移植源码。

### 1.1 可以借鉴的内容

- 当前语句边框在超长语句上的降级思路。
- gutter 使用“首个可执行行”而非语句起始空白行。
- inlay hint 只分析 viewport/活动语句，并采用 debounce 和缓存。
- Paste as IN 的输入体积、条目数上限及 quote-aware 分隔行为。
- 参数历史的版本化、去重、损坏数据回退和数量限制。
- 拖拽 payload 区分 table、column、multi-table 的行为设计。
- 意图分析与 CodeMirror transaction 应用分离；语义歧义时不提供动作。
- 大文档使用活动语句窗口、viewport、缓存和延迟计算的组合。

### 1.2 必须按 DataZen 架构重新设计的内容

- React + Zustand 的组件与状态组织。
- `DB_REGISTRY`、Driver metadata、`schemaStore` 和 `getTableSchema` 元数据路径。
- `panelStore`、`queryExecActions`、流式执行和多结果集路径。
- `connectionId` / `dbSessionId` 的生命周期和隔离。
- Rust Host 中的 typed literal 参数替换与 Safe Mode。
- `AiChatPanel`、`aiStore`、现有诊断上下文与脱敏逻辑。
- Web Context Menu、DataZen 主题 token、i18n 和 E2E 规范。

### 1.3 明确禁止

- 禁止生产代码或测试 import `docs/todo/sql-editor/dbx` 下的任何文件。
- 禁止把 Vue 文件机械翻译为 React，或镜像其文件名、符号名、注释、CSS、测试 fixture 后局部改名。
- 禁止复制其中的大型解析器、参数替换器、AI Assistant 或确认弹窗。
- 禁止引入本 PRD 不要求的 Mongo/ES/MyBatis/Oracle trigger、raw SQL 参数、跨库分发等能力。
- 禁止以前端字符串拼接替代 DataZen Host 的参数安全转义。

每个编码轨道的简报都必须包含一份“行为映射”：PRD 可观察行为、DataZen 现有落点、独立实现方案、明确不采用的 dbx 结构。测试代理必须检查仓库中不存在 dbx import 和机械翻译痕迹。

## 2. 现状基线与必须先解决的差异

### 2.1 已有能力，禁止重复建设

- `SqlEditor` 已支持：有选区时 `Mod+Enter` 执行选区；无选区时用 `getStatementAtCursor` 执行当前语句；`Mod+Shift+Enter` 执行全脚本。
- 流式执行、多 statement result、结果 Tab、取消、历史、事务检查、EXPLAIN 和结果工作区已经存在。
- `QueryErrorPanel` 已有 Retry、Explain/Fix SQL，`DiagnosisPanel` 已有一键应用修复。
- 表节点已能通过 `application/datazen-table` 拖入编辑器，落点已使用 `posAtCoords`，空编辑器会生成查询 SQL。
- `buildEditorSchema` 和 `contextualSchemaCompletion` 已提供基础的表/列上下文过滤。
- Rust `sql_guard` 已在 `readOnly` 模式阻止全部写操作，并在 Safe Mode 阻止 DROP、TRUNCATE、无 WHERE 的 UPDATE/DELETE。
- 多库或多 schema 调度继续归 Workflow，不进入本项目。

### 2.2 结构性问题

- `src/components/SqlEditor.tsx` 已超过 500 行。
- `src/windows/connection/QueryPanel.tsx` 超过 1700 行，同时承担编辑器装配、执行门、参数、安全、诊断、结果等职责。
- `ContentView.tsx`、`UnifiedSchemaTree.tsx`、`schemaStore.ts` 也都是热点大文件。
- `sqlStatementRange.ts` 和 `sqlTransactionGuard.ts` 存在两套语句扫描逻辑；后者才支持 PostgreSQL dollar quote。
- `schemaStore.columnMap` 只有列名，且裸表名 key 会造成不同 schema 同名表碰撞，无法支撑类型、注释、外键、索引和 hover。
- 参数前端仅支持 `:name` / `$1`；Host object payload 仅替换 `:name` / `$name`，`?` 只支持数组，而 UI 永远发送 object。
- `aiChatOpen` 和 Chat 输入都在 `ContentView` / `AiChatPanel` 内部，`QueryPanel` 没有打开并注入草稿的桥。

### 2.3 PRD 技术假设的校正

- “零后端 IPC”解释为键入、补全、hover、inlay 的同步回调不发 IPC；元数据仍需由外层按需加载并缓存。
- 顶部 Execute 按钮保持当前“选区优先，否则全脚本”的行为；本项目只保证快捷键和 gutter 精准执行当前语句，除非产品另行要求改变顶部按钮。
- 20,000 行与 `<5ms` 必须定义固定 fixture 和统计方法；不把易抖动的绝对墙钟断言放入普通单测。
- 当前 `TableSchema` 没有可靠的表注释字段。首版 hover 在无表注释时隐藏该行，不伪造数据；若要求所有驱动都显示表注释，必须单列 Driver API 升级项目并同步协议版本及所有插件。
- SQL Server 当前 registry 引号元数据与 PRD 的 `[]` 不一致。通用实现要支持 bracket quote；SQL Server 元数据修正和测试只能落在该驱动包。

## 3. 冻结的架构决策

以下决定应在编码开始前冻结，子代理不得自行改变。

### 3.1 执行语义

- `Mod+Enter`：非空选区 > 活动语句；找不到有效语句时不执行并给轻提示。
- `Mod+Shift+Enter`：执行全脚本，不赋予“新 Tab”第二种含义。
- gutter：执行 marker 对应的精确原文范围。
- 顶部 Execute：保留选区优先、否则全脚本。
- 参数面板解析全文并保留全部 descriptor/value，同时突出 primary cursor 所在语句的参数；真正执行时，gate 只筛选和校验 `ExecutionSnapshot.sql` 中出现的参数。这样顶部“执行全文”和快捷键“执行当前语句”都不会缺少输入。
- `SqlEditor` 必须上报活动语句/选区变化，供参数面板突出显示，但光标移动不清除其他语句的已填值。
- 每次请求冻结 SQL、range、panel、connection/session、安全配置和参数快照；最终提交只能使用该 snapshot，禁止调用会重新读取当前全文的旧入口。确认完成后任一关键字段改变就返回 `stale`，禁止“确认 A、执行 B”。
- 保存最近一次真正提交的 execution snapshot；Retry 从失败请求的实际 SQL/source/range/context 构建新请求，并重新读取当前安全配置、重新校验参数和重新通过 gate，不回退为当前全文。
- execution gate 只覆盖编辑器 query submit：顶部执行、selection/current shortcut、gutter、execute-all、错误 Retry、未闭合事务 Continue 后的提交。单独的 EXPLAIN、BEGIN/COMMIT/ROLLBACK 控件、导出、AI 应用 SQL（仅写入编辑器）保持各自现有入口；如果未来要纳入 gate，必须另建需求和安全矩阵。

### 3.2 AI 联动

- “Ask in Chat”只打开现有 `AiChatPanel`，注入脱敏、有界的草稿并聚焦输入框。
- 首版不自动发送。用户再次点击发送才产生 AI 网络请求，避免无意外发 SQL 或凭据。
- 未配置 AI 时仍打开现有配置引导，并保留一次性草稿；不创建新会话引擎，不清空既有聊天历史。
- 草稿不得包含 connectionId、dbSessionId、参数历史、凭据、原始连接串或结果行。

### 3.3 安全确认组合

- `readOnly=true` 且命中非只读：gate 直接返回 `blocked`，Host 仍保留最终硬阻止；不显示可绕过的确认。
- `safeMode=true` 且命中 DROP/TRUNCATE/无 WHERE UPDATE/DELETE：gate 直接返回 `blocked`，Host 仍保留最终硬阻止；不先弹确认再让 Host 拒绝。
- `safeMode=false` 且命中高危 finding：显示高危确认。
- `group === PRESET_GROUPS.production` 且 classification 不是 `read`：显示生产确认，覆盖普通写、高危写和 unknown。
- 同一执行同时命中“生产 + 高危”时合并为一个最高等级确认，列出全部原因，避免连续弹窗。
- SELECT/SHOW/DESCRIBE/普通 EXPLAIN 不因 production 弹窗；`EXPLAIN ANALYZE`、CALL/EXEC、SELECT INTO、WITH 后写语句按可能写入处理。
- production 只依据稳定 key `preset:production`，不依据显示文字、颜色、连接名或自定义组名猜测。

### 3.4 参数契约

- 五类 syntax：`colon`、`at`、`dollar-positional`、`question`、`template`。
- `:id`、`@id`、`${id}` 使用同一 value ID `named:id`，但每个 occurrence 保留原始 syntax 和 span。
- `$1` 使用 `dollar:1` ID；每个被识别为 placeholder 的 `?` 使用 `question:1`、`question:2` 等独立 ID；两者不隐式共享。
- 新 wire payload 为版本化 object：`{ version: 2, values, occurrences }`。`values` 使用上述稳定 ID；`occurrences` 带 `from/to/id/token`。Host 必须验证 span 有序、不重叠、原文 token 完全匹配、位于可替换词法区且每个 occurrence 有值，再按倒序替换。额外 value、非法 ID、缺值或失配 span 返回明确错误，不静默忽略。
- Host 暂时兼容旧 Record payload：命名参数依次查旧 bare name，`$n` 查旧数字 key；新 UI 一律发送 v2，不再依赖旧 lookup。
- `${name}` 仍绑定 SQL literal，不允许把值当标识符或 raw SQL 注入。
- `@name`、`?` 的识别由前端方言 adapter 控制，避开 `@@` 和 PostgreSQL `?` / `?|` / `?&` 运算符；Host 不重新猜测方言，只验证前端传来的精确 occurrence span。
- 同一 batch 中由顶层 `DECLARE @name` 或赋值型 `SET @name =` 引入的变量，按 adapter 的大小写规则从 bind descriptor 中排除；TS/Rust fixture 固定该行为。无法证明是 placeholder 时宁可不绑定。
- Host 验证/替换必须跳过注释、字符串、quoted identifier 和 dollar-quoted body，并在替换完成后再进行风险检查；execute 和 streaming 使用同一个 binder。
- 空字符串继续沿用当前 coercion 规则转为 SQL NULL；未填写与额外/缺失 payload 视为校验错误。
- 历史按 `connectionId + normalized descriptor key` 保存最近 5 个非空值；trim、去重、最新优先、版本化。参数名含 password/token/secret/key/credential 等敏感词时不落盘。
- 历史在 gate 返回 `submitted`、即将调用 snapshot submitter 时记录；确认取消、blocked、stale 和提交前校验失败不记录。数据库执行失败仍视为已经提交，可以记录。

## 4. 目标模块与接口契约

PRD 提议的目录作为方向，但文件名以本方案为准。迁移完成后旧 `src/components/SqlEditor.tsx` 可作为短期 re-export，最终所有新实现位于 `src/components/sql-editor/`。

```text
src/components/sql-editor/
├── SqlEditor.tsx
├── contracts.ts
├── editorExtensions.ts
├── semantic/
│   ├── tokens.ts
│   ├── scanner.ts
│   ├── statementRanges.ts
│   ├── scopeModel.ts
│   ├── relationResolver.ts
│   ├── dialectAdapter.ts
│   └── types.ts
├── metadata/
│   ├── identities.ts
│   ├── editorMetadataCache.ts
│   ├── editorMetadataSnapshot.ts
│   └── useEditorMetadata.ts
├── completion/
│   ├── schemaCompletion.ts
│   ├── joinCompletion.ts
│   ├── functionRegistry.ts
│   └── signatureHelp.ts
├── intentions/
│   ├── analyzeIntentions.ts
│   └── applyIntention.ts
├── extensions/
│   ├── statementFrame.ts
│   ├── statementGutter.ts
│   ├── insertHints.ts
│   ├── tableHover.ts
│   ├── definitionNavigation.ts
│   ├── objectDrop.ts
│   ├── pasteAsIn.ts
│   └── multipleSelections.ts
└── __tests__/
```

QueryPanel 新模块放在 `src/windows/connection/query/`，避免继续扩张容器：

```text
src/windows/connection/query/
├── QueryEditorSection.tsx
├── QueryResultSection.tsx
├── useQueryExecutionGate.ts
├── useQueryEditorMetadata.ts
├── useBindParameters.ts
├── queryExecutionRisk.ts
├── queryErrorChatPrompt.ts
├── queryDropHandler.ts
└── contracts.ts
```

### 4.1 基础范围契约

```ts
type SqlTextRange = {
  from: number
  to: number
}

type SqlStatementRange = SqlTextRange & {
  index: number
  contentFrom: number
  contentTo: number
  delimiterFrom: number | null
  delimiterTo: number | null
  firstExecutableLine: number
  kindHint?: SqlStatementKind
  confidence: 'exact' | 'degraded'
}

type SqlExecutionTarget = {
  source: 'selection' | 'current-statement' | 'gutter' | 'all'
  sql: string
  range: SqlTextRange | null
  statementIndex: number | null
  documentVersion: number
}

type ExecutionSnapshot = {
  target: SqlExecutionTarget
  panelId: string
  connectionId: string
  dbSessionId: string
  database?: string
  schema?: string
  params: SqlBindPayloadV2
  safety: {
    readOnly: boolean
    safeMode: boolean
    group?: string
  }
  contextFingerprint: string
}

type ExecutionGateResult =
  | { status: 'submitted'; requestId: string }
  | { status: 'blocked'; reason: string }
  | { status: 'cancelled' }
  | { status: 'stale' }
```

约束：

- `from/to` 覆盖完整原文范围；`contentFrom/contentTo` 去除外围空白及尾随 delimiter；delimiter 有独立 range。`SqlExecutionTarget.sql` 精确取 `contentFrom..contentTo`，frame 覆盖 `from..to`，marker 执行的仍是未经格式化的 content 原文。
- scanner 输出 token/span/depth 和 statement ranges；兼容 wrapper 可继续提供 string 数组，但不得再扫描一次。
- delimiter 是否归入 range 必须固定并通过测试：执行文本默认不含尾随分号，frame 可覆盖分号。
- 光标处在两条语句之间的纯空白时，选择距离光标最近且同一分隔区左侧的语句；文件头空白选择首条，文件尾空白选择末条；纯注释区不执行。
- 首版支持 `'`、`"`、反引号、`[]`、`--`、`#`、`/* */`、PG dollar quote。MySQL `DELIMITER` 与存储过程 body 无法可靠识别时，输出单个 `confidence='degraded'` range：保留 gutter 执行整段、隐藏 frame，禁止显示错误的多 marker。
- `documentVersion` 由 SqlEditor 在每次 `docChanged` 时单调递增；imperative handle 提供 `getExecutionContext()` 和 `validateExecutionSnapshot(version, range, sql)`，gate 在确认后调用验证。组件重建后 revision 重新起算时还必须同时匹配 panelId 和当前文档内容。
- 唯一提交 API 为 `submitExecution(snapshot)`；它不得从 panelStore 重读 SQL。现有 `executeQuery(panelId)` 仅可作为兼容 wrapper 或被非编辑器旧入口使用，新 toolbar/shortcut/gutter/retry 都调用 snapshot API。
- Query execution state 保存 `lastSubmittedExecution` 和失败 requestId，Retry 只引用匹配失败的 snapshot，并重新过当前 gate。

### 4.2 方言适配器

```ts
type SqlDialectAdapter = {
  dialectId: string
  quoteStyle: 'double' | 'backtick' | 'bracket' | 'none'
  foldUnquotedIdentifier(value: string): string
  shouldQuoteIdentifier(value: string): boolean
  quoteIdentifier(value: string): string
  projectionAliasVisibility: 'select-only' | 'order-group' | 'broad'
  parameterPolicy: {
    atNamed: boolean
    question: boolean
    dollarPositional: boolean
    template: boolean
  }
}
```

- `DatabaseTypeMeta` 新增可选 `editorDialectProfile`，内含 quote/fold/alias/parameter policy；未提供时按现有 `sqlDialect` 字符串查通用“方言族 profile registry”，再退回保守 Standard profile。工厂禁止 switch 具体 driver ID。
- path/Git driver 可通过自己的 UI meta 贡献 profile；该字段只影响前端编辑体验，不改 Driver Rust protocol。未知 profile 禁用有歧义的 `@` / `?` 参数和 aggressive alias rewrite。
- bracket escaping 为 `]` → `]]`；double/backtick 分别成对转义。
- 无法确认大小写折叠策略时保持输入原样并采取保守匹配。
- SQL Server registry 修正由驱动轨道完成，宿主 adapter 只实现通用 bracket 能力。

### 4.3 轻量语义模型

```ts
type SqlIdentifierSegment = {
  name: string
  quoted: boolean
}

type QualifiedRelationId = {
  namespacePath: readonly SqlIdentifierSegment[]
  name: SqlIdentifierSegment
}

type SqlRelationBinding = {
  relation: QualifiedRelationId
  alias?: string
  sourceRange: SqlTextRange
  aliasRange?: SqlTextRange
  sourceKind: 'table' | 'view' | 'cte' | 'subquery'
}

type SqlScope = {
  range: SqlTextRange
  parentId?: string
  ctes: SqlCteBinding[]
  relations: SqlRelationBinding[]
  projectionAliases: SqlProjectionAlias[]
}

type SqlSemanticModel = {
  statement: SqlStatementRange
  tokens: readonly SqlToken[]
  scopes: readonly SqlScope[]
  references: readonly SqlReference[]
  diagnostics: readonly SqlSemanticDiagnostic[]
}
```

- 只分析活动 statement 或明确传入的 range，不构建全文件完整 AST。
- 允许不完整 SQL；异常返回部分模型和 diagnostic，不 throw 到编辑器。
- CTE 能解析显式列列表和可确定的 projection；`SELECT *` 或复杂表达式无法确定时不伪造列。
- nested query 的 alias shadowing 以最近 scope 为准；存在歧义时 completion、intention、navigation 均不猜测。
- semantic model 不负责网络、store、Toast、执行或 React 生命周期。

### 4.4 元数据快照

```ts
type EditorRelationKey = string // 由 session/database/schema/name 规范化生成

type EditorRelationMetadata = {
  key: EditorRelationKey
  identity: QualifiedRelationId
  kind: 'table' | 'view'
  columns: readonly ColumnSchema[]
  primaryKey: readonly string[]
  indexes: readonly IndexInfo[]
  foreignKeys: readonly ForeignKeyInfo[]
  comment?: string
  loadedAt: number
}

type EditorMetadataSnapshot = {
  dbSessionId: string
  database?: string
  schema?: string
  epoch: number
  relations: ReadonlyMap<EditorRelationKey, EditorRelationMetadata>
}
```

- key 必须包含 `dbSessionId + namespacePath + relation name`，每段保留 quoted 状态并应用 adapter 的标识符正规化，从而支持 database/schema 以及 path-hierarchy driver 的任意 namespace 层级。
- 当前 `getTableSchema(dbSessionId, table)` 只接受单个 table string；metadata 层必须用 adapter 将完整 identity 编码成 driver 已支持的 qualified identifier 参数，并以真实驱动契约测试。若某驱动不能解析 qualified target，则只对当前已选 namespace 加载，不得假装支持跨 namespace。
- `getCachedTableSchema` 作为 IPC 层缓存来源；新增 inflight dedupe，避免 completion、hover、inlay 同时重复请求。
- CodeMirror 只接收不可变 snapshot；加载完成后外层更新 epoch 并 reconfigure/dispatch refresh effect。
- DDL、schema refresh、切换 database/schema、session 断开时失效；不得把 dbSessionId 持久化。
- 加载失败保存短 TTL 的 error state，UI 降级但不反复请求。
- 当前 `ForeignKeyInfo.referencedTable` 不含 namespace。首版 FK JOIN 只对“当前 schema 内且 referencedTable 唯一解析”的关系生成条件；跨 schema 或同名歧义只显示普通 relation completion。扩展 FK 完整 identity 需单独 Driver API/protocol migration。

### 4.5 编辑器对外契约

```ts
type SqlEditorCallbacks = {
  onExecuteTarget(target: SqlExecutionTarget): void
  onActiveExecutionTargetChange(target: SqlExecutionTarget | null): void
  onRequestMetadata(ids: QualifiedRelationId[]): void
  onOpenRelation(request: OpenRelationRequest): void
  onCopyRelationDdl(request: CopyRelationDdlRequest): void
  onDropObject(request: EditorDropRequest): void
}

type SqlEditorExecutionState = {
  status: 'idle' | 'running' | 'cancelling'
  targetRange: SqlTextRange | null
  documentVersion: number | null
}
```

- CodeMirror extension 只发领域事件，不直接 import panelStore、connectionStore 或 commands。
- gutter spinner 只在 documentVersion 与 targetRange 都匹配时显示；文档改变后降级为普通 running 状态，不把 spinner 错挂到其他语句。

### 4.6 拖拽契约

```ts
type SchemaObjectDragPayloadV1 = {
  version: 1
  sourceConnectionId: string
  sourceDbSessionId?: string
  databaseType: string
  object:
    | { kind: 'table'; namespacePath: string[]; table: string }
    | { kind: 'column'; namespacePath: string[]; table: string; column: string }
}
```

- 新 MIME 为版本化通用对象类型，同时兼容读取旧 `application/datazen-table`。本 PRD 不增加 tree 多选，因此首版没有 multi-table payload。
- source connection 不同则拒绝并提示，不跨连接偷偷生成 SQL；缺少 source session 时按 connectionId 校验。
- 空编辑器的 table drop 保留现有生成 `SELECT ... FROM ...` 行为；非空编辑器插入 quoted qualified table；column drop 优先插入可唯一解析的 alias-qualified column，否则插入 quoted column。
- drop caret 使用 CodeMirror decoration/state effect；`dragenter`、`dragover` 更新，`dragleave`、`drop`、`dragend`、编辑器卸载都清理。

### 4.7 AI 草稿桥

```ts
type AiChatDraftRequest = {
  requestId: string
  source: 'query-error'
  panelId: string
  connectionId: string
  dbSessionId: string
  database?: string
  schema?: string
  contextFingerprint: string
  content: string
  focus: true
}
```

- `ContentView` 是唯一协调者，持有侧栏开关和 pending draft。
- callback 由 `ContentView → PanelContentRenderer → QueryPanel → QueryErrorPanel` 传递。
- `AiChatPanel` 仅在当前身份/context 仍匹配时接收草稿；切到 Chat tab、填入 input、聚焦，但不发送。
- pending draft 由 ContentView 持有，直到 `AiChatPanel` 在 textarea 写入成功后调用 `onDraftConsumed(requestId)`。未配置 AI 的 early-return 页面不得消费。
- textarea 已有非空未发送内容时绝不静默覆盖：展示“替换/追加/取消”的小型非模态选择；追加时用明确分隔符。用户决定前 pending 保留。
- prompt 构建复用 `buildQueryDiagnosisContext` 的 safe SQL、safe error、bounded context；增加独立快照测试防止敏感信息回归。

### 4.8 参数 wire payload

```ts
type SqlParamOccurrenceV2 = {
  from: number
  to: number
  id: `named:${string}` | `dollar:${number}` | `question:${number}`
  token: string
}

type SqlBindPayloadV2 = {
  version: 2
  values: Record<string, string | number | boolean | null>
  occurrences: SqlParamOccurrenceV2[]
}
```

- span 相对于 `ExecutionSnapshot.target.sql`，而不是完整编辑器文档；gate 在截取 target 后重新解析 descriptor，避免 selection offset 漂移。
- Rust binder 先用自己的安全 scanner 标记不可替换区，再验证 occurrence；它不根据 `?` 或 `@` 自己推导 placeholder，也不依赖具体 driver ID。
- v2 payload 的 values 必须与 occurrence ID 集合相等；重复 named occurrence 可共享同一 ID，question occurrence 必须各有唯一 ID。
- ConfirmDialog 展示和复制 placeholder 原文，不展示绑定后的 SQL 或实际参数值。

## 5. 阶段与工作轨道

并发上限按“一个协调代理 + 最多三个执行代理”设计。同一 wave 内轨道可并行；跨 wave 只有通过门禁后才能启动。热点文件实行单一写 owner。

### Stage 0：协调准备与基线冻结

#### Track S0-A：集成基线

目标：建立可恢复的协调台账和可比较的测试基线，不写业务代码。

步骤：

1. 协调代理创建明确的 integration branch，并记录起始 commit 和工作区原有未提交变更。
2. S0 只为下一 wave 的轨道创建 worktree。以后每个 wave 都必须等上一门禁通过、integration HEAD 固定后，再同批用 `scripts/new-feature-worktree.sh <track-id> <integration-branch>` bootstrap；不得提前为 S2～S7 从旧基线建分支。
3. 在 `docs/development/coordination/tracks/<id>/` 初始化 `progress.md` 和 `bugs.md`；每份 progress 明写 `Task`、本方案绝对路径、base commit、worktree、branch 和最后心跳，子代理不得编辑 hub。
4. 记录当前 `pnpm test:unit`、`cargo test -p datazen --lib`、前端 typecheck/build 的结果及已知失败。
5. 冻结 20,000 行性能 fixture、500+ 行单 statement fixture 和 E2E 基础连接夹具的规格、路径、seed/hash 与验收命令；实际文件由 S1-C Coder 创建。
6. 冻结第 3、4 节的接口；任何变更通过协调代理广播，禁止轨道私自修改共享 contract。

完成定义：基线可重跑、轨道 worktree 可用、现有失败有记录、共享契约完成审阅。

### Stage 1：等价拆分热点容器

本阶段只允许行为保持重构，不加入 PRD 新功能。

#### Track S1-A：SqlEditor 壳层拆分

独占文件：`src/components/SqlEditor.tsx`、新 `src/components/sql-editor/SqlEditor.tsx`、`contracts.ts`、`editorExtensions.ts`。

禁止修改：QueryPanel、schemaStore、ContentView、Rust。

步骤：

1. 把公共 props、handle、drop legacy 类型移到 contracts。
2. 把 dialect compartment、theme compartment、keymap、completion 和 DOM handlers 装配拆入小 factory。
3. 保留旧 import path 的 re-export，避免一次修改所有消费者。
4. 保证 `Mod+Enter`、`Mod+Shift+Enter`、`toggleLineComment`、`insertAt`、自动补全和旧 table drop 完全等价。
5. 确保 extension reconfigure 不重复注册 listener；组件卸载销毁 EditorView。
6. 将本轨新建和修改的所有生产文件降至 500 行以下。

测试：现有 `SqlEditorShortcuts`、`SqlEditorDrop`、主题和 completion 测试全量回归；增加装配去重和 unmount 清理测试。

完成定义：无行为变化、旧调用方无需改动、文件限长通过。

#### Track S1-B：QueryPanel 壳层拆分

独占文件：`QueryPanel.tsx` 和新 `src/windows/connection/query/` 壳模块。

禁止修改：SqlEditor 内部、ContentView、schema tree、Rust。

步骤：

1. 先提取 `QueryEditorSection`、`QueryResultSection` 和纯 view props。
2. 提取现有执行入口为 `useQueryExecutionGate` 的等价版本，但不改变 guard 顺序或确认行为。
3. 提取 editor metadata/context、参数 state、drop handler 的现有行为。
4. 保留收藏、格式化、历史、事务、EXPLAIN、取消、诊断、结果 Tab 语义。
5. 把 `buildQueryPanelDiagnosisContext` 迁到独立模块并保持现有测试导出兼容。
6. 降低 QueryPanel 到推荐 300 行左右，任何抽取模块不得超过 500 行。

测试：QueryPanel 现有全部单测；重点回归 execute/cancel、危险 SQL、history、context、diagnosis、multi-result。

完成定义：测试输出与基线一致；后续轨道可通过窄接口接线，无需再次拆主文件。

#### Track S1-C：E2E 与性能夹具

独占文件：新 E2E helper、fixture 和性能脚本；不修改现有 SQL 主 spec。

步骤：

1. 封装设置编辑器文本、光标、选区、触发快捷键和读取结果的稳定 helper。
2. 增加读取 gutter marker、tooltip、对话框、AI 侧栏的选择器约定。
3. 生成确定性的 20,000 行 SQL fixture，不在测试运行中随机生成。
4. 增加性能测量 harness：warmup、重复次数、median/p95、机器信息输出。
5. 文档化测试数据库初始化和清理，禁止依赖真实 AI key。
6. 新增专用 SQL editor Vitest coverage 配置或扩充现有 include，明确 statement/branch/function/line 阈值；不得假设当前全局配置会自动覆盖新目录。

完成定义：helper 能对现有编辑器执行最小 smoke journey；fixture 可被后续轨道复用。

#### Track S1-D：i18n key 契约（在 S1-A/B/C 合流后串行）

独占文件：`src/locales/en/query.ts`、`src/locales/zh-CN/query.ts` 及确定存在的 editor/settings 翻译域。

步骤：

1. 根据 AC-01～AC-21 一次性定义本项目所需的 frame/gutter/completion/intention/paste/drop/param/risk/AI/settings 文案 key。
2. 同时添加英文和中文值，保证 `TranslationKey` 立即可编译；其他功能轨只能消费这些 key，禁止修改 locale 文件。
3. 后续若发现缺 key，由 Coordinator 暂停冲突轨、恢复本轨唯一 owner 增补并合流，再继续下游 wave。
4. 记录发布前需要同步到其他语言的 key 清单；开发门禁只要求 en/zh 类型和测试通过。

完成定义：下游功能不需要创建“临时翻译 key”，locale 始终只有一个写 owner。

Stage 1 门禁：四轨 Tester 均 PASSED；合流后运行 typecheck、Host 前端单测和 Rust 基线；只检查 S1-A/B 本阶段已修改的 SqlEditor/QueryPanel 文件族低于 500 行，不宣称全仓热点都已拆完。

### Stage 2：共享词法、语义与参数/风险纯核心

调度顺序固定为：Wave 2A 只有 S2-A，完成 Coder→Tester→merge 后固定新的 base commit；Wave 2B 并行启动 S2-B、S2-C；两者合流后，Wave 2C 再启动 S2-D。禁止让依赖 scanner 或 Rust guard 拆分的轨道从旧 HEAD 开发。

#### Track S2-A：统一 scanner 与 statement ranges

依赖：S1-A。

独占文件：新 semantic scanner/range 模块；兼容改造 `sqlStatementRange.ts`、`sqlTransactionGuard.ts`。

步骤：

1. 定义 token、quote/comment 状态、paren depth 和 original span。
2. 支持单/双引号、反引号、方括号、line/block comment、`#` comment、PG dollar quote。
3. 从 tokens 构造原文 statement ranges、首个可执行行和 kind hint。
4. 用兼容 wrapper 替换旧 `getStatementAtCursor` / `splitSqlStatements` 内部逻辑。
5. 保证 transaction guard、当前快捷键和多语句流执行的输入不变化。
6. 实现 CodeMirror `StateField` statement index：根据 `Transaction.changes` 映射未受影响旧 ranges，只从变化前最近的安全 delimiter 向后局部重扫，直到扫描状态与旧 index 重新汇合；无法证明汇合时才安全回退全文扫描。
7. 加入按 document identity + dialect + revision 的小缓存；选区移动不得重扫未变文档，受控外部 value 替换必须递增 revision 并重建 index。

测试：引号内分号、escaped quote、nested comment 边界、dollar tag、空白/注释、光标在 delimiter 前后、20k fixture、旧 API 回归。

完成定义：前端只剩一个 TypeScript 词法核心；所有前端消费者使用 range/token 结果。Rust Host 安全扫描保留，并通过共享 fixture 对齐。

#### Track S2-B：轻量 scope model、relation resolver 与 dialect core

依赖：S2-A 已 PASSED 并合流。

独占文件：`semantic/scopeModel.ts`、`relationResolver.ts`、`dialectAdapter.ts`、通用 quote helper、类型和纯单测。

步骤：

1. 从 token stream 识别 SELECT/INSERT/UPDATE/DELETE 的 scope 边界。
2. 提取 FROM/JOIN relation、database/schema qualification、显式/隐式 alias。
3. 解析 WITH CTE、显式 CTE 列和可确定 projection。
4. 建立 parent scope 与 alias shadowing。
5. 识别 cursor intent：relation、qualified column、projection、join target、function call、INSERT values。
6. relation resolver 输出唯一、歧义或未解析，不直接访问 store。
7. 对不完整 SQL 返回部分结果，不因孤立括号/关键字 throw。
8. 冻结现有 `sqlCompletions.ts` / `sqlCompletionContext.ts` 的迁移接口；本轨不修改它们，实际兼容 wrapper 由 S4-B 唯一完成。

测试：alias、quoted identifier、CTE、nested query、shadowing、同名表、多 schema、incomplete SQL、自关联。

完成定义：所有测试仅用纯输入/输出；没有 React、store、IPC 依赖。

#### Track S2-C：参数 lexer、history core 与 Host binder

依赖：S2-A 已 PASSED 并合流。

独占文件：`sqlBindParams.ts`、新参数 fixture/history core、参数 command types、拆分后的 `src-tauri/src/sql_guard/params.rs` 与相关 Host tests。

步骤：

1. 扩展 descriptor 为五种 syntax，保留 occurrence、token range、ordinal 和稳定 ID。
2. 定义方言 policy，处理 `::`、`@@`、dollar quote、PG JSON 问号运算符和 SQL Server 局部变量。
3. 让 TS parser 和 Rust binder 使用同一组 JSON 行为 fixture；分别验证“识别”和“替换”。
4. Rust object binder 支持新稳定 ID，同时兼容旧 `:name` / `$name` payload。
5. 保持 SQL literal 转义，不提供 raw SQL 或 identifier substitution。
6. 先把已超过 500 行的 `src-tauri/src/sql_guard.rs` 等价拆为 `sql_guard/mod.rs`、`params.rs`、`safety.rs`、Rust scanner/测试子模块；本轨只新增 params 逻辑。
7. 确保 execute 与 streaming 两条 Host 路径调用同一个 v2 binder；保留旧 payload 兼容。
8. 实现版本化 history 纯模块、敏感参数名过滤、connection 隔离和最近 5 条。

测试：五语法、重复命名、每个 `?` 独立编号、混用、comments/quotes/casts/operators、DECLARE/SET；occurrence span 校验、Rust 旧兼容和转义。

完成定义：前后端 parameter fixture 一致，旧调用兼容，Host binder 全测试通过；风险逻辑不在本轨扩展。

#### Track S2-D：风险 classifier 与 Host guard 回归

依赖：S2-A、S2-C 已 PASSED 并合流；消费 S2-C 拆出的 Rust 模块目录，使用不同文件写锁。

独占文件：`dangerousSql.ts`、`queryExecutionRisk.ts`、`src-tauri/src/sql_guard/safety.rs`、对应 TS/Rust tests。

步骤：

1. 实现 read/mutation/unknown 和 drop/truncate/no-WHERE findings。
2. WHERE 必须位于当前 DML 顶层，排除 CTE、subquery、字符串、注释。
3. 多语句汇总全部 findings 和最高风险，保留原文 range。
4. 保持 Rust `readOnly` / Safe Mode 行为，不增加确认绕过参数。
5. 验证 binder 后再 guard 的顺序由 execute/stream 共用。

测试：顶层 WHERE、CTE/subquery、字符串中 WHERE、多语句、unknown、readOnly/Safe Mode、execute/stream 一致性。

完成定义：前端 assessment 与 Host guard 的共享 fixture 无语义漂移，既有 guard 测试全绿。

Stage 2 门禁：Wave 2A、2B、2C 分别完成独立 Tester；合流后运行全部 scanner、transaction、param、risk 单测及 `cargo test -p datazen --lib`；性能基线不得比 Stage 0 恶化超过已定义阈值。

### Stage 3：丰富元数据、导航桥和 schema tree 壳层

#### Track S3-A：编辑器元数据缓存

独占文件：`src/components/sql-editor/metadata/*`、`src/lib/schemaCache.ts`、新 `src/stores/schemaStoreSelectors.ts` 及对应 tests；不修改 `schemaStore.ts`、QueryPanel 或 SqlEditor 装配。若真实接入必须修改超长 `schemaStore.ts`，先停止并由 Coordinator 单列等价拆分轨。

步骤：

1. 实现规范化 relation key，覆盖任意 namespace path、quoted case 和同名表。
2. 复用 `getCachedTableSchema`，增加 inflight dedupe、TTL error 和 immutable snapshot。
3. 提供批量 `ensureRelations`，对 semantic model 提取的 relation debounce 去重。
4. 输出 ColumnSchema、PK、index、FK；表 comment 保持 optional。
5. 接入 schema refresh、DDL mutation、session close 的 invalidation；同时把 DDL cache key 升级为 `dbSessionId + object kind + full identity`，避免跨 schema/table-view 碰撞。
6. 保留现有 `columnMap` 给旧 CodeMirror schema completion，先不破坏其他调用方。
7. Copy DDL 复用现有 DDLView 的方言 SQL 生成和 extractor strategy，通过完整 identity 调用升级后的 `getCachedDDL`；不调用不存在的通用 table/view DDL API。

测试：session/namespace/quoted-case 隔离、table/view 同名、并发 dedupe、失败重试、invalidation、切换上下文、无 table comment 降级，并回归 DDLView、StructureView、IndexesView、ExportDialog 的 cache 调用。

完成定义：completion 回调可同步读 snapshot，期间不产生 IPC。

#### Track S3-B1：ContentView / PanelContentRenderer 等价拆分

独占文件：`src/windows/connection/ContentView.tsx`、`src/windows/connection/PanelContentRenderer.tsx` 及新拆出的 workspace/view 子组件；不修改 QueryPanel、AiChatPanel 或行为。

步骤：

1. 抽取 sidebar、panel content 和导航 view model，保留所有 props 和 state 生命周期。
2. 保持当前 table/data/structure navigation、AI sidebar 开关、split layout 和 active panel 行为。
3. 将两个 touched 生产文件及新模块降到 500 行以下。

测试：现有 ContentView/PanelContentRenderer 测试、导航、AI sidebar、panel switching 回归。

完成定义：纯等价拆分通过，新 bridge 有稳定的窄接入点。

#### Track S3-B2：导航与 AI 草稿桥（S3-B1 合流后串行）

独占文件：S3-B1 新建的 bridge contracts/协调 hook、`ContentView.tsx` 的薄装配、`PanelContentRenderer.tsx` 的薄装配、`src/components/ai/AiChatPanel.tsx` 及对应 tests。QueryPanel 只消费 S1-B 预留的 optional boundary prop；若 S1-B 未预留，由 Coordinator 给本轨精确授权修改 `src/windows/connection/query/contracts.ts`，不得改执行逻辑。

步骤：

1. 定义 `openRelation`、`copyRelationDdl`、`openAiChatDraft` callbacks。
2. 复用现有 table selection/panel handler 打开数据与结构页。
3. ContentView 持有 pending draft request，并把 callback 下传到 QueryPanel boundary。
4. AiChatPanel 增加 backward-compatible draft/focus/ack props。
5. 未配置 AI 时不消费；已有非空草稿时执行第 4.7 节的冲突策略；streaming 时可预填但不发送。
6. 不发送消息、不清空历史、不建立第二个 chat store。

测试：callback 透传、身份匹配、单次 ack、侧栏打开、focus、未配置 AI、streaming、已有未发送草稿冲突。

完成定义：深层功能可通过 callback 请求导航/Chat，无 DOM query hack 和 store 越层调用。

#### Track S3-C：Schema tree 拖拽壳层

独占文件：`UnifiedSchemaTree.tsx` 及 schema-tree 子模块、拖拽 payload contract。

步骤：

1. 先拆分大文件，保持树展开、搜索、上下文菜单、选择等价。
2. 定义版本化通用 MIME 和 table/column discriminated payload，继续发送旧 MIME 兼容数据。
3. 为可见列节点提供展开/加载路径；搜索命中的列也可作为 drag source。
4. payload 始终携带 connectionId、对象 database/schema/table/column；有 session 时携带 dbSessionId。
5. 不在 tree 侧决定 SQL 文本和 quote，不 import editor semantic 模块。

测试：table/column payload、namespace identity、搜索列拖拽、legacy payload、tree 原行为回归。

完成定义：树只表达 schema object，不承担 SQL 生成。

Stage 3 调度：Wave 3A 并行 S3-A、S3-B1、S3-C；全部合流后 Wave 3B 执行 S3-B2。

Stage 3 门禁：快照契约、桥契约和 drag payload 冻结；四轨 Tester PASSED；合流后 schema refresh 和导航 smoke 通过。

### Stage 4：语句 UI、智能补全与阅读辅助

本阶段只交付可独立测试的 CodeMirror extension factory 和纯逻辑模块，不修改中央 `SqlEditor.tsx` / `editorExtensions.ts`。它们由 S6-D 唯一装配，因此本阶段 DoD 是 factory contract 通过，不宣称功能已经对用户可见。

#### Track S4-A：Statement Frame、Gutter 与执行状态

独占文件：statement frame/gutter extension factory 及其测试；不修改 SqlEditor composer 或 QueryPanel。

步骤：

1. frame 仅消费统一 active statement range；多行非空 selection 时隐藏。
2. 超过 500 行时停止昂贵几何探测并隐藏 frame，只保留 gutter；设置可测试的 degraded state，禁止由实现者在多种视觉行为中自行选择。
3. gutter 只在每条有效语句首个可执行行显示 play；纯注释不显示。
4. tooltip 文案区分 macOS 与其他平台快捷键。
5. 点击 marker 生成 `SqlExecutionTarget(source='gutter')`。
6. running spinner 按 documentVersion + targetRange 精确匹配；取消/结束恢复 play。
7. 使用主题 token 和 Web tooltip，不引入 Tauri menu。

测试：range 映射、selection hide、500 行降级、marker click、running/cancel、文档改变、暗色/亮色 class。

完成定义：Frame、gutter 和快捷键对同一 SQL 产生同一 range。

#### Track S4-B：补全、FK JOIN 与函数签名

独占文件：completion 目录、function registry、signature extension、`src/lib/sqlCompletions.ts`、`src/lib/sqlCompletionContext.ts` 及测试；不修改 metadata loader 或 editor composer。

步骤：

1. 保留现有基础 schema completion 作为 fallback。
2. `alias.` 唯一解析时只返回该 relation 的列，detail 显示 type/nullable/comment；歧义时不猜。
3. relation completion 使用当前 scope 的 database/schema 上下文。
4. JOIN completion 从 snapshot 的 FK 双向构建候选；支持复合键、自关联、多个关系分别列出。
5. 选中 JOIN 候选插入 quoted relation、唯一 alias 和完整 ON 条件；不得覆盖用户已有 ON。
6. 函数 completion 与 signature help 共用 registry；signature 根据 comma depth 高亮当前参数。
7. registry 由 dialect 元数据/通用集合组合，不写 driver ID switch。
8. 初始 registry 至少覆盖 PRD 指定的 `DATE_ADD`、`CONCAT` 及现有 common/PG/MySQL/SQLite 清单；旧 `sqlFunctionCompletions` 改为该 registry 的兼容投影。

测试：alias filter、CTE、ambiguous、类型注释、复合/多 FK、自关联、现有 ON、嵌套函数、缺元数据 fallback。

完成定义：completion/source 回调同步完成且不发 IPC；函数信息单一来源。

#### Track S4-C：Intention 与 INSERT Hint

独占文件：intentions、insertHints 及纯/extension tests；不修改 composer。

步骤：

1. `analyzeIntentions` 返回纯 action 数据，`applyIntention` 单 transaction 应用。
2. `SELECT *`：单 relation 扩展物理列；`alias.*` 只扩对应 relation；多 relation 裸 `*` 若策略不唯一则不提供动作。
3. add/remove qualifier 只处理 semantic reference token，不用全文字符串替换；冲突或歧义时拒绝。
4. Alt+Enter 菜单支持键盘、Esc、focus 恢复和 lightbulb 入口。
5. INSERT 有显式列时本地对位；无显式列时用 metadata 物理顺序。多 VALUES 行均提示，列数不匹配只提示可确定部分。
6. inlay 只分析 viewport ∩ active statement，debounce，设置关闭时不装配。
7. extension 接受 setting boolean，但动态装卸由 S6-D composer 完成。

测试：intention 事务和 cursor、歧义拒绝、多 VALUES/viewport、setting on/off factory 行为。

完成定义：intention/hint factory 在缺 metadata 时优雅缺席，不错误改写 SQL。

#### Track S4-D：Hover 与定义导航（S4-A/B/C 合流后启动）

独占文件：`tableHover.ts`、`definitionNavigation.ts` 及测试；不修改 composer、ContentView 或 QueryPanel。

步骤：

1. hover 延迟 300ms，移出/文档变更取消；展示已存在的 database/schema、PK、index、核心列，comment 不存在就隐藏。
2. 列清单和 tooltip 内容有上限，大表显示省略计数。
3. Hover actions 和 Mod/Ctrl-click 使用同一个 resolver/callback；默认 click 打开数据页，结构页由 hover 明确动作进入。
4. Copy DDL 只发既定 callback，不从 metadata 拼 DDL。
5. identity 不唯一、view 不支持某动作或 metadata 缺失时隐藏对应动作。

测试：hover delay/cancel、identity、modifier click、动作可见性、复制 DDL callback。

完成定义：hover/navigation factory 可独立挂载测试，无跨层 store/command import。

Stage 4 调度：Wave 4A 并行 S4-A、S4-B、S4-C；合流后 Wave 4B 执行 S4-D。

Stage 4 门禁：四轨 factory 测试通过；组合装配、reconfigure 和真实 UI E2E 延后由 S6-D 及各功能 Tester完成。

### Stage 5：生产力、参数 UI 与统一执行门

调度顺序：Wave 5A 并行 S5-A、S5-B；合流后 Wave 5B 单独执行 S5-C。S5-A 的 editor extension 仍是 leaf，由 S6-D 装配。

#### Track S5-A：Paste as IN、Drop Caret 与多光标

独占文件：paste/drop/multiple-selection extension factory、context-menu item factory 及测试；不修改 tree 或 SqlEditor composer。

步骤：

1. 实现纯 `parseDelimitedValues`：CRLF/newline/comma/tab、quoted delimiter、trim、空项策略。
2. 提供“自动类型”和“全部按字符串”两种模式。自动类型中数字保持数值、`NULL` 保持 SQL NULL；其余内容单引号并将 `'` 转义为 `''`，不执行表达式。右键菜单用子项选择，快捷键使用用户上次选择或默认自动类型。
3. 设 source 上限 1 MiB、value 上限 10,000，超限拒绝并给可本地化提示。
4. 光标前是 `IN` / `NOT IN` 时只插入括号，否则插入 `IN (...)`；非空 selection 替换 selection。
5. 绑定 `Mod+Shift+V` 和 Web Context Menu 项，读取 clipboard 失败时不改文档。
6. 实现 drop caret；校验 source connection；按第 4.6 节决定 empty/non-empty/table/column 行为。
7. quote 使用 dialect adapter；PostgreSQL 全小写安全标识符可免引号，保留字/大小写/特殊字符必须引用。
8. 显式启用 `allowMultipleSelections`、Mod+D next occurrence、`rectangularSelection()`，处理 keymap 优先级。

测试：delimiter/quote/NULL/numeric/limits/prefix、clipboard failure、drop caret 生命周期、legacy/new payload、cross-connection、三个 quote style、Mod+D 和矩形 selection。

完成定义：所有写文档行为都是单 transaction，undo 一次可完整撤销。

#### Track S5-B：BindParamPanel 与历史

独占文件：BindParamPanel、useBindParameters、history UI 和对应 tests；Rust binder/command wire 已由 S2-C 完成，不修改 QueryPanel 或 execution gate。

步骤：

1. 面板按当前 execution target 解析 descriptor；命名参数去重、question 保持 occurrence。
2. label 保留原 syntax，input key 使用稳定 ID，目标变化时复用仍存在的值。
3. 历史下拉支持鼠标、上下键、Enter、Esc；显示最近 5 条并可清除当前参数历史。
4. localStorage 不可用、配额错误、损坏 JSON 时静默降级，不影响执行。
5. hook 输出 `buildPayloadForTarget(target.sql)`、`markSubmitted(snapshot)` 和参数 fingerprint，供 S5-C 消费；本轨不接执行入口。
6. `markSubmitted` 记录历史，敏感名不记录；只有 S5-C 在真正提交前调用。
7. 保持紧凑横条，不增加模态对话框，不支持 raw SQL 值。

测试：五语法展示、shared name、question ordinal、历史隔离/键盘/a11y、失败降级、执行目标切换、Retry 参数改变取消。

完成定义：参数 UI/hook 可独立测试并生成符合 v2 contract 的 payload；真实 UI→Host 验收由 S5-C 完成。

#### Track S5-C：统一风险确认与 QueryPanel 接线

独占文件：`src/windows/connection/query/useQueryExecutionGate.ts`、QueryPanel 薄接线、query execution contracts、`src/components/ui/ConfirmDialog.tsx`、`src/hooks/useConfirmDialog.tsx` 及对应 tests。不得修改 locale；只使用 S1-D 已定义 key。

步骤：

1. 所有 toolbar/shortcut/gutter/all/retry/unclosed-transaction continue 入口统一通过 execution gate；EXPLAIN/事务控件/导出/AI Apply 按第 3.1 节白名单排除。
2. gate 冻结 target、context、params，先完成事务检查和参数校验，再按 readOnly→Safe Mode hard block→production/high-risk confirm 的顺序处理，最终只调用 `submitExecution(snapshot)`。
3. production 规则固定为 `isProduction && classification !== 'read'`；危险 + production 合并为一次最高级确认并显示全部 reason。
4. 对通用 ConfirmDialog 仅增加 optional badge/codePreview/description，旧调用视觉和行为不变。
5. SQL 预览限制行数和字符数，支持复制完整 target，但不把参数历史或连接凭据显示出来。
6. 用户取消、焦点退出、document revision/SQL/panel/connection/session/group/readOnly/safeMode/参数 fingerprint 任一改变均返回 cancelled 或 stale，不得提交。
7. Safe Mode 后端错误保持原来的可解释提示，不提供绕过按钮。

测试：readOnly、Safe Mode on/off、no-WHERE、production 普通写/高危/unknown、SELECT、高危+生产单弹窗、选区/当前/gutter/all、确认期间变化、unclosed transaction、snapshot submit、failed snapshot Retry、五参数真实 UI→Host。

完成定义：不存在绕开 gate 的执行入口；Host guard 测试继续通过。

Stage 5 门禁：真实参数执行集成测试通过；安全矩阵全绿；BindParamPanel 和 ConfirmDialog a11y/焦点回归通过。

### Stage 6：AI 错误联动、设置与方言收口

#### Track S6-A：Ask in Chat

独占文件：`src/components/query/QueryErrorPanel.tsx`、`src/windows/connection/query/queryErrorChatPrompt.ts`、QueryPanel error-section 薄接线、`PanelContentRenderer.tsx` 的 bridge prop 透传及对应 tests。消费 S3-B2 的 ContentView/AiChatPanel bridge，不修改 locale。

步骤：

1. `QueryErrorPanelProps` 增加 optional `onAskInChat`，不影响 Retry/Fix/Explain 条件和布局。
2. 使用现有 diagnosis context 构建稳定分段 prompt：错误、SQL、dialect、database/schema、用户请求；所有字段有长度上限并脱敏。
3. 点击后发带 panel/connection/session/context fingerprint 的 `AiChatDraftRequest`，展开 Chat、切到 chat tab、预填并 focus；只有 textarea 成功写入后 ack。
4. 不自动发送；未配置时保留草稿并显示原配置入口。
5. 确认历史参数值、结果行、session IDs 和 secret fixture 不出现在 prompt。

测试：按钮显隐、一次点击一次 request、已有 actions 不变、脱敏/长度、侧栏/focus/未配置/streaming/非空草稿冲突。

完成定义：错误上下文能进入现有 Chat 草稿，且没有新增聊天引擎或自动数据外发。

#### Track S6-B：INSERT Hint 设置持久化

独占文件：AppSettings TS/Rust、settings store/UI/tests；翻译只消费 S1-D 已建立的 key，不修改 locale。

步骤：

1. 新增持久化字段 `editorInsertValueHints`，默认 true；Rust serde default 兼容旧配置。
2. 在现有编辑器设置区域增加 switch，描述只影响视觉提示、不修改 SQL。
3. 验证旧配置缺字段时按 serde/default 正常加载，保存后字段稳定。
4. 开发期不修改其他语言；S1-D 台账继续记录发布前 i18n sync 工作。
5. 本轨只验证 setting/prop；关闭时 extension 不装配、重新开启即时生效由 S6-D composer 测试。

测试：旧配置迁移、默认值、store save/load、Settings switch、编辑器 reconfigure。

完成定义：设置跨重启保存；前后端配置类型和默认值通过。

#### Track S6-C：方言 metadata 审计与驱动任务

本轨先只读审计并生成逐驱动任务清单，不再拥有通用 quote helper；helper 已由 S2-B 唯一实现。每个需要修正的 path driver 作为独立子轨，精确拥有该 `packages/drivers/<id>/ui/meta.ts` 与该驱动 tests。

步骤：

1. 检查 MySQL/ClickHouse/Doris、PG/openGauss、SQL Server registry metadata 是否与 PRD 一致，并评估 quoteChar 变更对通用 SQL generator、DDL/index 等路径的影响。
2. SQL Server 子轨修正 bracket metadata 时同步验证 CodeMirror MSSQL profile；`resolveCmDialect` 的通用装配改动留给 S6-D。
3. ClickHouse 当前 metadata 与 PRD 有差异，先形成产品决策门；未确认前不改变可能影响既有生成器的 quoteChar。
4. 只在获批子轨修正错误 metadata，不在 Host 添加 driver ID 判断。
5. 驱动专属单测放在各 driver UI tests；Git driver 若不在仓库中则登记为插件仓任务，不修改生成目录。
6. 不编辑 `generated.ts`、`driver_init.rs` 或生成 capability 文件。

完成定义：审计清单完整；获批 path-driver 子轨各自通过测试；Git driver 外部任务有 owner，Host 通用测试不包含驱动 ID 常量。

#### Track S6-D：中央 Editor / Query 装配（S6-A/B/C 合流后串行）

这是唯一中央装配轨。独占文件：`src/components/sql-editor/SqlEditor.tsx`、`src/components/sql-editor/editorExtensions.ts`、`src/components/sql-editor/contracts.ts`、`src/components/SqlEditor.tsx` 兼容入口、`src/windows/connection/query/QueryEditorSection.tsx`、`src/windows/connection/query/queryDropHandler.ts` 及对应组合 tests。不得修改 leaf 算法、locale 或 driver meta。

步骤：

1. 将 S4-A/B/C/D 和 S5-A 的 factory 按固定优先级装入 compartments：statement、completion/signature、intention/hint、hover/navigation、paste/drop/multiple selection。
2. 接入 metadata snapshot、execution state、active target change、navigation/DDL callbacks 和 INSERT hint setting。
3. `documentVersion` 用 editor StateField 维护；外部 value replacement、undo/redo 和 component remount 均有明确定义与测试。
4. 将 table/column drop request 交给 `queryDropHandler`：空编辑器保留生成 SELECT，非空插入引用，跨连接拒绝；旧 payload 继续兼容。
5. 将现有 CodeMirror dialect mapping 接入方言族 profile；SQL Server 使用 `@codemirror/lang-sql` 的 MSSQL 支持，未知方言退回 Standard。
6. 按设置动态装卸 hint；反复 reconfigure 不重复 listener/timer/tooltip。
7. 把旧 `sqlCompletions`、`sqlCompletionContext`、旧 SqlEditor import 保持为兼容 wrapper，确认没有第二份函数或 scanner 数据源。

测试：全部 extension 组合、keymap 冲突、reconfigure/dispose、metadata refresh、setting toggle、drop→QueryPanel、execution snapshot、MSSQL/Standard mapping。

完成定义：所有 leaf 功能在真实 SqlEditor 中可见；组合生命周期测试和对应功能 E2E 通过。

Stage 6 调度：Wave 6A 在文件锁无冲突时并行 S6-A、S6-B、S6-C；合流后 Wave 6B 单独执行 S6-D。

Stage 6 门禁：AI 隐私、settings migration、i18n typecheck、目标驱动、Editor 组合测试和 feature E2E 通过。

### Stage 7：端到端收口与性能硬化

本阶段轨道顺序执行，避免共同修改主 E2E spec、coverage 文档和装配文件。

#### Track S7-A：中央装配审计（Tester-only）

步骤：

1. 核对 SqlEditor、QueryPanel、ContentView、schema tree 的 props 和生命周期。
2. 搜索旧重复 scanner、绕开 query-submit gate 的入口和直接 IPC completion；只记录 Bug，不修改业务代码。ObjectBrowser、Import、Privilege、DDL、导出等非 QueryPanel submit 路径不算绕过。
3. 检查本项目新增及实质修改的生产 TS/Rust 文件行数、`any`、硬编码 driver ID、错误的 connection/session ID。
4. 检查 listener/tooltip/debounce/cache 在 unmount、session 切换和文档切换时清理。
5. 检查 `docs/todo/sql-editor/dbx` 无 import、复制结构或运行时依赖。

发现问题时按原轨归属写入 bugs，恢复原 Coder 修复，再启动新的独立审计 Tester；本轨不得“顺手清理”。

完成定义：架构不变量全部通过静态审计和 targeted tests，且没有开放 Bug。

#### Track S7-B：Host E2E

各 UI feature 轨的 Tester 必须在功能合流前提交并运行自己唯一拥有的 E2E spec；不能把测试代码全部推迟到本阶段。建议所有权如下：

- `sql-editor-statement.ts`：多语句 frame、gutter、shortcut、running、多结果回归。
- `sql-editor-intelligence.ts`：alias completion、FK join、Alt+Enter、hint、hover/navigation。
- `sql-editor-productivity.ts`：Paste as IN、table/column drop、Mod+D。
- `sql-editor-safety-params.ts`：Host 通用参数 journey、历史、Safe Mode、production confirmation；方言碰撞不在单一 Host fixture 中强测。
- `sql-editor-ai-error.ts`：确定性错误、Ask in Chat 草稿和脱敏；不调用真实 LLM。

S7-B 只负责运行/去抖上述 spec、补一个跨功能 journey 和汇总 coverage，不重新实现五组测试。

E2E 要求：

1. 使用 Host 通用行为，不写具体数据库方言断言；具体引号 E2E 放驱动目录。
2. 每个测试创建和清理自己的表、连接分组、Safe Mode 状态；失败后也恢复设置。
3. 新增/变更 journey 同步更新 `docs/development/e2e-coverage.md`。
4. 不能自动化的 UI 路径必须登记例外、原因、手工步骤和 owner。
5. 参数按方言矩阵验收：PG 验证 `:name`/`$1`/`${name}` 及 JSON `?` 不误识别；支持 positional `?` 的 SQLite/MySQL contract 验证 `question:n`；SQL Server driver 验证 `@name` 与 `DECLARE/SET @local`。不可用 Git driver 的项目登记到外部插件任务。

完成定义：feature journeys 和跨功能 journey 在 debug webdriver build 上稳定重复通过。

#### Track S7-C：性能与降级验证

步骤：

1. 在固定 fixture 上分别测首次 scanner、活动 statement parse、单字符编辑、selection move、scroll、completion source。
2. warmup 后至少 30 次，记录 median/p95；`<5ms` 定义为活动 statement semantic parse 的 p95，而非首次扫描整份 20k 文档。
3. 20k 文档的 cursor/selection 移动不得触发全文重扫；单字符编辑只重算受影响 window/range。
4. 浏览器 Performance trace 验证持续输入/滚动无 >50ms 长任务，目标交互帧率 60fps。
5. 500+ 行单 statement 验证 frame 降级；大 INSERT 验证 hint 只处理 viewport；hover 快速移动验证请求取消。
6. 普通 CI 使用宽松回归阈值；严格 `<5ms` 和 60fps 放固定机器脚本及发布门禁，输出原始数据以便比较。

完成定义：达到 PRD 目标才可 PASSED。未达标必须 TEST_FAILED 并回流对应 owner；只有用户/产品书面批准 waiver，才可带数据、环境、原因和后续 owner 标注 AC-19 waived，Tester 无权自行降级。

Stage 7 门禁：全量单测、Rust Host 测试、目标驱动测试、Host E2E、性能门禁和 coverage 文档全部通过。

## 6. 功能级算法与验收细节

本节用于避免轨道只完成“有 UI”而没有正确语义。

### 6.1 Statement Frame / Gutter

- frame 与 gutter 必须复用同一 ranges；不得一个按正则、一个按 scanner。
- selection 只有单光标或空 selection 才显示 frame；多光标但无范围 selection 时以 primary cursor 为活动语句。
- marker 的执行文本保留用户原文，不格式化、不重写 quote。
- gutter 执行中的 spinner 只代表当前请求；后续请求覆盖时立即切换。
- marker 点击失败或请求取消后不残留 loading decoration。

### 6.2 Completion / JOIN / Signature

- `o.` 只在 `o` 唯一绑定时返回该表列；未加载时显示 loading/fallback，不返回所有库列。
- 列 detail 顺序：数据类型、nullable、comment；缺字段不显示占位垃圾文本。
- FK candidate 显示关系方向和列对；复合外键生成多个 AND 条件。
- 新 alias 必须避免当前 scope 已用 alias；不能可靠生成时只补 relation，不补 ON。
- signature 的参数 index 只统计当前函数调用深度的 comma，忽略 nested call/string/comment。

### 6.3 Intention

- `SELECT *` 不展开 `COUNT(*)`、乘法 `a * b` 或字符串中的星号。
- 单表裸 `*` 可展开为列；多表裸 `*` 首版不猜，可分别提供“展开为所有 source 列”的明确动作，但必须保序和限定名。
- 加 qualifier 前检查裸列在哪些 source 存在；唯一时执行，多义时不提供。
- 移除 qualifier 前检查移除后仍唯一；否则拒绝。
- 生成列清单遵循 metadata 物理顺序，quote 由 adapter 决定。

### 6.4 INSERT Inlay Hint

- 支持 `INSERT INTO t (a,b) VALUES (1,2)` 和无显式列的简单 VALUES。
- 首版不对 INSERT...SELECT、DEFAULT VALUES、复杂 vendor syntax 伪造 hint。
- 多行 VALUES 每行独立对位；表达式内部 comma 不增加 value index。
- hint 是 decoration，不进入复制、搜索、undo 或 SQL 请求。

### 6.5 Hover / Navigation

- hover 只对 semantic relation reference 生效，不对同名 alias、字符串或注释生效。
- metadata 未加载时可显示轻量 loading，并由外层请求；离开后取消展示，缓存请求本身可继续完成。
- 列清单限制展示数量并提供省略计数，避免大表 tooltip 卡顿。
- 打开对象必须携带完整 database/schema/name；同名表不得跳错。
- view 没有结构设计能力时隐藏动作，而不是调用 table-only command。

### 6.6 Paste / Drop / Multiple Selection

- 粘贴解析不执行 CSV 公式、不解释 SQL 表达式、不读取剪贴板以外的数据。
- 超限先提示再退出，文档保持不变。
- drag payload 要校验 JSON、version、kind、必填字段和长度；非法外部 payload 忽略。
- 从其他连接拖入时拒绝；从同连接不同 session 拖入时按当前 connection identity 再解析，不使用 source dbSessionId 发命令。
- column alias 仅在当前语义模型唯一映射时添加。
- Mod+D 不能破坏 CodeMirror 的 find next；keymap 优先级要有组件测试。

### 6.7 参数

- parser 和 binder 对同一 token 的判断必须一致；共享 fixture 是发布门禁。
- 重复 `:id` 只有一个输入框；`:id`、`@id`、`${id}` 共享值但各 occurrence 独立替换。
- 每个 `?` 都有单独输入框和 history ID，删除前一个后 ordinal 重新计算，并以本次 target 快照为准。
- value coercion 继续支持 null/boolean/number/string；引号和 escape 始终由 Host 完成。
- 参数替换不允许改变 identifier、keyword 或 SQL 结构；`${table}` 仍得到字符串 literal。

### 6.8 风险

- WHERE 必须属于目标 UPDATE/DELETE 顶层；CTE、subquery、字符串或注释中的 WHERE 不计。
- 多语句 target 汇总全部 findings，确认框显示最高等级并列出受影响 statement index。
- unknown 在 production 下确认，在非 production 下按现有行为执行并记录 classifier diagnostic。
- 前端 assessment 与后端拒绝不一致时，以后端为准，错误面板应明确 Safe Mode/readOnly 原因。

### 6.9 AI

- prompt 复用现有脱敏函数；新增测试覆盖 URI credential、JSON secret、password assignment 和参数字面量。
- prompt SQL/error 各自限制 4,000 字符，schema 摘要限制 relation/column 数量。
- 按钮只在存在 query error 和 callback 时显示；Retry/Fix/Explain 顺序保持。
- 预填失败不得影响错误面板其他动作。

## 7. 测试矩阵与质量门禁

### 7.1 TypeScript 纯逻辑测试

- scanner/range：所有 quote/comment/delimiter/cursor 边界及原始 offset。
- semantic：alias、CTE、nested、shadowing、incomplete、ambiguity。
- adapter：quote/escape/fold/policy，通过注入 metadata 测通用逻辑。
- metadata：identity、dedupe、TTL、invalidate、session isolation。
- completion：alias detail、FK direction/composite/multiple、自关联、fallback。
- intention：star 分类、qualifier 唯一性、transaction changes。
- paste：分隔、quote、escape、limit、prefix。
- params/history：五 syntax、ordinal、混用、敏感 history、storage failure。
- risk：顶层 WHERE、multi-statement、production matrix。
- AI prompt：bounded、redacted、无内部 IDs。

新增/核心纯模块要求 statement/branch/function/line coverage 均不低于 80%；安全、binder、range、risk 目标 90% 以上。不能用无意义 snapshot 凑覆盖率。

### 7.2 CodeMirror 组件测试

- extension 装配、reconfigure、dispose。
- frame/gutter/running、keyboard target。
- completion、signature、Alt+Enter transaction。
- inlay viewport、hover timer、navigation callbacks。
- clipboard/drop caret/multiple selections。

jsdom 不断言真实像素布局；几何计算抽纯函数，折行/滚动视觉交给 E2E。

### 7.3 QueryPanel / ContentView 集成测试

- target 到 `executeSelection/executeQuery` 的准确链路。
- 仅 target 参数参与绑定。
- execution gate 的安全矩阵和确认快照。
- metadata prefetch、cache refresh 和 session 切换。
- Error → Chat callback、草稿脱敏、未配置 AI。
- 保留 Retry fingerprint、Fix SQL、Diagnosis、多结果集和取消。

### 7.4 Rust Host 测试

- 新 parameter token 的 literal replacement、旧 payload 兼容、quote/comment/dollar quote。
- execute 与 stream path 行为一致。
- replacement 后再执行 `check_sql`。
- Safe Mode/readOnly/no-WHERE 现有测试全部保留。

这些是 Host 通用能力测试；具体驱动方言不放 Host。

### 7.5 合流后的标准命令

每轨先跑 targeted tests。每个 stage 合流后至少运行：

```bash
pnpm test:unit
cargo test -p datazen --lib
```

涉及 path driver 时追加：

```bash
cargo test -p datazen-driver-<id>
pnpm test:unit:drivers
```

最终 R 阶段按 E2E 文档构建 webdriver debug app，并运行相关 Host E2E。开发验收要求 en/zh 类型和测试通过；发布前补齐全语言后再要求 `i18n-sync-check` 返回 0，仅改 en/zh 时该脚本报告其他语言缺 key 不等同于功能失败。

## 8. 子代理执行协议

### 8.1 角色

- Coordinator：维护集成分支、简报、台账、合流和门禁；不写业务代码。
- Coder：只在自己的 worktree 和文件写范围内实现；完成后只能报 `READY_FOR_TEST`，不得自称通过。
- Tester：必须是与 Coder 不同的新实例；审查代码、重跑测试、补测试和 E2E，但不得修业务实现。
- R Tester：所有阶段完成后做独立全局回归、架构审计和性能验收。

### 8.2 每个 Coder 简报必备字段

1. 轨道 ID、目标、base commit、integration branch、绝对 worktree 路径和分支名。
2. 必读的仓库 `AGENTS.md`、subagent coder 手册、本方案和本轨 progress/bugs 路径。
3. 允许修改/新建的精确真实路径和禁止修改的热点文件；“相关文件”“必要 selector”等占位词必须在派发 manifest 中展开。需要扩大写锁时先停下，由 Coordinator 仲裁并广播。
4. 已冻结的输入输出 contract、消费的 contract commit、产出的 contract 和 ownership transfer。
5. BOOTSTRAP 检查：cwd、branch、HEAD、clean status；Rust 轨设置独立 `CARGO_TARGET_DIR`。
6. 按顺序执行的实现步骤、每 5 分钟 heartbeat、卡住时的证据格式。
7. 必测边界、targeted command、四项 coverage 目标、该轨唯一 E2E spec。
8. dbx 行为映射和 clean-room 禁令。
9. 完成定义、回滚点、已知非目标、最终 commit 和实际测试数字。
10. 禁止 `pnpm install`、禁止 merge/rebase/cherry-pick、禁止碰 hub、禁止修改无关用户变更。

### 8.3 每个 Tester 简报必备字段

1. 与 Coder 不同的新实例、绝对 worktree/branch、待测 coding commit 和精确 diff range。
2. 必读 Tester 手册、AGENTS、本方案、本轨 progress/bugs 和 Coder 报告。
3. A：代码审查；B：独立重跑；C：coverage 驱动补测并补该轨 Host E2E；D：PASSED/FAILED 判定。
4. Tester 可修改测试、fixture、测试文档和 bugs/progress，禁止修业务实现。
5. 新模块四项 coverage 不低于 80%，安全/binder/range/risk 目标 90%；无法度量的 Rust 分支需记录人工分支审查证据。
6. 检查 dbx clean-room、文件限长、driver 测试落点和文件写锁越界。
7. FAILED 时一次性记录所有已发现 Bug；PASSED 时报告命令、测试数、E2E、coverage、测试 commit 和 clean status。

### 8.4 状态与 Bug 流转

标准状态与 playbook 保持一致：`DISPATCHED → BOOTSTRAP → CODING → READY_FOR_TEST → TESTING → PASSED → READY_TO_MERGE → MERGED → CLEANUP → CLOSED`。失败循环使用 `BUG_RECORDED → REPAIRING → READY_FOR_TEST`。

- Tester 将所有问题一次性写入本轨 `bugs.md`，包含 ID、严重级、状态、复现、期望/实际、证据、归属和建议测试。
- `FAILED` 后恢复原 Coder 修复，不新开不知上下文的 Coder。
- 每次修复后必须启用新的 Tester 实例复测。
- 同一轨最多 5 个修复循环，仍失败标为 `ESCALATED`，由协调代理决定拆轨、改 contract 或请求产品决策。
- 子代理只编辑自己的 `progress.md` / `bugs.md`；协调代理运行 `node scripts/aggregate-hub.mjs` 聚合总览。
- 修复后开放态统一写 `待验证(修复后)`，以便聚合脚本识别；合入门仍人工检查所有非“已修复/关闭”状态，不能只相信 hub 数字。
- Agent 超过 20 分钟无 heartbeat 才判定失活；Coordinator 最多连续 3 次带新证据继续原 agent，仍无进展才按手册升级 Rescuer。

### 8.5 文件锁、Wave 与合流

- 同一 wave 内最多三个 Coder；任何热点文件只有一个 owner。
- 同一 wave 的 Coder 从同一最新 integration base 同批派发；任一 Coder `READY_FOR_TEST` 后立即启动独立 Tester，不等待其他 Coder。
- Coder 开工和恢复前检查 worktree、branch、HEAD、未提交变更。
- 只有 `PASSED` 且无 open bug 才能 `--no-ff` 合入 integration branch。
- 每次合流后立即运行 typecheck 和该轨 targeted tests；失败则停止后续合流并恢复责任轨。
- locale、主 E2E specs、coverage 文档由收口轨单一 owner 修改。
- 不编辑用户已有的无关改动，不提交 codegen/gitignored 文件。
- 本方案不在 coordination 目录，所有 progress 必须显式记录 `Task` 和 `Plan: /Users/flyxl/code/datazen/docs/todo/sql-editor/implementation-plan.md`，保证聚合总览能显示任务；也可由 Coordinator 创建只含链接和轨道索引的 coordination plan 指针。
- 每个 wave 派发前生成写锁 manifest：track、base commit、allow create/modify、deny hotspots、consumed/produced contract、E2E spec、locale key 清单。后续 ownership transfer 必须在新 wave manifest 明写。

## 9. 验收追踪清单

最终 R Tester 必须逐项给出证据，不能只写“全部通过”。

- AC-01：多语句时 frame 准确，选区隐藏，500+ 行降级。
- AC-02：每条有效语句首行有 gutter play，running 精确归属。
- AC-03：Mod+Enter selection/current、Mod+Shift+Enter all 行为正确。
- AC-04：alias/CTE/scope 容错，歧义不猜。
- AC-05：`alias.` 只返回目标列，显示类型/nullable/comment。
- AC-06：FK JOIN 支持方向、复合键、多候选和自关联。
- AC-07：函数签名正确识别 nested call 和当前参数。
- AC-08：Alt+Enter 展开 star、加/移 qualifier 可一次 undo。
- AC-09：INSERT hint 对位正确、viewport 限制、设置可持久化。
- AC-10：table hover 300ms、metadata 有界展示、动作可用。
- AC-11：Mod/Ctrl-click 打开完整 identity 的目标对象。
- AC-12：Paste as IN 支持常见分隔、转义、prefix 和上限。
- AC-13：table/column drag 有 drop caret、smart quote、身份校验和 legacy 兼容。
- AC-14：Mod+D 和 Alt-drag 多选可用，undo/selection 正常。
- AC-15：五类参数从 UI 到 Host 可执行，历史按连接隔离最近 5 条。
- AC-16：Retry/Fix/Explain 无回归，Ask in Chat 只预填脱敏草稿。
- AC-17：Safe Mode/readOnly 无绕过；production/high-risk 统一确认。
- AC-18：本项目所有新增生产文件及被本项目实质修改的既有生产文件少于 500 行，无 driver ID Host 硬编码；不扩张为无关全仓重构。
- AC-19：20k 文档活动语义解析 p95 `<5ms`，交互无长任务，目标 60fps；未达标只能 FAILED，或附用户/产品书面 waiver。
- AC-20：Host UI journeys 有 E2E，驱动专属测试落点正确。
- AC-21：生产/测试代码没有 `docs/todo/sql-editor/dbx` import 或机械翻译结构。

## 10. 决策门与已知风险

### 10.1 必须在相关轨道开工前确认

- 表注释：默认采用“有数据则显示，无数据则隐藏”。若改为强制显示，需单独批准 Driver API / protocol migration，不能由 hover Coder 顺手修改。
- Git drivers 的 quote metadata：若源码不在当前仓库，建立外部插件任务；Host 不增加临时特判。
- MySQL `DELIMITER` / routine body：首版保守降级为单范围；若要求完整支持，作为 parser 后续轨道。
- Ask in Chat：本方案出于隐私默认“预填但不自动发送”；PRD 中“发送”的字面若要求一次点击即调用模型，必须由产品明确批准，并追加 egress/streaming/未配置策略测试。
- 生产高危：本方案默认把“生产确认”和“高危确认”合并为一次最高等级弹窗；若“二次确认”字面要求连续两次弹窗，需产品明确批准并重写安全矩阵，禁止各入口自行决定。
- ClickHouse 引号：当前 metadata 与 PRD 描述不同，修改会影响编辑器之外的 SQL generator；必须先用驱动文档/测试确认目标行为。

### 10.2 主要风险及缓解

- 解析与执行范围不一致：统一 scanner，所有消费者只用 range contract。
- 大文档卡顿：活动 range、viewport、缓存、debounce、同步 snapshot、固定性能门禁。
- metadata 风暴：inflight dedupe、batch ensure、error TTL、session invalidation。
- 参数误识别/注入：TS/Rust 共享 fixture、Host typed literal、方言 policy、禁止 raw SQL。
- 安全入口漏网：单一 execution gate，静态搜索所有 execute 调用，Host guard 保底。
- 跨层 UI 耦合：编辑器只发 callbacks，ContentView 唯一协调导航和 Chat。
- 并行冲突：Stage 1 先拆热点，后续单一 owner、独立新增模块、locale/E2E 最后收口。
- 参考实现版权与架构污染：clean-room 简报、无 import、Tester 对照行为而非源结构。
- 性能测试抖动：普通 CI 宽松回归，固定机器严格 p95/trace，保存原始结果。

## 11. 最终交付物

完成项目时应同时具备：

1. 模块化 SQL Editor、语义、metadata、completion、intentions 和 CodeMirror extensions。
2. 统一 Query execution gate、新参数 binder/history、安全确认和 AI 草稿桥。
3. Schema tree table/column versioned drag payload。
4. INSERT hint setting 及英文/中文文案。
5. Host/driver 单测、CodeMirror 组件测试、QueryPanel/ContentView 集成测试。
6. 五组 Host E2E journeys 与更新后的 E2E coverage 文档。
7. 固定性能 fixtures、测量脚本、结果记录和降级说明。
8. 每轨 progress/bugs 台账、聚合 hub、最终 R Tester 报告。
9. dbx clean-room 审计结论及无运行时依赖证明。

只有当 AC-01～AC-21 均有可重复证据、无未关闭 P0/P1/P2 bug、完整门禁通过，才能将该 PRD 标记为实施完成。
