# 数据迁移三件套支持 SQL Server：设计与实施方案

日期：2026-09-29

状态：首轮安全支持范围已实施；原始设计目标与当前能力边界见第 9 节。

工作分支：`codex/sqlserver-migration-plan`。
最近更新：2026-09-30。

## 1. 目标与范围

三项功能分别交付：

- **Schema Diff**：SQL Server → SQL Server 的结构比较、计划预览、执行和执行后复核。
- **Data Sync**：SQL Server → SQL Server 的数据同步，保持同族、结构一致、主键一致的门槛。
- **Data Transfer**：SQL Server 作为源和目标，支持同族传输，以及与现有 SQL 驱动间的异构传输。

能力必须区分可读取、可比较、可生成计划、可执行和可恢复，不能用单一“支持”开关掩盖差异。支持范围内应端到端可执行、结果可复核；范围外必须在写入前明确拒绝，不能静默损失数据或结构。

## 2. 实施前代码基线

以下记录是 2026-09-29 编写方案时核对的基线，不代表当前代码状态。

| 模块 | 当前代码事实 | 主要缺口 |
| --- | --- | --- |
| Schema Diff | Host 已有中立 Migration IR、能力接口和执行流程 | SQL Server 未接入 `migration_renderer`、`migration_capabilities`、`type_normalizer` |
| Data Sync | 已有同族分类、比较和 ChangeSet 流程 | Host 部分路径仍采用 MySQL / PostgreSQL 二分方言；SQL Server 参数绑定、事务和稳定快照需要补齐 |
| Data Transfer | 已注册 `SqlServerSyncAdapter`，已有类型转换、字面量和三段式表名处理 | 类型语义、IDENTITY、复杂结构和可靠写入尚不完整 |
| 结构元数据 | 已读取列、默认值、自增和主键标记 | 索引、外键、CHECK 返回空；复合主键由列顺序收集，不能代表真实键顺序 |
| 查询执行 | 已有查询、流式读取和执行接口 | `query_with_params` 当前忽略 `_params`；取消接口直接返回成功 |

主要核对入口：

- [SQL Server 驱动](../../packages/drivers/sqlserver/src/sqlserver.rs)
- [现有传输适配器](../../packages/drivers/sqlserver/src/sync_adapter.rs)
- [SQL 目标作用域处理](../../packages/drivers/sqlserver/src/sql_target.rs)
- [公共驱动契约](../../packages/driver-api/src/traits.rs)
- [结构迁移契约](../../packages/driver-api/src/schema_migration.rs)
- [同步与传输适配器契约](../../packages/driver-api/src/sync/adapter.rs)
- [同步比较](../../src-tauri/src/commands/sync/compare.rs)
- [同步应用](../../src-tauri/src/commands/sync/apply.rs)
- [同步 Keyset 读取](../../src-tauri/src/commands/sync/keyset_source.rs)

## 3. 共用基础设计

先完成共享基础，再接入三条业务链路。SQL Server 方言和专属行为留在驱动，Host 只负责通用规划、调度、校验和结果汇总。

### 3.1 完整结构元数据

在 SQL Server 驱动内部建立统一 catalog 读取模块，覆盖：

- 列完整类型：长度、精度、小数位、时间精度、字符排序规则。
- 主键：约束名、真实键顺序、聚集属性。
- 索引：唯一性、键顺序、排序方向、INCLUDE、过滤条件。
- 外键：列映射顺序、引用对象、更新及删除动作、启用和可信状态。
- DEFAULT / CHECK：约束名称、表达式及状态。
- IDENTITY：seed、increment；当前计数值属于运行状态，不参与结构相等判断。
- computed、rowversion、temporal、特殊索引等特征，用于能力检查。

现有公共模型不能表达的信息，应扩展有明确语义的字段或类型；不能将缺失属性塞进 SQL 字符串后声称结构完整。

读取权限不足与对象确实不存在必须区分。不完整元数据不能生成具有删除能力的计划。

### 3.2 命名与作用域

统一使用显式 `database + schema + object`：

- 标识符逐段引用，正确处理 `]`、点号、空格和 Unicode。
- 元数据查询与执行 SQL 必须指向同一作用域。
- 用户选定的 schema 全程透传，不能执行时静默回退到 `dbo`。
- 对象名称比较依据实际排序规则，不能统一转小写。
- 检测源、目标是否指向同一物理对象，避免自同步和自覆盖。
- 配置归属使用 `connectionId`，已建立会话操作使用 `dbSessionId`，禁止双模回退。

### 3.3 参数、事务与会话

补齐驱动契约：

- 真正绑定 `@P1` 等参数，覆盖 NULL、Unicode、二进制、decimal、UUID、日期时间。
- 参数类型结合列类型，避免 decimal 经浮点数中转、带时区时间丢失偏移量。
- 实现事务开始、提交和回滚，保证整个事务固定在同一物理连接。
- 迁移任务拥有独立会话，或明确的会话独占机制。
- 失败、取消、断线后清理事务和会话状态；无法证明干净的连接直接废弃。
- 区分请求取消、数据库执行已停止、事务已回滚，不能提前报告取消成功。

现有 `sqlserver.rs` 已较大，新增能力拆为 metadata、binding、transaction、migration 等职责模块，避免继续扩大单文件。

## 4. Schema Diff

### 4.1 接入方式与能力范围

接入 `MigrationRenderer`、`MigrationCapabilities`、`TypeNormalizer`。Host 继续生成中立操作，T-SQL 由驱动生成。

| 对象 | 首批计划支持 |
| --- | --- |
| 表、普通列 | 创建、删除、增删列、类型与可空性变更 |
| DEFAULT | 读取实际约束名，删除旧约束并创建新约束 |
| 主键、普通索引、唯一约束 | 基于完整元数据创建、删除和重建 |
| 外键、CHECK | 创建、删除及必要的依赖处理 |
| 视图、过程、函数、触发器 | 定义完整可见、作用域映射可靠时支持 |
| 特殊对象 | 明确显示不支持原因，禁止生成部分可执行计划 |

### 4.2 规划与执行规则

1. 类型归一化只消除真正等价的写法。长度、精度、排序规则以及 Unicode / 非 Unicode 差异不能被抹掉。
2. DEFAULT 作为独立约束处理。修改默认值必须使用目标实际约束名，不能套用其他数据库的列默认值语法。
3. 依赖顺序必须完整：先处理阻塞变更的依赖，再变更主体，最后恢复约束和依赖对象；同时检查未被选择的对象。
4. IDENTITY 属性变化不能当作普通 ALTER COLUMN。首版阻止自动执行并解释原因；后续若支持表重建，独立实现数据复制、依赖恢复和失败恢复流程。
5. 模块定义保持批次语义。复用并验证已有独立批次处理，客户端分隔符不能直接作为服务器 SQL 执行。
6. 执行前重新验证目标快照。预览后若结构变化，应重新生成计划。
7. 如实区分事务回滚、逆向 DDL 和数据恢复。删除数据后的逆向建表不能标记为完整恢复。

### 4.3 验收

执行成功后重新比较，受支持对象不再出现差异。不支持或依赖不完整时，执行前阻断，不能执行计划前缀后才报告不支持。

## 5. Data Sync

### 5.1 接入方式

保留 Gate → Compare → ChangeSet → Preview → Apply 流程，替换其中的方言假设。

重点改造 `commands/sync/compare.rs`、`apply.rs`、`keyset_source.rs`：

- 引用符、参数占位符、分页、字面量和键排序通过驱动能力获取。
- 消除“非 MySQL 就按 PostgreSQL 处理”的分支。
- Host 只保留通用比较、冲突处理和任务管理逻辑。

### 5.2 比较一致性

- 同族校验后，验证列结构、主键列及顺序、可写属性。
- 使用稳定读取快照。数据库未启用所需能力时，在预检解释原因，不自动修改数据库配置。
- 源和目标快照分别稳定，不宣称两端拥有同一时间点的分布式快照。
- Keyset 分页必须使用与比较器一致的排序语义。
- 特别验证字符串排序规则、尾随空格、复合主键、`uniqueidentifier`、decimal 键。
- 无法保证排序一致的键类型明确拒绝，不能套用不可靠的默认字符串比较。

### 5.3 写入与冲突

- INSERT / UPDATE / DELETE 使用参数绑定。
- 更新和删除带旧值或已定义的并发校验条件，检测比较后目标被修改。
- 受影响行数异常进入冲突处理，不能计为成功；验证触发器和 `NOCOUNT` 场景。
- 保留已有跳过、覆盖等策略语义，不以 `MERGE` 替换整套冲突逻辑。
- IDENTITY 列允许按原值插入，但不作为普通可更新列。
- computed 与 rowversion 不直接写入；生成列策略在预览中可见。
- rowversion 用于目标并发校验，不作为普通业务差异列；不能要求两端版本值相等。

rowversion 是数据库生成的版本值，不是日期时间。参考 [Microsoft rowversion 文档](https://learn.microsoft.com/en-us/sql/t-sql/data-types/rowversion-transact-sql?view=sql-server-ver17)。

### 5.4 验收

应用成功后再次比较，业务可写列无差异。冲突、取消和失败不会留下未经报告的部分写入。生成列的差异处理规则明确、可重复，不造成无限重复同步。

## 6. Data Transfer

扩展现有 `SqlServerSyncAdapter`，不新建平行传输框架。

### 6.1 类型转换

优先纠正或补齐以下语义：

- `tinyint` 与 IR `Int8`：明确无符号范围，双向转换不能直接视为等价。
- `datetimeoffset`：保留时区偏移，不能统一转成不带偏移的 `DATETIME2`。
- `decimal/numeric`：完整保留 precision / scale，禁止浮点中转。
- `money/smallmoney`：定义精确映射及范围检查。
- `char/varchar` 与 `nchar/nvarchar`：检查字符集、长度单位和排序规则。
- `binary/varbinary`：保留固定长度与变长语义。
- XML、空间类型、UDT、`sql_variant`：逐项明确原生保留、显式转换或拒绝。
- 未知类型不能静默退化为 `NVARCHAR(MAX)`。
- 默认表达式经过目标方言确认后才能迁移，不能原样跨库复制。

计划预览为每列显示目标类型，以及无损、需确认转换、不支持的结果。

### 6.2 IDENTITY 与生成列

- 保留显式主键值时，在同一会话管理 `IDENTITY_INSERT ON/OFF`。
- 一个会话同时只能为一张表开启该设置，因此并行任务使用独立会话，或按表串行管理。
- 成功、异常、取消路径都关闭设置；清理失败则废弃连接。
- 非默认 seed / increment 完整保留，或在规划阶段拒绝。
- computed、rowversion 默认由目标生成；异构目标若需保留源值，应明确转为普通列，不能混淆两种语义。

参考 [Microsoft SET IDENTITY_INSERT 文档](https://learn.microsoft.com/en-us/sql/t-sql/statements/set-identity-insert-transact-sql?view=sql-server-ver17)。

### 6.3 结构与批量写入

- 根据参数预算、行数和数据体积动态划分批次，为实际执行路径保留参数余量。
- 创建表、导入数据、创建索引和外键按依赖图排序。
- 循环外键单独规划；恢复约束后验证数据和约束状态。
- 默认不禁用触发器，执行影响在预检中提示。
- 现有适配器对复杂结构的拒绝检查逐项解除：某类元数据和渲染能力完成后，才开放该类对象。
- SQL 文件导出和在线传输使用一致的类型、IDENTITY、作用域规则。
- 恢复点只在事务提交后落盘，绑定源范围、目标结构和配置指纹，防止恢复时重复或漏写。

## 7. 实施顺序

| 阶段 | 主要落点 | 验收条件 |
| --- | --- | --- |
| 1. 能力与模型 | `packages/driver-api`、SQL Server 驱动 | 元数据完整度、可写列、类型与事务能力可表达 |
| 2. 驱动执行基础 | SQL Server binding / transaction / metadata 模块 | 参数往返、回滚、快照、会话清理通过真实数据库测试 |
| 3. Schema Diff | 驱动 migration 模块、Host 通用规划接口 | 比较→预览→执行→复核完整闭环 |
| 4. Data Sync | 驱动键契约、Host sync 方言调用点 | 比较准确、冲突可检测、应用后业务差异归零 |
| 5. Data Transfer | 现有 adapter、通用 writer / preflight 接口 | 双向传输无静默损失，IDENTITY 与失败恢复可靠 |
| 6. UI 与回归 | 三件套窗口、驱动 UI、正式文档 | 能力提示准确，连续用户旅程与既有驱动回归通过 |

公共协议若发生需要版本升级的变化，同步更新所有受影响插件。新增接口尽量提供明确的“不支持”默认行为，避免其他驱动被错误视为具备能力。

前端通过驱动元数据与后端能力结果展示状态，不添加 Host 数据库类型硬编码。开发期间仅修改英文 `en.ts`；发布前检查并补齐翻译。

完成实现后，将长期有效的使用边界和架构事实并入 `docs/features/`、`docs/architecture/`、`docs/development/` 对应文档，不把未实现方案描述成已实现事实。

## 8. 测试与发布标准

### 8.1 测试落点

SQL Server 专属实现、方言、Command 和 UI 测试全部放在 `packages/drivers/sqlserver/`：

- Rust 单元测试：对应模块内。
- Rust 集成测试：`packages/drivers/sqlserver/tests/`。
- 驱动 UI 单测：`packages/drivers/sqlserver/ui/__tests__/`。
- 驱动专属 E2E：`packages/drivers/sqlserver/e2e/`。

Host 只测试通用能力分派、任务生命周期及跨驱动契约，不承载 SQL Server 专属测试。

### 8.2 必测场景

- 多 database、多 schema、同名表、特殊字符名称。
- 列顺序不同于复合主键顺序。
- Unicode、二进制、NULL、decimal 边界、日期时间精度和偏移量。
- IDENTITY 插入、失败清理、下一次任务继续执行。
- 过滤索引、INCLUDE、外键、DEFAULT、CHECK 的完整性。
- 无元数据权限、快照不可用、断线、死锁、目标并发修改。
- 分批边界、取消、恢复、触发器和受影响行数。
- SQL Server 作为源和目标，以及 PostgreSQL / MySQL 与 SQL Server 的双向传输。
- 用户切换连接、database、schema 后旧比较结果失效；失败或取消后可重新规划执行。
- 对不支持的对象或类型，验证执行前拒绝且没有部分写入。
- 对既有驱动验证能力接口变更没有引入回归。

交互测试必须覆盖连续用户旅程及中间状态，不能只测试静态合法输入。

### 8.3 验证方式

使用驱动单测、真实数据库集成测试、`pnpm typecheck` 和 WDIO。测试文件参与类型检查，禁止用 `any` 绕过。

WDIO 必须经 `pnpm tauri:build:webdriver` 构建并确保注入 SQL Server 驱动，不能用裸 `cargo build` 替代。已生成可运行 `.app` 后单独发生的 DMG 打包失败不阻塞功能验收，继续用该应用运行 WDIO。

本节是实施前的验证门槛。首轮实现后的测试范围及仍未完成的真实数据库验证见第 9 节。

## 9. 首轮实现结果与边界

代码已在 `codex/sqlserver-migration-plan` 分支集成。首轮实现开放了能通过驱动能力和元数据证明安全的子集；不代表第 1 节列出的所有数据库对象、类型和恢复场景都已支持。

共享驱动基础已增加 SQL Server 列、主键顺序、索引、外键和 CHECK 目录读取，并在元数据缺失、排序语义不可表达、特殊列或特殊索引无法安全建模时拒绝继续。数据库物理身份使用服务器身份与目录中的当前数据库 ID；Schema Diff 同库作用域再核对 `schema_id`。`ReuseDriver` 转发这些身份能力。参数绑定上限按 SQL Server 的 2100 个参数处理。

Schema Diff 已接入 SQL Server migration renderer、能力检查和类型归一化。只执行 renderer 声明支持且元数据完整的操作；IDENTITY 属性重建、routine/trigger、sequence、用户定义类型，以及无法证明安全的表选项或索引特征会被阻断。SQL Server 对象专用计划暂不做跨 schema DDL 重写，检测到来源与目标 schema 不一致时清空计划语句并返回 `Unsupported`。

Data Sync 已接入 SQL Server 同族比较、参数化写入和会话级 IDENTITY_INSERT 管理。相同目标表跨分页时复用开关，切换表、失败、取消、冲突和提交前均执行清理；关闭失败时先回滚再丢弃连接。运行时目标信息不进入预览 JSON。无法证明排序语义的文本、`uniqueidentifier` 和 `rowversion` 键会被拒绝；投影中的 identity 值缺失或为 NULL 时也在执行前拒绝。

Data Transfer 已支持 SQL Server 源和目标路径，包括安全目标关系引用、2100 参数分块和会话级 identity 清理。SQL 文件按 INSERT 实际映射的目标列与目标表 identity 目录信息的交集决定是否生成 IDENTITY_INSERT 包装，避免用来源列属性猜测目标 identity。未知类型、无法保留的排序规则或不完整结构仍在预检中拒绝。

离线验证结果：Schema Diff Host 定向测试 228 项通过；Data Sync 执行器 37 项、SQL 生成 6 项、命令路径 28 项、执行 IPC 6 项通过；Data Transfer Host 定向测试 203 项通过；SQL Server 驱动 96 项、driver-api 199 项通过。格式检查和提交差异检查通过。

尚未连接真实 SQL Server，因此 TDS 会话 toggle、Azure SQL 实际权限/目录可见性、跨数据库权限及端到端迁移结果仍需 live/integration 验证；WDIO/E2E 也未运行。实现与测试均没有读取或加载 `.env` / `.env.test`。发布前仍需按第 8 节补齐真实数据库验证，并继续确认所有不支持路径都在写入前拒绝。
