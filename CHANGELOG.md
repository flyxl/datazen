# 更新日志 / Changelog

本文件记录 DataZen 的显著变更，重点是影响外部契约的破坏性变更。

格式参照 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 简化版；
术语约定：**`connectionId` / `connection_id` = 配置连接 id（持久化）**，
**`dbSessionId` / `db_session_id` = 运行时数据库会话 id（内存态）**。

---

## [Unreleased]

---

## [0.3.0] - 2026-11-14

> 自 v0.2.3 以来累计 **2326** 次提交。主线是**数据浏览与编辑全链路重做**（键盘导航 / 剪贴板 / 类型化单元格编辑 / 外键跳转 / 行数三态 / 筛选与分页重做）、**导入导出扩充**（导入向导、XLSX / Parquet / TSV / Markdown / SQL UPDATE 导出、无会话导出）、**值呈现规则**（时间显示设置与列级覆盖）、**AI 助手能力对标**（Agent 模式 / AI 记忆 / MCP 工具治理 / datazen CLI），以及**团队 Web 服务**（`/api/v1` 十五域 108 路由 + 浏览器壳）首版落地。

### ⚠️ 破坏性变更（Breaking Changes）

**`DatabaseDriver` 移除 `split_restore_sql`**（`datazen-driver-api` 0.0.19 → 0.0.20）。备份文件的 SQL 切分规则改由领域契约 `BackupDialect` 承载，唯一实现是 `sql_text::split_restore_sql`。**只实现过该方法的外部驱动需要删掉这段实现**；没有实现过该方法的驱动零编译影响（仓库内 15 个 path 驱动均未显式声明它）。

**`olap` 驱动删除。** `drivers-registry.json` 不再声明 `olap`，`driver-olap` Cargo feature、Presto/Trino 连接类型映射、`sync_taxonomy` 中的 `presto → trino` 归一化以及前端 Presto/Trino 方言配置一并移除。已保存的 `olap` 连接不会再解析到驱动；需要访问 Presto/Trino 的用户应改用通用的 HTTP/SQL 驱动或重新接入独立驱动。

`PROTOCOL_VERSION` 冻结为 **6**（2026-10-21 起；兼容门禁族一并退役），详见 [驱动层 §2.5](docs/architecture/backend/drivers.md#25-协议版本闸门)。

### 🚀 新功能（Added）

#### 数据浏览与编辑全链路重做（DB-01…DB-07 / DBR-01…06）

- **键盘导航**（DB-02）：结果网格接上完整键盘导航（方向键 / Enter / Escape / ⌘A），行容器退出 Tab 序列；8 个快捷键补全 i18n 提示。
- **剪贴板复制与粘贴**（DB-03）：跨类型列复制粘贴、剪贴板「来源」与内容绑定（陈旧来源不再静默写错列）、粘贴拒绝给出点名文案。
- **类型化单元格编辑**（DB-05）：列类型归一化为 11 种 `CellType`，每个族一个专用控件，提交时机只有一处；**写入意图三态**——`NULL`、值、「不写这一列」不再互相折叠；JSON 列非法输入阻止提交并给出行内原因；驱动的类型声明同时约束单元格编辑器、着色与筛选算子（筛选器不再把 SQL Server `timestamp` 当时间列）。
- **脏标记与新增行**（DB-06）：未保存单元格高亮、按脏索引一处计算、「撤销删除标记」；新增行走草稿通道，跨页身份冲突显式拒绝。
- **外键跳转**（DB-07）：单元格 FK 箭头 → 目标表透传跳转，面包屑、返回栈、首帧预置与快捷键；隐藏外键成员不再让整组失去箭头。
- **行数三态**：`≈` 估算（PostgreSQL 走 `pg_class` 统计）、明确的「估不出来」提示与精确统计入口；估算不得钳制用户页码，判定活过每一次重读。
- **筛选重做**：逐条 `AND` / `OR` 连接符取代全局 mixed-logic；算子按列类型分派（几何/数组/二进制/枚举各有正确算子集）；不完整条件显式阻断而不是发出错误 SQL；LIKE 大小写语义按方言纪律。
- **分页重做**：keyset 分页按驱动能力启用；四方言各自的 seek 语法；「无法 seek」如实告知而不是换一种写法。
- **导入向导**（DBR-01）：RFC-4180 解析器、列映射向导、分批事务执行与可解释的错误报告。
- **行复制与粘贴新增**（DBR-02）：行负载通道、`⌘D` 复制当前行。
- **全库值搜索增强**（DBR-03）：Not Contains、Schema 感知的搜索范围与定位、类型范围按驱动声明开放。
- **Any Column / Raw SQL 筛选**（DBR-04）：模式行 + 后端校验的原始谓词，预览执行同链。
- **Set Value**（DBR-05）：「设为当前时间」类型化写表达式、Quick Look 快捷入口、二进制从文件加载。
- **表头排序对齐 Navicat**：单击三态循环（升 → 降 → 无）、列级排序菜单、底栏排序状态净化。
- **查询结果单元格编辑增强**：含主键列的直接投影 `SELECT` 可进入窄写回映射并编辑；查询结果编辑改为暂存（脏标记）+ 预览 + 显式提交/回滚，支持一次提交多处修改；提交后保留运行时绑定，可在同一 live 结果继续编辑（Navicat 式），并补充提交成功提示与失败重试引导。

#### 导入导出格式扩充

- **XLSX 导出**（`rust_xlsxwriter`，calamine 读回验收）与 **Parquet 导出**（按方言与列值做 schema 推断 + 读回验收）。
- **TSV / Markdown / SQL UPDATE** 加入 Rust 批量导出；导出源支持查询结果与已加载结果集。
- **无会话导出**：只依赖已加载数据的导出不再要求打开会话；单表 / 对话框路径统一走 `DataFetcher`。
- **导出体验一致化**（DBR-06）：列选择、格式门槛（不可用格式禁用并说明原因）、流式进度与**中途取消**（含 SQL 导出路径）、`.part` 中转原子保存——失败保留原文件、abort 如实报告。
- **CSV 公式注入防护**统一到 Rust 流式写出与文本格式化路径（此前只防前端下载）。

#### 值呈现规则（issue-32）

- **时间显示设置**：全局时间戳显示规则（格式、时区、毫秒精度）。
- **可编辑的呈现规则**与**列级覆盖**（来源隔离持久化、继承清除预览规则值）。
- 跨窗口设置同步（路径作用域保存）、未知版本字段保存时保留不丢弃。
- 网格、详情抽屉与草稿行走同一份呈现计划。

#### AI 助手能力对标

- **Ask / Agent 双聊天模式**：Agent 模式带只读查询与采样工具（未绑定连接时 fail-closed）。
- **AI 记忆**：检索注入对话与 NL2SQL 入口、连接笔记优先、记忆管理 UI、修正模式与陈旧过滤。
- **会话与上下文**：AI 会话绑定稳定连接 id 并持久化到 AppDb；自动聊天上下文快照；编辑器选区注入对话；对话中的 SQL 方言代码块可直接执行并给出出网摘要。
- **MCP 治理**：批量与 Redis 读工具、Streamable HTTP 传输（opt-in）、工具调用作用域与审计、插件与工作流工具经确认接入。
- **Provider 与 CLI**：AI Provider 预设、兼容性探测（models 端点不可用时自动探测）、本地 CLI agent provider、`datazen` CLI 与 agent skill、技能发现与预算上限。

#### 团队 Web 服务（W8 首版）

`crates/datazen-server/` axum HTTP 宿主 `/api/v1` 十五域全亮——settings、identity、connection、session、execution、job、artifact、events（SSE）、tableData、driverGateway（过渡）、queryAssets、schemaDiff、workflow、dashboard、dataSync，合计 **112 个域客户端方法 / 108 条路由**，契约见 [HTTP API 契约](docs/architecture/platform/web-http-api.md)（v1.10.1）。要点：

- `packages/backend-client` 域客户端是 HTTP 面的唯一契约来源；桌面 Tauri command 与 server HTTP handler 是同一用例的两个薄绑定，桌面 IPC wire 逐字节不变。
- `packages/web/` 浏览器壳落地（HTTP transport、SSE 事件流、108 键路由分发）；web 不支持名单收敛至 134 个 legacy 桌面命令（文件系统/对话框等桌面专属语义留表）。
- 服务端管理库首版为 **SQLite**（expand-only 迁移链 V12–V15：jobs、workflows、dashboards、sync）；PostgreSQL 适配器为后续生产化切片。凭据行内只存 `SecretRef`，不复用宿主 AES 落盘。
- Workflow / Dashboard / Data Sync / Schema Diff / 查询资产五域的服务端半边：双端定时调度器（授权单确定性派生）、双端刷新监控引擎、sync 执行面（compare 冻结投影 + 受理幂等键）、恢复核验只读裁决。
- Wapp 资产 origin 静态面 `GET /wapps/{id}/{path}`：web 形态沙箱 iframe 的 server 半边。
- `deploy/dev/` 一键开发栈：dex OIDC + 引导链 + seed + `up.sh` / `down.sh`。
- 未随本版：桌面团队认证（P8）、多 worker 协调（P9）、server 形态 AI 供应商注册表（交互面按执行器默认错误策略 fail-closed）。

#### 应用与更新

- **设置页安装外部语言包**：共享 `language_packs` 域 + 桌面 IPC + 前端注册链；内置语言保持同步切换，包语言异步注册。
- **定时更新提醒**：空闲唤醒检查，不再打扰专注时段。
- **驱动领域契约 additive 升级**（`datazen-driver-api` 0.0.13 → 0.0.20）：`SqlDialect` / `CatalogReader` / `CommandCatalog` / `SchemaMigration` / `BackupDialect` 五契约各配借用适配器单向委托旧 trait——**外部驱动无需重新编译即可继续工作**。

### 🔧 改进（Changed）

**git 驱动 `kiwi` / `superset` 对齐 `datazen-driver-api` 0.0.20。** 两者此前只实现 `DatabaseDriver`，宿主通过 `require_resource_provider` 拿到的是 `ResourceProviderMissing`。现在两者都提供真实的 `ResourceProvider`：

- 工厂与 provider memoize 同一驱动实例，避免已签发 handle 被 epoch 检查拒绝；
- 补齐 `CapabilitySet` 的 12 个能力格与逐格证据：`namespace_switch = PerRequest`、`data = BufferedReadWrite`，事务/快照/会话句柄/重置均明确拒绝，Superset 备份明确 `Unsupported`；
- 命名空间形状声明为单个**可选** `Database` 层，`Schema`/`Catalog` 目标按名拒绝，不静默丢弃；
- 补齐 `TableSchema` 新增的 `check_constraints` / `table_options` 字段。

**行为变化：类型归一化改了若干列的渲染族与筛选算子集。** 旧实现是一串子串判断（`type.includes('int')` …），因此把几何列当成数字、把数组当成文本。归一化后，下列类型名的**渲染族**（即单元格的着色族）与**筛选算子集**发生了变化——清单由 `src/lib/__tests__/filterOperators.cellTypeMigration.test.ts` 逐条钉住：

| 类型名 | 渲染族（旧 → 新） | 筛选算子集的变化 |
| --- | --- | --- |
| `point`（PG 几何）、`interval` | number → text | 去掉 `between`/`notBetween`，多出 LIKE 家族、`isEmpty`/`isNotEmpty` 与 `regex` |
| `abstime`、`reltime`（PG 内部类型） | datetime → text | 去掉 `between`/`notBetween`，多出 LIKE 家族、`in`/`notIn`、`isEmpty`/`isNotEmpty` 与 `regex` |
| `ARRAY`、`ARRAY[]`、`text[]` | text → json | 去掉大小比较、LIKE 家族、`in`/`notIn`、`isEmpty`/`isNotEmpty` 与 `regex`，多出 `jsonContains` |
| `integer[]`、`Array(Int32)` | number → json | 去掉大小比较、`between`/`notBetween` 与 `in`/`notIn`，多出 `jsonContains` |
| `image`（SQL Server）、`RAW`（Oracle）、`FixedString(16)`（ClickHouse） | text → binary | 去掉大小比较、LIKE 家族、`in`/`notIn`、`isEmpty`/`isNotEmpty` 与 `regex` |
| `dec`、`dec(10,2)`、`NUMBER(38,0)` | text → number | 去掉 LIKE 家族、`isEmpty`/`isNotEmpty` 与 `regex`，多出 `between`/`notBetween` |
| `enum('a','b')`、`set('a','b')`、`Enum8('a' = 1)`、`Enum` | text → text（族不变，控件变成下拉） | 去掉大小比较、LIKE 家族与 `isEmpty`/`isNotEmpty`、`regex` —— PostgreSQL 上这些入口必然报错 |
| `uuid`、`uniqueidentifier`、`UUID` | text → text（族不变，控件加 UUID 校验） | 同上 |

其余类型名（含 `UInt8`…`UInt256` 这类 ClickHouse 无符号整数、`Int128`/`Int256`）的族与算子集**不变**；无符号整数此前靠 `'uint64'.includes('int')` 碰巧落在 number 族，新规则表为此显式写了 `uint` 一条。SQL Server 的 `timestamp`/`rowversion` 按行版本（字节）处理且不可就地编辑，MySQL 的 `tinyint(1)` 按布尔处理——这两条由驱动自己的元数据声明，宿主不写驱动分支。

**Windows portable 包含完整运行时资源**：portable zip 现与安装器一致，附带 `prompts/` 与 Pro 扩展 `builtin-ep/`，不再依赖构建机缓存遗留目录。

**运行时与驱动失败映射为可行动的错误消息**：失败码收敛到安全、可操作的文案，不再裸露内部状态。

### 🐛 修复（Fixed）

- **事务状态**：未知/不支持的事务状态不再误弹回滚确认，也不误报「事务中」；流式查询会话的面板命名空间目标正确送达。
- **Redis Pub/Sub**：载荷按持久化 `connectionId` 归属，修复前端丢弃全部消息。
- **SQL Server**：括号包裹的外键引用解析；租约会话如实应答；T-SQL 限制边界。
- **MySQL**：空列清单 INSERT 使用 `() VALUES ()`，其余方言保持 `DEFAULT VALUES`。
- **导出**：批次最后一张表加载途中取消不再继续整批；`structure_only` 导出文件名用 `.sql`；ExportDialog 默认范围在整表不可服务时回落正确；XLSX 整数写单元格不再因 `i64::MIN` panic。
- **SQL 防护**：EXPLAIN ANALYZE VERBOSE/ANALYSE 选项次序 fail-open 封堵；深度类型与连接/SQL guard 边界加固。
- **连接与运行时**：会话复用钉住连接，修掉目录读取 30s 池超时；未配置 database 的 profile 允许打开会话；空闲会话在派发授权期间保活。
- **数据网格**：外键箭头整组失效；全格被跳过时不再发布一次状态；跨页身份冲突覆盖缺口；粘贴拒绝的用户可见错误四处收口；可拖拽表头下列头排序按钮点击无响应（macOS/Tauri）。
- **查询结果编辑**：多条暂存修改预览仅显示首条、批量提交后续项因绑定提前失效而报只读的问题修复。

### 🧪 工程与质量

- TypeScript 7 升级并以双 TS7 编译器门禁。
- DDD 内核重构收口：唯一业务内核 crate（`datazen`），桌面/服务端宿主薄壳化，命令层 `_impl` 115 → 21。
- E2E 全量 145 specs 绿并支持并行整跑；CI docs-only PR 豁免重型 job、runner 磁盘加固。
- 全文档系统与代码一致性对齐（DDD 迁移后 64 处修复）。

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
