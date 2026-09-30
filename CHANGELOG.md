# 更新日志 / Changelog

本文件记录 DataZen 的显著变更，重点是影响外部契约的破坏性变更。

格式参照 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 简化版；
术语约定：**`connectionId` / `connection_id` = 配置连接 id（持久化）**，
**`dbSessionId` / `db_session_id` = 运行时数据库会话 id（内存态）**。

---

## [Unreleased]

_（暂无）_

---

## [0.2.3] - 2026-09-30

> 自 v0.2.2 以来累计 **769** 次提交。主线是**数据迁移三件套的 Navicat 对标交付**（Schema Diff / Data Sync / Data Transfer 全链路重做）、**SQL Server 驱动活体验证**、`@datazen/ui` 设计系统收敛，以及更新通道按 SKU 分轨。

### ⚠️ 破坏性变更（Breaking Changes）

**无。** `PROTOCOL_VERSION` 保持 4，本周期新增的 driver-api trait 方法（`pagination_syntax` / `begin_read_snapshot` / `execute_with_params` / `parameter_placeholder` / `has_complete_foreign_key_catalog_visibility` / `render_transfer_sql_file_insert` 等）**全部带默认实现**，外部驱动无需重新编译。两处存储行为升级对自动迁移、用户无需手工操作：

- **加密主密钥后端升级**：macOS 未签名/ad-hoc 构建与 Windows 改用**平台保险库**（macOS `security` CLI → 登录钥匙串；Windows DPAPI 用户级加密），取代明文 `.key` 文件。升级后首次启动自动透明迁移旧 `.key` 并删除原文件；`DATAZEN_KEYRING=file` 逃生口保留（CI / 开发）。Linux 行为不变。
- **SQL 收藏改为文件存储**：收藏不再是 SQLite 行，而是收藏目录下按 **ULID 命名**的 `.sql` 文件（元数据写成 `--` 注释行）。列表顺序即创建顺序，iCloud / Dropbox / Git 可像普通文本一样同步与合并。

### 🚀 新功能（Added）

#### 数据迁移三件套（Schema Diff / Data Sync / Data Transfer）

- **Schema Diff 统一对象规划器 + 依赖 DAG**：视图 / 例程 / 触发器 / 外键的结构依赖以**结构化依赖目录**建模（driver-api 新增 `SchemaObjectDependencies` / `TypeDependencyUsage` / `SequenceDependencyUsage` 协议类型），迁移操作按**经过校验的 DAG 排序**部署——此前固定分组排序会破坏主键 / 索引替换顺序；审阅对象选择可持久化、可保存为可复用 Profile。
- **可安全迁移的对象类型大幅扩充**：视图（跨库映射、混合快照拒绝）、存储过程 / 函数与触发器（声明头身份校验）、CHECK 约束、表选项、用户自定义类型、PostgreSQL 序列（身份连续性 + 回滚补齐）、目标侧删表的安全操作（外键目录守卫）、SQLite 安全表重建（catalog 与文件路径分离、陈旧部署拒绝）。MySQL 目录侧修复：视图列限定符归一化、CHECK OPTION 解析、外键依赖可见性、别名引号。
- **Data Sync 比较执行重做**：不可变比较执行计划（指纹锁定审阅内容）、大体量比较**溢写磁盘索引存储**（进程退出后可恢复，含帧前缀校验）、比较页**流式执行**、结构化来源过滤、稳定读快照比较（driver 侧 `begin_read_snapshot`）、显式冲突策略与乐观写冲突检测、复合元组范围 / 稳定行记录集、跨页全选与翻页默认保留、归一化键契约（SQLite 文本游标按 BLOB 绑定）、取消生命周期隔离、未知结果安全对账。
- **Data Transfer 传输引擎补强**：不可变计划执行契约、稳定记录集选取与边界校验、**有界可续传检查点**（空页续传、回滚围栏、ack 丢失防护）、外键依赖写入排序、PostgreSQL 恒等序列保留与重置、有损列映射 / 未证实排序规则拒绝、UTF-8 文本值保真、Redis 配对前置拒绝；**SQL 文件目标**新增（原子写入、编码选择、gzip 输出、按所选方言渲染、表映射 workflow、结构依赖保留、不可表示作用域拒绝）。
- **迁移档案与调度**：三件套共享**运行历史**与可复用 Profile（AES-GCM 加密持久化），Workflows 调度器可按迁移档案定时执行。
- **实验性配对标注**：仅**两端均为 PostgreSQL 或 MySQL** 的配对（含两者互迁）经过真实端到端迁移验证；源 / 目标任一端为其他库时，在 Data Sync / Data Transfer 界面标注「实验性」。

#### SQL Server 驱动（活体验证）

- 在真实 Azure SQL Database 上把 `datazen-driver-sqlserver` 跑通驱动层 / 宿主 IPC 层 / GUI 层，修复 14 项缺陷：`Incorrect syntax near 'LIMIT'` 根因（分页子句）、批处理语义（`CREATE PROCEDURE` 等走真实 batch）、事务泄漏（`BEGIN TRAN` error 266）、`explain()` 曾**实际执行**被分析的 DML（数据损坏级）改用 `SET SHOWPLAN_TEXT`。sqlserver 仅进入 **All SKU**，Basic 不含。

#### 编辑器与 AI 生产力

- **多光标**：`⌘/Ctrl + 点击` 添加光标、`Escape` 退出；键位以 `Prec` 提权，与复制行另辟快捷键共存。
- **全局查询历史重做**：状态 / 时间范围 / 数据库 / Schema 四维筛选，最新 / 最早 / 最慢三种排序，批量导出、复制 SQL、在查询面板打开、一键收藏；截断时明示「{shown} / {total}」而非假装完整。
- **面板（pane）维度状态模型**：焦点按 tab 归属路由，修复跨 tab 焦点泄漏与表数据状态串连接。
- Pro 侧：Code Folding EP 槽位、三个通用 EP 钩子打通设置链路，`@codemirror/language` / `@codemirror/commands` 进入宿主-Pro 共享模块集。

### 🔧 改进（Changed）

- **`@datazen/ui` 设计系统收敛**：109 处手写转圈统一为 `Spinner`；3 个手写 tab 条收敛为 `Tabs`；连接树与 Redis 键浏览器落在共享 `VirtualTree` 壳（纯层级逻辑抽出为公共契约）；14 处内联错误条统一为 `ErrorBanner`；数据迁移三件套对话框改用公共组件。`@datazen/ui` 由 module-layers + import-boundaries **双规则守卫**强制零宿主依赖（`PathInput` 改注入式选择器，移除对 Tauri 的依赖）。
- **分页改由驱动自主声明**：driver-api 新增 `PaginationSyntax` + `pagination_syntax()` 默认实现，宿主 4 处调用点（query_executor / data_transfer / data_sync keyset / sync keyset_source）不再硬编码 `LIMIT ? OFFSET ?`；SQL Server 走 `OFFSET … FETCH NEXT …` + `ORDER BY (SELECT NULL)` 兜底；编辑器行数上限不再对自带 `OFFSET` 的 T-SQL 注入 `TOP`（error 10741）。
- **更新通道按 SKU 分轨**：Basic / All / Akulaku 各自发布并读取**自己的** `latest.json` / `latest-all.json` / `latest-akulaku.json`；安装前复核清单 `variant` 与构建 SKU，不符即拒装——All / Akulaku 用户不再被 Basic 构建静默替换（丢驱动）。SKU 矩阵 / tauri.conf / release.yml / 打包模板由 `pnpm test:release-variants` 四方一致性守卫看住。
- Release CI：撤销 driver union 预热（实测墙钟 +62%，21 分钟负优化，教训已写入 ci-test-matrix.md §6.1）；`union-typecheck` 单点类型闸门保留；codegen 内容未变时不重写文件；Linux AppImage 通过 appimage.github.io 校验。
- 依赖治理：CodeMirror 与 React 在宿主 / Pro 两侧全部钉死精确版本并纳入 seam 守卫；`scripts/` 与 `scripts/__tests__` 进入真实 tsc 闸门（修复 38 条类型错误）；pack-ep 加产物级签名闸门。
- 文档：README 全文重写并按当前设计重拍 53 帧截图与演示视频；新增验证方法论与 worktree 隔离边界两篇开发文档。

### 🐛 修复（Fixed）

- **Updater / 发布**：变体被 Basic 清单替换（见上）；变体清单缺平台从静默改为失败；`PRO_DEPLOY_KEY` 缺失时 release 硬报错而非半程构建。
- **安全**：rustls `CryptoProvider` 多提供者环境下安装改为确定性并在全新进程中验证；EP 打包「失败仍留下可复用已签名树」与 unmapped 包签名旁路封堵；`grep -F` 字面匹配修正 4 处路径注入误判。
- **崩溃 / 窗口**：早退分支后调用 hook 导致 React error #310；Onboarding 向导 close 后未真正销毁；执行门卸载后参数聚焦定时器未清理。
- **SQLite**：文件路径被当作 catalog 名插值到元数据查询（结构视图空列根因之一）；`ON CONFLICT` 策略 fail closed；CRLF / 多行视图体解析；Schema Diff 部署冻结 catalog 作用域。
- **连接与表格**：关闭连接时一并关闭该库全部 tab；再次单击已选中行可取消选中；多选右键菜单作用于整个选区；连接树叶子命名空间节点不再谎报 `aria-expanded`；navigator 对象身份键保留（重载函数不再互相覆盖）。
- **i18n**：dev 期暴露 `t()` 静默回退原始 key；宿主 document-view 字符串迁出驱动 `mongo.*` 命名空间；lazy 域 JSX 常量移入 `localesReady` 守卫之后（首帧竞态）；key 碰撞守卫补两个缺口。
- **E2E / 构建脚本**：macOS `[target.'cfg(macros)'.dev-dependencies]` 表被驱动注入标记吞进占位段导致 Linux CI 构建破坏；`tsc -b` 用编译产物覆写真实 tsconfig；驱动注入期间 `Cargo.lock` 被弄脏（纳入 stash 保护）；worktree 创建脚本铺好环境、缺件大声报错、失败回滚。
- 复制失败不再谎报「已复制」；复制反馈定时器可取消、可归属到具体调用与窗口。

### 🧪 测试（Testing）

- 迁移三件套交付周期含 tester 独立复测轮：WDIO 用例按宿主 / Pro 边界分流，元素定位统一改用 `data-testid`，拆批量消除此前**恒真、永远不会失败**的断言，并修复被它们掩盖的真实缺陷。
- E2E macOS 静默模式：测试窗口永不成为 key window，跑 E2E 不再抢开发者焦点。
- `scripts/` 门禁单测 27 files / 339 tests（基线 334），含守卫变异自证；契约种子建表失败不再被吞掉。
- 发布门禁全量绿：`pnpm typecheck`、Host 单测 **549 files / 5648 tests**、driver-api + basic 四驱动 + datazen lib + ai-api（`scripts/ci-local.sh` 11 步）、sqlserver 驱动 lib 59 tests、i18n-sync 312 missing → 0。

---

## [0.2.2] - 2026-09-27

> 自 v0.2.1 以来累计 **800+** 次提交。主线是可视化查询构建器、Redis 工作台重做、数据库隧道，以及一次驱动契约的硬切换。

### ⚠️ 破坏性变更（Breaking Changes）

- **Driver API `PROTOCOL_VERSION` 3 → 4（硬切换）**：元数据契约里的 `(database, schema)` 维度不再靠会话隐式携带。
  - `get_tables` / `get_table_schema` / `get_columns` / `get_all_columns` / `dump_*_ddl` 现在显式接收 `database` + `schema`。
  - **删除 `use_database`**：宿主不再在每次读取前发 `USE` 语句。
  - 新增 `has_schema_level()` / `default_schema()` 描述驱动是否有第二层命名空间；`validate_schema_target(driver, database, schema, scope)` 成为双方共用的唯一校验规则。
  - `SchemaScope::{AnySchema, ExactSchema}`：列举类方法可接受「全部 schema」，解析类方法必须指定一个。
  - **所有外部驱动必须重新编译。** Kiwi、Superset 已于 2026-09-21 在各自 `main` 跟进该契约；`olap` 仍钉在 `ref 7096c873`（v3 时代）。
- **宿主移除 `ensure_session_database` / `set_active_database`** 及 12 处调用点；`SchemaCache` 键纳入 schema，空列集不再写入也不再返回。

### 🚀 新功能（Added）

#### 可视化查询构建器

- **三区布局** 全新 Query Builder：Navicat 式子句列表 + 字段选项弹窗 + HAVING，画布原生滚动，`Ctrl`/`Cmd` + 滚轮缩放。
- **拖拽建 JOIN**：点列到列手动连线，连线锚定到具体列；复合外键按「一对表只连一条」归一化。
- **外键自动推断**：仅从元数据推断外键，**默认关闭**，编辑器与构建器共用同一份推断结果；ER 图也接入同一开关。
- **方言适配**：`DatabaseTypeMeta.qbTypeCategories` 描述每种类型的可用操作符（如 `LIKE` 仅对文本/二进制列开放），宿主侧有兜底。

#### 数据库隧道

- **HTTP/HTTPS CONNECT 与 WebSocket 隧道**，本地 loopback 端口转发，覆盖三大云数据库代理场景。
- **SavedTunnel 持久化**：隧道引用在导出时物化，凭据走 AES-256-GCM 加密存储；连接表单可配置隧道来源。
- **设置窗口统一管理已保存隧道**，隧道来源三态状态机闭环。
- 中继链路集成测试 + 上游探测（代理/WS），转发器随句柄 drop 中止。

#### Redis 工作台重做

- **Key 写入语义**：`SET ... KEEPTTL`、绝对过期 `EXPIREAT`，`set_string` 增加 `keepTtl` / `expireAt` 参数。
- **压缩值查看**：gzip / zlib / raw-deflate / base64 / none 编解码器，字符串值可解压查看，JSON 美化。
- **Key 浏览**：TYPE 过滤、树形/扁平视图切换、`MEMORY USAGE` 开关、命名空间树、SCAN 游标与层级保持刷新。
- **面板重组**：Console 结构化结果 + fail-closed 危险分类、Slowlog 提升为一级 Tab、Pub/Sub 增强（订阅列表/统计/搜索）、独立连接的实时 MONITOR 面板。
- **观测组件**：`memory_usage_key`、INFO 搜索过滤面板，Cluster / Sentinel 模式感知的服务端行与拓扑指标。
- **集群寻址修正**：探测命令按槽位显式寻址，大 key 字段读改为每键一次寻址批次。

#### AI

- **会话隔离** + **NL2SQL 流式预览** + 展示修正。
- **可取消**：新增 `AiError::Cancelled`、`StreamChunk.cancelled`、`CompletionRequest.cancel_token`、`CancellationRegistry`，覆盖 SSE 中断与 Tool Loop 守卫。
- **egress 汇总**：`egress_summary` StreamChunk + 协议升级，暴露用量/测试面。
- 宿主持有的 Redis 键值事实接入 AI 助手。

### 🔧 改进（Changed）

- **全量语言包补齐**：9 个宿主语言包（de / es / fr / ja / ko / pt-BR / ru / zh-CN / zh-TW）与 9 个 Redis 驱动语言包全部对齐 `en.ts`——宿主侧补 172 个缺失 key、删除 20 个已废弃 key，Redis 驱动侧补 247 个缺失 key。占位符一致性逐 key 校验（0 失配）。隧道、可视化查询构建器、对象树等本版新术语已在全部语言中统一落地，并顺带修正了一批既有误译（如 de `newConn.host` 误作「Gastgeber」、es `settings.logging` 误作「Explotación florestal」、pt-BR `workflows.form.condition` 误作「Doença」）。
- **驱动 ↔ 宿主解耦**：新增边界护栏（宿主 src、驱动内部、`setLocale`）并接入 CI；驱动不再引用宿主 `src/lib/cn`，统一从 `@datazen/ui` 导入。
- **驱动 locale 包自注册**进共享 i18n 登记表，并在 `i18n-sync-check` 中纳入校验。
- **LIMIT/OFFSET 改为驱动自主声明**（`supports_offset()` 语义收敛），宿主不再硬编码。
- Redis 驱动按职责拆分为 `driver/` / `value/` / `commands/` / `ops/` / `tree/` / `stream/` / `workbench/` 模块树，移除 `include!` 反模式。
- AI Prompt 解析异步化，补充 PG / MySQL / SQLite 方言提示。

### 🐛 修复（Fixed）

- **多库 workflow 未指定 database（重要）**：用户在多库连接上建 workflow 时，界面标注「必填」的 database 实际并未强制——`validateWorkflowFields` 只校验 id/name/steps，可视化模式有拦截但 **YAML 模式与 AI 生成面板没有**，workflow 因此得以保存。运行时三处（step / workflow / connection）都没配时，驱动静默回落到内置默认（PostgreSQL 在 `resolve_connect_database` 硬编码 `"postgres"`），用户最终只看到误导性的 `relation "..." does not exist`。
  - 三条编辑路径（可视化 / YAML / AI）统一走同一校验，缺库时直接指出 `steps[N].database`。
  - command step 此前拿不到 workflow 级 database（`WorkflowStep::Command` 在 Rust 侧无 `database` 字段，目标库在 `input.database`），现已补齐继承，step 自身显式值仍优先。
  - 运行时改为报明确的 `MissingDatabase`，指明 step、connection 与三个可设置位置。
  - 该校验**只对多库驱动生效**：新增 trait 方法 `has_multi_database()`（默认 `false`，与既有 `has_schema_level()` 同构，不破坏任何外部驱动），在声明 `hasMultiDatabase: true` 的 postgres / mysql / sqlserver / clickhouse / mongodb 中覆写为 `true`。SQLite 等单库驱动行为不变。
  - 附带修复：表单为 command step 提供的 database 下拉框此前在 `workflowDraftToDefinition` 中被直接丢弃，YAML 侧又写成 serde 忽略的平铺键——选了不起作用。
- **PostgreSQL 流式查询读错库**：`query_stream` 在准备阶段即快照会话默认连接池，即使调用方指定了 target database，语句仍从默认库取数且不报错。现按目标库重新解析并装回执行记录；事务仍绑定其原库，传入外部 target 直接拒绝。
- **表右键菜单缺少「打开结构」**：navigator 未设置 `showOpenStructure`，该入口一直缺失。改由宿主通过 `ConnectionViewActions.openTableStructure` 显式暴露（可选成员，插件无需同步改动）。
- **BUG-003 SchemaCache 污染（重要）**：先打开 ER 图会读取非当前会话的数据库并切换共享会话，随后把**零列结果**以 300s TTL 写进 `SchemaCache`。此后所有读取都命中被污染的条目——网格行数正确但单元格为空、结构视图无列、ER 图表格无列。随 v4 契约一并根治。
- **MySQL SQL 字面量反斜杠转义缺失**（注入风险），并补充双引号转义以兼容 Navicat。
- 切换到从未打开过的连接时，不再沿用上一个连接的面板状态（表数据状态改为按面板而非按连接隔离）。
- Redis 字面量检索由精确键匹配改为按前缀搜索。
- 写入语句后丢弃已缓存的表数据；表切换时确保列元数据已加载。
- i18n 补齐 25 条「代码在用、词典里没有」的 key，并加入守卫防回归。

### 🧪 测试（Testing）

- 新增 / 修复 133 个测试提交：Redis 键树契约对齐、Visual Query Builder journey（连续击键状态机 + 残缺中间态）、QB 回归矩阵。
- 修复 9 个 spec 的陈旧会话等待与一次性树断言，以及 7 个失败 spec 的根因（改为修测试而非改断言掩盖）。
- 消除了全部 57 条未使用导入告警。

---

## [0.2.1] - 2026-09-17

> 自 v0.2.0 以来累计 **627** 次提交。本节为按 Git 历史回溯补记。

### 🚀 新功能（Added）

- **SQL 编辑器 Pro 扩展**：EP 热插拔运行时 + CodeMirror Compartment 动态重组；签名验证门禁与 `.dzx` 打包工具，Release 自动上传扩展。
- **Schema 迁移能力契约**：`SchemaMigrationRenderer` / `SchemaMigrationCapabilities` 驱动侧方言渲染，可取消的 DDL 部署任务（共享 Job Registry），建表迁移操作。
- **同族类型归一化**：驱动级 `TypeNormalizer` 供同族类型比较。
- **Workflow / Dashboard**：Dashboard 首次执行辅助、Widget 创建后自动执行。
- **对象树**：`ObjectKind::Table` / `View` 支持 `object_ddl_sql`。
- **连接表单** 新增独立 `domain` 字段。
- 官网 SEO：双语博客系统（12 篇）。

### 🔧 改进（Changed）

- 扩展点中的 statement range 合并，默认执行策略收敛。
- 补全表前缀偏好与统一提示 tooltip。
- 暗色主题全面翻新 + UI 打磨。

### 🐛 修复（Fixed）

- 修复 `CONFIG_ID` 术语遗留、深层评审 Wave 1 / Wave 2 全部 12 条轨道的遗留缺陷。
- Pro 扩展配置与翻译在发布构建中的固定（`fix(release)`）。

> 更细的逐条变更请查阅 v0.2.1 的 Git 历史；各版本对外发布说明不入库，仅存于本地 `posts/`。

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
