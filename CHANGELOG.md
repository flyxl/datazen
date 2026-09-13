# 更新日志 / Changelog

本文件记录 DataZen 的显著变更，重点是影响外部契约的破坏性变更。

格式参照 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 简化版；
术语约定：**`connectionId` / `connection_id` = 配置连接 id（持久化）**，
**`dbSessionId` / `db_session_id` = 运行时数据库会话 id（内存态）**。

---

## [0.2.0] - 2026-09-13

> 自 v0.1.2 以来累计 **493** 次提交，涵盖功能新增、架构重构、质量加固与官网重塑。

### ⚠️ 破坏性变更（Breaking Changes）

- **MCP DB 工具入参改名**：所有数据库工具（`list_databases`、`list_tables`、`search_tables`、`query`、`get_schema`、`explain_query`、`describe_table` 等）的参数 `config_id` → `connection_id`，旧键名会被直接拒绝（deserialize 失败），无别名回退。
- **MCP 资源输出与模板改名**：Schema 资源 URI 模板为 `datazen://schema/{connectionId}/{database}`；`datazen://query-history` 条目 JSON 字段 `configId` → `connectionId`。
- **SQLite 历史库列名改名**：`history.sqlite` 中 `query_history.config_id` / `favorite_queries.config_id` → `connection_id`。应用启动时自动执行一次性迁移（schema v3 → v4），数据完整保留。
- **Schema Diff 剪贴板/配置 JSON 升级到 v2**：导出格式键 `configId` → `sourceConnectionId` / `targetConnectionId`（`version: 2`）。v1 格式导入会被明确拒绝。
- **数据同步任务持久化字段改名**：`sourceConfigId/targetConfigId` → `sourceDbSessionId/targetDbSessionId` + `sourceConnectionId/targetConnectionId`。旧字段名的持久化载荷将无法反序列化。
- **插件桥协议键改名**：`command.invoke` 消息参数 `configId` → `connection_id`；无别名回退。
- **命名空间重构**：`Extension` / `Plugin` → `Wapp`（Workspace App）/ `Driver`；`app-sdk` → `wapp-sdk`；`nav.connections` → `nav.databases`。

### 🚀 新功能（Added）

#### 首次运行引导向导
- 全新 **Onboarding Wizard**：首次启动时提供 3 步引导流程（连接数据库 → 探索 AI → 开始使用），内置示例 SQLite 数据库自动初始化。
- 向导支持 8 种语言（en, zh-CN, ja, ko, es, fr, de, pt），含状态机持久化，中断后可恢复。
- 连接工作区空状态引导：未连接时提供快速操作入口和示例查询一键打开。

#### SQL 编辑器增强
- **4 模式精准执行策略**：Run Current（当前语句，默认）/ Run Selection / Run All / Ask（每次询问），通过工具栏下拉选择器切换。
- **SQL Snippets 管理**：设置页新增代码片段管理卡片，支持自定义片段并通过 CodeMirror 补全扩展热插拔注入。
- **Paste as IN 批量导入**（`Mod+Shift+V`）：将多行文本自动转为 `IN ('a', 'b')` 格式。
- **Pro 扩展热插拔**：SQL Editor Pro 扩展通过 Extension Points 机制运行时加载，CodeMirror Compartment 动态重组。
- **危险执行确认**：新增 `confirmDangerousExecution` 设置项，识别无 WHERE 的 UPDATE/DELETE 时强制红色弹窗二次确认。
- **未赋值占位符拦截**：当 SQL 中含 `:param` 或 `?` 占位符但未填值时，直接拦截执行，防止隐式 NULL。

#### Schema Diff 架构升级
- **DAG 拓扑排序**：基于外键依赖构建有向无环图，按 `主表创建 → 从表创建 → 外键关联 → 索引` 严格顺序生成 DDL。
- **Driver API 迁移渲染**：Schema Diff 通过 `SchemaMigrationRenderer` trait 委托驱动层渲染方言特定 DDL，移除旧版方言模块。
- **MySQL 跨方言类型建议** + **索引前缀处理**。
- **可取消部署任务**：通过共享 Job Registry 支持 DDL 部署取消。

#### Extension Points（扩展点）体系
- **EP 核心运行时**：Extension Points 框架支持热插拔挂载、CodeMirror Compartment 动态重组。
- **EP 签名验证门禁**：`.dzx` 扩展包支持签名验证，防止未授权篡改。
- **EP 打包与发布**：`pack-ep.mjs` 打包脚本，GitHub Actions 自动上传 `.dzx` 到 Release。
- **Pro 扩展预构建快速通道**：CI 支持从 Pro 仓库下载预构建 tarball，避免源码编译。

#### Driver API 扩展
- 新增 `ObjectKind::Table` / `ObjectKind::View` 支持 `object_ddl_sql`。
- 新增 `SchemaMigrationCapabilities` 和 `SchemaMigrationRenderer` trait，驱动层可声明支持的迁移操作类型。
- 各驱动（PostgreSQL、MySQL、SQLite）实现迁移渲染器并暴露迁移能力。
- `effective_primary_keys` 移入 `TableSchema`，`get_columns` 和 `table_to_ir` 统一使用。

#### 连接工作区
- 可折叠的最近连接分区。
- 双模式侧边栏（导航/查询模式切换）。
- 集成引导栏，自动打开示例查询。

#### 安全加固
- IPC 错误 Payload 自动脱敏：密码、Token、本地文件绝对路径由正则星号模糊化。
- 导入连接配置时拒绝覆盖 `.key` 文件（除非显式 opt-in）。
- SQL Guard 增强：只读/安全模式下，注释中的写操作动词也被正确识别。

#### CI/CD 与构建
- `--edition` 与 `--drivers` 参数可同时使用，支持 `tauri:build` 和 `tauri:dev`。
- GitHub Actions 构建矩阵支持 Basic / All / Akulaku 三种变体。
- CI 入口新增 `workflow_dispatch`，支持手动触发。
- macOS bash 3.2 兼容性修复。

#### i18n 国际化
- 拆分为 **领域包（Domain Packs）** 架构：core、connection、schema、query、settings、sync、transfer、dashboard、ai、chart、backup、mcp、workflows、schemaDiff。
- `useLocaleDomains` Hook 实现子窗口按需惰性加载，减少首屏 JS 体积。
- 全端 8 语言翻译同步完成。

#### 官网重塑
- 全站去 AI 味：下载卡重构、按钮实心化、移除 emoji。
- Hero 区域重写、场景卡片、对比表升级、Demo 流程扩展。
- SEO 结构化数据、博客文章、Mid-page CTA。

### 🔧 改进（Changed）

- 全端 ID 术语统一：`connectionId`（持久化配置）/ `dbSessionId`（运行时会话），MCP、Workflow、IPC 全面对齐。
- Query Toolbar 重构为 4 个功能区 + 溢出菜单。
- 暗色主题全面刷新 + 亮色主题 Token 微调。
- `Badge` 组件统一使用 `tone` prop。
- `prettier@3.6.2` 锁定为 devDependency，移除 dlx pre-commit hook。

### 🐛 修复（Fixed）

- E2E 稳定性：滚动锁轮询、右键菜单抑制、表选择重试、并行隔离（每 worker 独立 PG 库）。
- 平台键盘快捷键对齐（Cmd vs Ctrl）。
- CSP 允许 `asset:` 协议以支持 Pro 扩展加载。
- 导航器右键菜单：Schema 根节点不再传递给 `switchDatabase`，Copy-DDL 固定到表所属数据库。
- 连接标签关闭：删除数据库/表后自动关闭对应标签。
- SQL 语法主题颜色应用到编辑器。
- Dashboard 图表配置在添加 SQL 时正确携带。
- Databases 标签截断防止与工具栏按钮重叠。
- 查询工具栏滚动条隐藏。
- 结果集 Tab 列表水平滚动（隐藏滚动条）。
- Schema Diff：MySQL/PostgreSQL/SQLite 迁移能力对齐、拒绝不安全的空值渲染。
- Driver API：SQL Server 对象 DDL 标识符引用转义、`parse_type_parts` 保留数组括号。
- 连接池泄漏修复：错误路径关闭连接池。

### 🧪 测试（Testing）

- Journey Test 体系建立：连续状态机测试覆盖关键交互路径。
- E2E 并行化：分组并行脚本 + 每 worker 独立数据库隔离。
- Onboarding 全流程 E2E（正常 + 异常 + 边界场景）。
- Schema Diff 迁移渲染器能力测试（PostgreSQL、MySQL、SQLite）。
- EP 签名验证集成测试 + Pro 打包管道验证。
- Settings Snippets 生命周期 Journey Test。
- UI 语义 Token 迁移回归测试。

### 📦 架构重构（Refactoring）

- **Plugin/Extension → Wapp/Driver 命名迁移**：移除旧版兼容层，统一术语。
- **Schema Diff 重构**：移除旧版方言模块，通过 Driver API `SchemaMigrationRenderer` trait 委托渲染。
- **前端模块拆分**：大型 Store 和组件按职责拆分为高内聚、低耦合子模块。
- **AI Prompt 模板**：从 `.txt` 迁移到 `.markdown`，统一 PromptResolver 优先级。
- **i18n 领域包拆分**：从单一大文件拆分为 14 个领域包 + 惰性加载。

---

## [0.1.2] - 2026-08-XX

_（v0.1.x 版本变更记录请参阅 Git 历史）_

---

## [0.1.0] - 2026-07-XX

_（初始发布版本）_
