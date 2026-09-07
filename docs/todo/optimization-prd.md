# 产品需求文档 (PRD)：DataZen 查询交互体验提升、Schema 架构整顿与多模型 AI 独立安全门控

| 文档属性 | 详情说明 |
| :--- | :--- |
| **文档名称** | DataZen 查询体验优化、Schema 架构治理与多模型安全门控需求规格说明书 |
| **存放路径** | `docs/todo/optimization-prd.md` (镜像同步于 `todo/optimization-prd.md`) |
| **涉及模块** | `src/windows/connection/`、`src/windows/welcome/`、`src/components/sql-editor/`、`src/stores/`、`src-tauri/src/ai/`、`src-tauri/src/commands/`、`packages/ai-api/` |
| **核心基准** | 遵循 `AGENTS.md` 规范：Single Source of Truth (SSOT)、单文件规模严格限制、严禁硬编码、安全零回归 |
| **文档版本** | v1.0.0 (正式设计发布版) |
| **发布日期** | 2026-09-07 |

---

## 目录
1. [背景与需求全景](#1-背景与需求全景)
2. [需求一：连接默认页点击历史查询直达 SQL 编辑器](#2-需求一连接默认页点击历史查询直达-sql-编辑器)
3. [需求二：首页“最近 SQL”中“查看全部”解耦与优化方案](#3-需求二首页最近-sql中查看全部解耦与优化方案)
4. [需求三：Schema 获取实现审计与单一可信源（SSOT）治理规范](#4-需求三schema-获取实现审计与单一可信源ssot治理规范)
5. [需求四：DML/DDL 执行结果状态反馈重构（解决 PG INSERT 误提示）](#5-需求四dmlddl-执行结果状态反馈重构解决-pg-insert-误提示)
6. [需求五：查询无结果/非图表化时图表按钮 Disabled 规范化](#6-需求五查询无结果非图表化时图表按钮-disabled-规范化)
7. [需求六 & 七：多模型支持与模型级独立 AI 安全门控系统](#7-需求六--七多模型支持与模型级独立-ai-安全门控系统)
8. [模块依赖与接口变更清单](#8-模块依赖与接口变更清单)
9. [实施路线图与测试验收规范](#9-实施路线图与测试验收规范)

---

## 1. 背景与需求全景

DataZen 在日常数据库开发运维与 AI 协同过程中，暴露出多处交互断层、架构重复以及安全门控“一刀切”的痛点：
1. **连接工作区断层**：打开连接后的默认页（`ConnectionWorkspaceHome`）展示了历史 SQL，但卡片仅可点击“复制”，无法一键在当前连接下开辟查询标签并带入 SQL 执行。
2. **首页历史与连接绑定割裂**：首页“最近 SQL”的“查看全部”强行唤起某个连接并打开右侧边栏抽屉，而侧边栏抽屉实质是与单个连接强绑定的，混淆了“全局历史”与“连接私有历史”。
3. **Schema 获取架构冗余与缓存失效死角**：前端存在多套 Store、组件级直接调用 IPC 旁路缓存；后端存在重复端点，且 `SchemaCache` 的主动失效（Invalidate）在生产 DDL 路径中从未被触发。
4. **DML 结果反馈误导**：PostgreSQL 执行 `INSERT/UPDATE/DELETE/DDL` 等非查询语句后，因其结果集行数为 0，界面提示“暂无可显示的行 此查询没有返回任何行。”，造成执行失败或查无数据的误解。
5. **图表切换按钮视觉状态缺失**：查询无结果或无法数值化时，图表切换按钮缺少清晰的 Disable 视觉样式与原因 Tooltip。
6. **AI 安全门控一刀切**：当前的 `aiStrictEgress` 粗暴擦除所有结果行，无法满足本地私有模型（如 Ollama）或受信任企业内网模型对数据样例分析的诉求。
7. **单模型配置局限**：系统仅支持保存单一 AI 模型配置，无法针对不同场景（快速生成、深度推理、本地离线）配置多个模型，且各模型的安全边界无法独立设定。

---

## 2. 需求一：连接默认页点击历史查询直达 SQL 编辑器

### 2.1 现状与痛点
- 源码定位：`src/windows/connection/ConnectionWorkspaceHome.tsx`（第 542–588 行）。
- 当连接建立但尚未打开任何查询/数据 Tab 时，呈现默认首页 `ConnectionWorkspaceHome`。
- 其底部的 `recentQueries` 仅提供复制按钮（`Copy`），用户若想重跑该 SQL，必须经过“复制 SQL -> 点击顶部新建查询按钮 -> 等待编辑器加载 -> 粘贴 SQL”共 4 步繁琐交互。

### 2.2 优化规格设计
1. **交互行为定义**：
   - 鼠标悬停在历史 SQL 卡片上时，卡片呈现可点击视觉态：`cursor-pointer`、边框亮起 `hover:border-accent/50`、背景微高亮 `hover:bg-surface-raised`。
   - 点击卡片主体区域（非复制按钮区），立即调用打开查询面板动作，并注入该卡片的 `sql` 语句以及所属目标 `database`。
   - 卡片右上角原有的【复制按钮】保留，维持 `e.stopPropagation()`，点击仅复制文本并提示成功，不触发页面跳转。
2. **编辑器聚焦与光标定位**：
   - 新建并激活 `QueryPanel` 后，编辑器自动聚焦（Focus），且光标自动移动至 SQL 末尾，用户可直接按快捷键（`Cmd+Enter` / `Ctrl+Enter`）直接运行。
3. **接口扩展**：
   - 修改 `src/windows/connection/ConnectionWorkspaceHome.tsx`：
     ```typescript
     export interface ConnectionWorkspaceHomeProps {
       // ...原有属性
       /** 点击历史记录项，直接以该 SQL 打开新查询面板 */
       onSelectHistoryQuery?: (entry: QueryHistoryEntry) => void;
     }
     ```
   - 在 `ContentView.tsx` 中将 `handlers.handleNewQuery` 传入：
     ```typescript
     onSelectHistoryQuery={(entry) => {
       handlers.handleNewQuery(entry.sql, { database: entry.database });
     }}
     ```

---

## 3. 需求二：首页“最近 SQL”中“查看全部”解耦与优化方案

### 3.1 现状与冲突根因
- 源码定位：`src/windows/connection/ConnectionWorkspaceHome.tsx`（第 212–235 行 `handleOpenHistory`）、`usePanelHandlers.ts`（第 439–448 行 `handleOpenQueryHistory`）。
- **核心冲突**：
  - 侧边栏抽屉 `QueryHistoryDrawer` 的设计初衷是**连接级工具**，其数据筛选依赖于已连接会话的 `connectionId` 与 `dbSessionId`。
  - 用户在未激活具体连接的欢迎页或工作区首页点击“查看全部”时，当前代码盲目获取 `quickConnections[0]`，自动连接数据库并弹出一个新建的空白 `QueryPanel`，再拉出侧边栏。这严重违背了用户预期，且污染了用户的 Tab 工作区。

### 3.2 方案对比与评估

| 评估维度 | 方案 A：全局模态弹框（Global Query History Modal） | 方案 B：独立全屏页面（Global History Page） | 方案 C（推荐）：弹窗主导 + 支持全屏展开 |
| :--- | :--- | :--- | :--- |
| **交互连贯性** | 极佳。原地弹出，不丢失当前首页/仪表盘上下文 | 会切换工作区路由，回退时需重新定位 | 最佳。轻量交互优先，需要深度分析时一键最大化 |
| **连接隔离性** | 完全解耦。无需连接任何数据库即可查看全量历史 | 完全解耦。独立的一级视图 | 完全解耦。独立管理数据请求 |
| **查询过滤能力** | 支持按连接、数据库、状态、关键字模糊检索 | 屏幕利用率最高，可承载历史执行耗时趋势图等 | 弹窗内支持轻量过滤，全屏态支持图表与高级筛选 |
| **改造成本** | 中等，复用 Dialog 体系与虚拟列表 | 较高，需在主导航与路由系统增设一级 Tab | 首期实现弹窗，二期开放放大为独占页面 |

### 3.3 详细优化实施方案（方案 C 规格）
1. **新建全局查询历史组件**：`src/components/history/GlobalQueryHistoryDialog.tsx`。
2. **弹窗布局与能力规格**：
   - **顶部检索栏**：
     - 全局关键词搜索框（实时高亮匹配 SQL 关键字）；
     - 连接筛选下拉框（全部连接 / 指定连接名称）；
     - 状态筛选（全部 / 仅成功 / 仅失败）；
     - 时间范围选择（今天 / 近7天 / 近30天 / 全部）。
   - **内容列表区**：
     - 采用虚拟滚动渲染（Virtual List），防止上万条历史记录造成 DOM 性能卡顿；
     - 列表项包含：连接名徽标、数据库徽标、执行时间、耗时标签（ms）、行数、SQL 预览；
     - 提供代码折叠与格式化预览。
   - **动作工具栏（每条记录）**：
     - 【复制 SQL】：复制至剪贴板；
     - 【在连接中打开】：点击后，系统自动激活对应连接，开辟 `QueryPanel` 并载入 SQL。若该连接尚未连接，提示用户确认连接；
     - 【删除记录】：可单条删除历史记录，或顶部提供“清空历史”。
3. **彻底解耦代码调用链**：
   - 移除 `ConnectionWorkspaceHome.tsx` 中通过 `setPendingQueryHistory` 强行切换到 `QueryPanel` 的逻辑。
   - 首页点击“查看全部”直接设置 `isGlobalHistoryOpen = true`，呼出全局历史弹窗。

---

## 4. 需求三：Schema 获取实现审计与单一可信源（SSOT）治理规范

### 4.1 深度审计发现的问题清单

#### 4.1.1 前端：多源割裂、重复缓存与旁路直接调用
1. **存在 3 套互不相通的 Schema 缓存体系**：
   - `src/stores/schemaStore.ts`：以 Zustand store 存储 `databases`、`tables`、`columns`（`Map<string, string[]>`）。
   - `src/lib/schemaCache.ts`：以全局内存 `Map` 存储 `TableSchema`（含字段、索引、外键）与 DDL，具备 60s TTL、并发防重（`schemaInflight`）及失效广播。
   - `src/components/sql-editor/metadata/metadataCache.ts`：SQL 编辑器 AST 补全专属的 `SessionState.relations`，依赖轮询或订阅 `schemaCache` 的广播。
2. **多处组件旁路直接调用 IPC，击穿缓存**：
   - `TableStructureEditor.tsx`（第 171 行）：直接调用 `databaseCommands.getTableSchema(dbSessionId, initialTableName)`，完全无缓存。
   - `src/lib/tableSchemaForSql.ts`（第 51、62 行）：直接调用 `databaseCommands.getTableSchema` 和 `databaseCommands.getColumns`。
   - `useSchemaTreeState.ts` 与 `useNavigatorDbState.ts`：各自在 hook 内部独立调用 `databaseCommands.getTables(connectionId, dbName)`，没有写入 `schemaStore.setLoadedTables`，导致侧边栏与主工作区数据脱节。
   - `SchemaDiffWindow` 与 `DataSyncWindow`：直接发起 `databaseCommands.getTables`，没有共享主窗口已加载的表结构。

#### 4.1.2 后端：命令冗余、缓存利用不均与 DDL 失效死角
1. **API 重复与路径混乱**：
   - Tauri IPC 暴露了老旧的 `get_tables`、`get_columns`、`get_all_columns`、`get_table_schema` 等专用命令；同时又存在统一的 `execute_driver_command`（`list_tables`、`list_objects`）。
   - `get_tables_impl` 内部调用了 `run_schema_catalog_command("list_tables")`，而 `get_er_data_impl` 却绕过 Driver Command，直接调用驱动 trait 方法 `driver.get_tables(&handle, &database)`。
2. **缓存利用极度不平衡**：
   - 后端 `src-tauri/src/cache/schema_cache.rs` 实现了精细的二级缓存（Columns 层与 Full TableSchema 层），但后端仅有 `get_table_schema_impl` 查验了缓存！
   - `get_columns_impl`（第 67 行）和 `get_all_columns_impl`（第 97 行）直接呼叫驱动，完全绕过了 `SchemaCache::get_columns`。
3. **重大漏洞：生产 DDL 执行未触发缓存失效**：
   - 全局搜索 `schema_cache.invalidate` 发现：**该失效方法仅在单元测试中被调用**！
   - 用户在 SQL 编辑器执行 `CREATE TABLE`、`DROP TABLE`、`ALTER TABLE` 等语句后，后端的 `SchemaCache` 依然持有脏数据，只能干等 300 秒 TTL 自然超时，极易造成执行后元数据不一致。

### 4.2 SSOT（单一可信源）重构规范

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        前端唯一可信入口 (UI & Editor)                    │
│                 schemaStore (统一管理 Database / Table 树)              │
│                                    │                                   │
│                                    ▼                                   │
│       schemaCache.ts (统一内存级 TableSchema / DDL / Inflight / TTL)    │
│                                    │ (IPC: execute_driver_command)     │
└────────────────────────────────────┼───────────────────────────────────┘
                                     │
┌────────────────────────────────────▼───────────────────────────────────┐
│                           后端 Tauri IPC 边界                          │
│               统一收口至 schema.rs / execute_driver_command            │
│                                    │                                   │
│                                    ▼                                   │
│           SchemaCache (后端二级缓存: Columns & Full Schema)             │
│            ▲                                               │           │
│            │ (DDL 执行拦截器主动触发 Invalidate)              │ (未命中)  │
│            │                                               ▼           │
│    QueryExecutor ───> DatabaseDriver (Postgres/MySQL/SQLite...)        │
└────────────────────────────────────────────────────────────────────────┘
```

1. **前端统一门面**：
   - 将 `src/lib/schemaCache.ts` 提升为全前端唯一的 TableSchema/DDL 获取源；
   - 废弃并重构 `TableStructureEditor`、`tableSchemaForSql` 等直接调用 `databaseCommands.getTableSchema` 的代码，统一改用 `getCachedTableSchema`；
   - `useSchemaTreeState` 和 `useNavigatorDbState` 的表列表加载统一走 `schemaStore.loadTables()`。
2. **后端缓存一致性修复**：
   - `get_columns_impl` 强制改用 `state.schema_cache.get_columns`；
   - 在 `QueryExecutor` 及 DDL 拦截层中，一旦识别到 `CREATE / ALTER / DROP / RENAME` 动词执行成功，**强制调用** `state.schema_cache.invalidate(&connection_id, &database, table_name)`，并向前端广播 `schema:invalidated` 事件，彻底消灭元数据脏读。

---

## 5. 需求四：DML/DDL 执行结果状态反馈重构（解决 PG INSERT 误提示）

### 5.1 根因定位与问题分析
1. **数据流分析**：
   - 在 `packages/drivers/postgres/src/execution.rs` 中：
     执行 `INSERT INTO ...` 时，底层执行 `sqlx::query().execute()`，产生 `StatementResult { columns: [], rows: [], rows_affected: Some(1), execution_time_ms: 12 }`。
   - 前端接收到结果后，`ResultWorkspace.tsx` -> `ResultTableView.tsx` 将该结果直接喂给 `<DataTable columns={[]} rows={[]} />`。
   - `DataTable.tsx`（第 519–527 行）判断 `rows.length === 0`，直接显示：
     ```html
     <span>暂无数据</span>
     <span>此查询没有返回任何行。</span>
     ```
2. **用户体验缺陷**：
   - 用户成功插入了 1 条甚至 10,000 条记录，看到醒目的“此查询没有返回任何行”，以为数据并未落库或语句报错，造成严重的心理负担与重复操作风险。

### 5.2 重构规格说明
1. **语句结果类型判定（Result Kind Classifier）**：
   定义纯函数 `isMutationExecution(result: StatementResult): boolean`：
   - 当满足以下条件之一，判定为 **DML/DDL 执行反馈**，而非空查询结果：
     1. `result.columns.length === 0 && result.rows.length === 0 && result.rowsAffected != null`;
     2. 语句以 `INSERT`、`UPDATE`、`DELETE`、`CREATE`、`ALTER`、`DROP`、`TRUNCATE`、`GRANT`、`REVOKE`、`SET`、`BEGIN`、`COMMIT` 开头，且未带 `RETURNING` 产生数据集。
2. **新建执行摘要视图组件（ExecutionSummaryCard）**：
   - 在 `src/windows/connection/result-workspace/` 新建 `ExecutionSummaryCard.tsx`。
   - 当 `isMutationExecution(result)` 为真时，隐藏 `<DataTable />`，替换为整洁专业的执行成功面板：
     - **状态图标**：绿色圆环 Check 图标（`CheckCircle2`）；
     - **主要提示语**：`执行成功 (Query OK)`；
     - **核心指标条**：
       - `受影响行数`：`result.rowsAffected` 行（如 `1 行受影响 (1 row affected)`）；
       - `执行耗时`：`result.executionTimeMs ms`；
       - `执行时间戳`：精确到秒的时间标记；
     - **执行 SQL 预览**：高亮展示当前执行的语句片段；
     - **快捷动作**：提供“复制执行摘要”、“查看表结构/数据”。
3. **多结果集（Multi-Statement）适配**：
   - 若用户执行批处理脚本（如包含一个 INSERT 和一个 SELECT）：
     - Tab 1（INSERT）：展示 `ExecutionSummaryCard`；
     - Tab 2（SELECT）：展示常规 `DataTable` 数据表格。

---

## 6. 需求五：查询无结果/非图表化时图表按钮 Disabled 规范化

### 6.1 现状与缺陷
- 源码定位：`src/windows/connection/result-workspace/ResultWorkspace.tsx`（第 122–138 行）、`src/windows/workflow/WorkflowExecutionResultPanel.tsx`（第 147–160 行）。
- **问题 1：视觉无禁用态**。`ResultWorkspace` 的图表按钮虽然加了 HTML `disabled={!resolution.chartAvailable}`，但 Tailwind 类名仅有 `text-fg-muted hover:text-fg-secondary`，没有 `disabled:` 变体样式，悬停仍产生高亮效果，光标仍为常规箭头，毫无禁用感知。
- **问题 2：工作流面板漏设禁用**。`WorkflowExecutionResultPanel` 甚至完全没有绑定 `disabled` 属性，空数据或纯字符串结果集依然可以点击切到图表视图，导致内部抛出空白画布。
- **问题 3：缺少原因解释**。用户不知道为什么按钮不可用。

### 6.2 规范化修改规格
1. **统一禁用条件**：
   - 禁用判定条件：`!resolution.chartAvailable`，即：
     `result == null || result.rows.length === 0 || result.columns.length === 0 || !isChartableResult(result)`。
2. **视觉样式规范**：
   ```tsx
   className={cn(
     'flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors',
     resolution.view === 'chart'
       ? 'bg-accent/20 font-medium text-accent'
       : 'text-fg-muted hover:text-fg-secondary',
     // 明确的禁用视觉样式
     !resolution.chartAvailable &&
       'cursor-not-allowed opacity-40 hover:bg-transparent hover:text-fg-muted pointer-events-auto',
   )}
   ```
3. **智能 Tooltip 提示说明**：
   - 当按钮处于禁用状态时，外裹 Tooltip：
     - 若 `result.rows.length === 0`：显示文案“查询结果为空，无法生成图表 (Query returned no rows)”；
     - 若 `!isChartableResult(result)`：显示文案“结果集中未包含数值类型字段，无法生成图表 (No numeric fields available for charting)”。
4. **全端同步覆盖**：
   - 同步修复 `WorkflowExecutionResultPanel.tsx`，补齐 `disabled` 状态、样式与 Tooltip。

---

## 7. 需求六 & 七：多模型支持与模型级独立 AI 安全门控系统

### 7.1 业务背景与架构动机
- **告别“一刀切”的必要性**：
  - 数据安全门控（Safety Gate）不仅是简单的“外发不外发”，而是分级控制。公网闭源模型（OpenAI GPT-4o、Claude 3.5 Sonnet）必须执行严苛脱敏（禁止传输真实数据行、强制凭证擦除）；而本地模型（通过 Ollama、vLLM 私有化部署）运行在本地算力或内网隔离机房，用户希望 AI 能结合数条真实结果样例进行深度诊断，此时“一刀切的行擦除”直接废掉了 AI 的诊断分析能力。
- **多模型配置的必要性**：
  - 开发者在不同场景有不同模型偏好：日常快速补全使用 DeepSeek-V3 / Ollama 本地模型，复杂建表/报表推理使用 Claude 3.7 Sonnet，紧急故障排查使用 GPT-4o。单模型配置迫使用户每次都要去设置页重新填 API Key 和 Endpoint，体验极其割裂。

### 7.2 数据结构设计（Schema & Types）

#### 7.2.1 细粒度安全门控类型定义（`AiSafetyGateConfig`）
在 `packages/ai-api/src/types.rs` 与前端 `src/types/` 中定义：

```typescript
/** 数据外发脱敏级别 */
export type AiDataEgressLevel = 
  | 'strict'        // 严格模式：擦除所有结果集、数据行、样本以及敏感 Key
  | 'sample_masked' // 样例脱敏模式：允许携带不超过 N 条样本行，但敏感字段自动掩码（默认脱敏敏感词）
  | 'unrestricted'; // 完全放行模式：允许携带完整上下文数据行（仅建议用于本地 Local / 内网私有模型）

/** 工具调用权限策略 */
export type AiToolPermissionPolicy = 
  | 'disabled'          // 禁止该模型调用任何工具
  | 'read_only'         // 自动允许只读工具（如查询表列表、读取 schema），阻断写操作
  | 'require_confirm'   // 任何工具调用均需在前端弹窗人工确认
  | 'unrestricted';     // 完全自动执行工具调用

/** 模型独立安全门控配置 */
export interface AiSafetyGateConfig {
  /** 凭证脱敏（密码、Token、私钥等），不可关闭，始终为 true */
  redactCredentials: true;
  /** 数据行与结果集外发策略 */
  dataEgressLevel: AiDataEgressLevel;
  /** 当 egress 为 sample_masked 时允许的最大样本行数 (默认 3，范围 1~10) */
  maxSampleRows: number;
  /** 内置 DB 工具权限策略 */
  dbToolPolicy: AiToolPermissionPolicy;
  /** 外部 MCP 客户端工具权限策略 */
  mcpToolPolicy: AiToolPermissionPolicy;
  /** AI 生成的 SQL/DDL 是否必须弹出确认执行对话框 */
  requireSqlConfirm: boolean;
  /** 最大 Prompt 上下文预算大小 (字节，默认 32KB) */
  maxContextBytes: number;
}
```

#### 7.2.2 多模型配置模型（`AiModelProfile`）
```typescript
export interface AiModelProfile {
  id: string;                    // UUID
  name: string;                  // 自定义显示名称，如 "本地 Ollama (Qwen 2.5)"、"云端 Claude 3.7"
  providerType: AiProviderType;  // 'open_ai' | 'anthropic' | 'deep_seek' | 'ollama' | 'custom'
  endpoint?: string;
  apiKey?: string;
  model: string;                 // 具体模型标识符
  maxTokens: number;
  extra?: Record<string, unknown>;
  /** 该模型绑定的独立安全门控 */
  safetyGate: AiSafetyGateConfig;
  /** 预设标记：是否为系统推荐预设 */
  isDefault?: boolean;
}

/** 多模型持久化聚合体 */
export interface AiSettingsConfig {
  activeProfileId: string;
  profiles: AiModelProfile[];
}
```

### 7.3 安全门控预设策略矩阵 (Preset Profiles)
为避免用户配置繁琐，系统提供 3 档开箱即用的门控模板：

| 门控维度 | 级别 1：公网云端严格级 (Cloud Strict) | 级别 2：内网企业平衡级 (Enterprise Balanced) | 级别 3：本地私有信任级 (Local Trust) |
| :--- | :--- | :--- | :--- |
| **适用场景** | OpenAI / Anthropic / 公网商业 API | 内部自建网关 / 专线托管大模型 | 本机 Ollama / Local vLLM / 局域网 |
| **凭证擦除** | 强制激活 (`true`) | 强制激活 (`true`) | 强制激活 (`true`) |
| **数据外发** | `strict`（彻底清空行数据） | `sample_masked`（最多 3 行样本脱敏） | `unrestricted`（保留完整上下文结果） |
| **DB 工具权限** | `read_only`（只读） | `require_confirm`（需人工确认） | `unrestricted`（全自动执行） |
| **MCP 工具** | `disabled` | `require_confirm` | `unrestricted` |
| **SQL 确认** | 强制二次确认 (`true`) | 强制二次确认 (`true`) | 可由用户选配关闭 |

### 7.4 UI 交互设计规格

#### 7.4.1 设置页多模型管理（`AiSettingsSection.tsx`）
1. **模型列表卡片视图**：
   - 顶部提供【添加模型】按钮；
   - 列表展示已配置的模型卡片（包含模型名称、Provider 徽标、Endpoint 地址、当前安全门控级别徽标、默认标记）；
   - 卡片右侧具备：【设为默认】、【编辑】、【复制】、【删除】操作。
2. **模型编辑对话框（Profile Drawer / Dialog）**：
   - **基础信息 Tab**：Provider、API Key、Endpoint、拉取模型下拉选单、Max Tokens；
   - **安全门控 Tab (Safety Gate)**：
     - 提供一键选择模板：`[公网严格] [企业平衡] [本地信任]`；
     - 展开自定义配置：数据外发脱敏级别单选、允许携带样本行数滑块（1~10）、工具调用权限下拉选单、执行二次确认开关；
     - 当用户试图将公网商业 API（如 OpenAI / DeepSeek 公共端点）配置为 `unrestricted` 时，界面弹出橙色高危安全警告对话框，明确告知数据泄露风险并要求二次确认。

#### 7.4.2 运行时模型快速切换
1. **AI Chat 侧边栏（`AiChatPanel.tsx`）**：
   - 顶部工具栏增设模型选择下拉菜单：显示当前使用的 Model Profile 及安全盾牌图标（绿色为本地信任，黄色为平衡，蓝色为严格）；
   - 切换模型后，立即提示门控策略变动；
   - 输入框下方即时更新脱敏状态提示（如：“当前为本地信任模式：允许携带真实错误行数据辅助排错”）。
2. **SQL 编辑器内联 AI 诊断与解释**：
   - 诊断与自动修复根据当前默认 Model Profile 的安全门控执行上下文脱敏。

### 7.5 后端安全管道改造（`src-tauri/src/ai/safety.rs`）
1. 升级核心脱敏函数：
   ```rust
   pub(crate) fn redact_for_profile_egress(
       value: &str, 
       gate: &AiSafetyGateConfig
   ) -> String
   ```
2. **多级过滤逻辑**：
   - **第一层（凭证层，无条件执行）**：无论何种模式，URL 凭据、Bearer Token、`password/secret/apiKey` 等敏感字段统一擦除为 `[REDACTED]`。
   - **第二层（结果行数据层）**：
     - `strict`：丢弃 `rows/record/data` 等结果行字段；
     - `sample_masked`：保留不超过 `gate.max_sample_rows` 条记录，并对常见手机号、邮箱、身份证正则进行局部星号掩码；
     - `unrestricted`：在 `max_context_bytes` 范围内完整保留 JSON/文本数据。
3. **IPC 调用升级**：
   - `ai_chat`, `ai_generate_sql`, `ai_diagnose_error`, `ai_explain_query` 等命令新增可选入参 `profile_id: Option<String>`；
   - 后端统一调度函数 `resolve_ai_with_gate(&state, profile_id).await -> (Arc<dyn AiProvider>, AiModelProfile)`。

---

## 8. 模块依赖与接口变更清单

### 8.1 前端代码变更明细
| 文件路径 | 变更类型 | 职责说明 |
| :--- | :--- | :--- |
| `src/windows/connection/ConnectionWorkspaceHome.tsx` | 修改 | 历史卡片绑定点击直接打开查询面板；解耦“查看全部”逻辑 |
| `src/windows/connection/ContentView.tsx` | 修改 | 透传 `onSelectHistoryQuery` 至 Home，调用 `handlers.handleNewQuery` |
| `src/components/history/GlobalQueryHistoryDialog.tsx` | 新增 | 全局历史查询模态弹窗，支持全维度过滤与一键打开 |
| `src/windows/connection/result-workspace/ResultWorkspace.tsx` | 修改 | 增加执行型语句识别分支，渲染执行成功面板；规范图表按钮 Disabled |
| `src/windows/connection/result-workspace/ExecutionSummaryCard.tsx` | 新增 | DML/DDL 执行成功专业反馈卡片（受影响行、耗时、无误导文案） |
| `src/windows/workflow/WorkflowExecutionResultPanel.tsx` | 修改 | 图表按钮补齐 Disabled 判定、样式与 Tooltip |
| `src/stores/aiStore.ts` & `src/types/ai.ts` | 修改 | 增加多模型 Profile 管理、当前活跃 Profile 切换与安全门控字段 |
| `src/windows/settings/AiSettingsSection.tsx` | 重构 | 多模型卡片列表展示、编辑 Drawer、独立安全门控配置面板 |
| `src/components/ai/AiChatPanel.tsx` | 修改 | 顶部增设模型与安全门控快捷切换选择器 |

### 8.2 后端代码变更明细
| 文件路径 | 变更类型 | 职责说明 |
| :--- | :--- | :--- |
| `packages/ai-api/src/types.rs` | 修改 | 新增 `AiSafetyGateConfig`、`AiModelProfile`、`AiSettingsConfig` 类型 |
| `src-tauri/src/store/ai_config.rs` | 重构 | `ai_config.enc` 存储多模型 Profiles 结构，具备旧单配置自动平滑迁移 |
| `src-tauri/src/ai/safety.rs` | 修改 | `redact_for_profile_egress` 支持三档外发控制与样本掩码 |
| `src-tauri/src/commands/ai/config.rs` | 修改 | 暴露多 Profile CRUD 命令与默认模型指定命令 |
| `src-tauri/src/commands/ai/util.rs` | 修改 | `resolve_ai` 支持传入 `profile_id` 并返回关联的安全门控 |
| `src-tauri/src/commands/schema.rs` | 重构 | `get_columns` 接入 `SchemaCache`；清理死角 |
| `src-tauri/src/services/query_executor.rs` | 修改 | DDL 执行成功后触发 `state.schema_cache.invalidate` |

---

## 9. 实施路线图与测试验收规范

### 9.1 分阶段实施路线图 (Phased Milestones)

```text
┌─────────────────────────┐     ┌─────────────────────────┐     ┌─────────────────────────┐
│       Milestone 1       │     │       Milestone 2       │     │       Milestone 3       │
│  交互与结果集反馈修复    │ ──> │   Schema SSOT 架构治理   │ ──> │  多模型与独立安全门控   │
│  (需求 1, 2, 4, 5)      │     │         (需求 3)        │     │       (需求 6, 7)       │
└─────────────────────────┘     └─────────────────────────┘     └─────────────────────────┘
```

- **第一阶段（交互与体验修复）**：
  1. 完成 `ConnectionWorkspaceHome` 历史卡片点击带 SQL 直达查询面板；
  2. 实现 `GlobalQueryHistoryDialog`，彻底剥离首页查看全部对连接侧边栏的绑定；
  3. 改造 `ResultWorkspace`，实现 `ExecutionSummaryCard`，根治 PG 执行 INSERT 误提示“没有返回任何行”；
  4. 规范化图表按钮 Disabled 视觉与 Tooltip。
- **第二阶段（Schema 单一可信源治理）**：
  1. 前端统一收口至 `schemaCache.ts`，消除 `TableStructureEditor` 等处的直调；
  2. 后端 `get_columns` 接入 `SchemaCache`；
  3. 实现 DDL 执行拦截并联动后端 `SchemaCache::invalidate` 与前端通知。
- **第三阶段（多模型与独立安全门控）**：
  1. 扩展 `ai-api` 与存储层，支持多 Profile 持久化与旧配置无缝迁移；
  2. 实现 `safety.rs` 三档外发控制（严格/样本/放行）；
  3. 改造 `AiSettingsSection` 支持 Profile 卡片管理与安全门控抽屉；
  4. 在 `AiChatPanel` 等前端入口打通模型切换与门控生效。

### 9.2 质量与测试验收标准 (QA & Verification)

#### 9.2.1 单元测试（Unit Tests）
- **前端 Vitest**：
  - `ConnectionWorkspaceHome.test.tsx`：验证点击历史卡片正确触发 `onSelectHistoryQuery`，且点击复制按钮不会冒泡触发选择；
  - `ResultWorkspace.test.tsx`：覆盖 INSERT 语句输入时正确渲染 `ExecutionSummaryCard` 而非 `<DataTable />`；
  - `ResultWorkspace.test.tsx`：验证空行或无数值列时，图表按钮具有 `disabled` 属性与 `cursor-not-allowed` 样式；
  - `GlobalQueryHistoryDialog.test.tsx`：验证关键词搜索与连接过滤筛选逻辑；
  - `aiSafety.test.ts`：验证严格、样本掩码、完全放行三种安全策略下的脱敏输出。
- **后端 Cargo Tests**：
  - `src-tauri/src/ai/safety.rs`：测试三种门控策略下的凭证擦除与样本截断；
  - `src-tauri/src/cache/schema_cache.rs`：测试 DDL 触发后的缓存精确逐出与全库逐出；
  - `src-tauri/src/store/ai_config.rs`：测试旧单模型配置向新版多 Profile 配置的自动升级兼容性。

#### 9.2.2 端到端集成测试（E2E Specs）
- 编写/扩充 `e2e/specs/query-history.ts`：验证从连接默认页点击最近 SQL，正确开辟带初值的 Query Tab 并执行成功；
- 编写 `e2e/specs/sql-editor-mutation-feedback.ts`：执行 PostgreSQL `INSERT INTO ...`，断言页面展示受影响行数与“执行成功”，且断言不包含“此查询没有返回任何行”；
- 编写 `e2e/specs/ai-multi-model-safety.ts`：配置本地测试模型与云端模拟模型，切换模型并验证发往 Provider 的 Payload 是否符合对应门控规则。
