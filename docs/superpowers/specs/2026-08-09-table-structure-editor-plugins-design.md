# 表结构编辑器 — 插件配置 + 驱动自报能力 设计

> 日期：2026-08-09  
> 状态：已批准  
> 实现计划：[../plans/2026-08-09-table-structure-editor-plugins.md](../plans/2026-08-09-table-structure-editor-plugins.md)  

> 分支上下文：`feat/abc-competitive-parity`  
> 相关：[Schema Diff Deploy](../plans/2026-08-09-schema-diff-deploy.md)、DBX 表结构文档 / `tableStructureCapabilities.ts`

## 1. 问题

DataZen 已有**表结构编辑器**（`TableStructureEditor.tsx`）以及独立的**索引**页签（`IndexesView.tsx`）。相对 DBX 的差距在于：

- Host 编辑器里的 DDL 与类型列表实质上按 **PostgreSQL 形态**写死。
- 索引未与列变更进入同一份草稿 / SQL 预览。
- 驱动尚未**自行上报**结构能力 / 为编辑器规划 DDL（Host 自行拼 PG 风格 SQL）。
- 竞品文档曾误写成「没有表结构编辑器」；真实差距是**深度与方言覆盖**，不是有无入口。

收敛目标：不为每个驱动再写一套 React 结构编辑组件，也不把文档库 / KV 引擎硬塞进 SQL 表模型。

## 2. 目标（P1）

1. **唯一 Host 壳**：新建 / 改表时，列 + 索引同一草稿、SQL 预览、执行。
2. **UI 只消费配置**：驱动导出 config（类型列表、字段显隐、索引 method），**不**提供自定义结构编辑 React 组件。
3. **安全真相在后端**：每个 SQL 驱动实现 `structure_capabilities`（感知连接，含服务器版本）与 `plan_structure_changes`（意图 diff → 方言 DDL）。
4. **Host 不维护能力对照表**（有别于 DBX 的 `capabilityByType`）。Caps **由当前连接上的驱动返回**；Host 只渲染驱动上报的结果。应用 crate 内不得出现结构能力的中央 `match db_type { … }` 注册表。
5. **所有 `supportsSQL` 方言均可挂载**：未覆盖 trait 的驱动保留**默认不支持**实现（多数编辑操作禁用，并有清晰 UX）。
6. **能力随版本变化**：逻辑写在**驱动内部**（例如读取 `server_info` 再填充返回结构）。同一驱动 id、不同主/次版本 → 返回不同 caps；版本未知 → 该驱动的保守基线。Host 从不按版本分支。
7. **非表模型 opt-out**：MongoDB、Redis 等不进入本壳（与 DBX 专用工作区产品划分一致）。

## 3. 非目标（P1）

- 结构壳内外键的创建 / 编辑（FK 仍走现有只读视图）。
- MongoDB 集合 / validator / GridFS 编辑器（如需另开 epic）。
- 每驱动自定义结构 UI 组件。
- 字段血缘、对象浏览器与 DBX 对标。
- 在线改表（pt-osc / gh-ost）。
- 替代 Schema Diff Deploy（跨连接同步仍为独立路径）。

## 4. 产品规则（已锁定）

| 决策 | 选择 |
|------|------|
| 方言覆盖野心 | 所有 `supportsSQL` 注册项可挂载；深度因驱动而异 |
| UI 策略 | 仅配置；唯一 Host 壳 |
| SQL 生成 | Rust 驱动 trait（不是 Host `if dialect`） |
| 索引 | P1 与列同屏、同一草稿 |
| FK | P1 只读 |
| MongoDB / Redis / 文档 / KV | 结构壳 opt-out |
| 版本差异 | **驱动内**经连接 `server_info` 解析；前端从不解析版本字符串 |
| 未知 / 不支持 | 驱动默认 trait 实现 = 全 false / 空 plan（失败关闭） |
| Caps 唯一真相源 | **仅驱动方法返回值** — 不另建 Host 表与之同步 |

## 5. 架构

### 5.1 分层

```
Host TableStructureEditor（列 + 索引草稿）
  - UI 来自 StructureEditorUiConfig（驱动 meta）
  - 启用/禁用来自 StructureCapabilities（IPC）
  - 预览/执行走 plan_table_structure_changes
           |
     +-----+-----+
     |           |
 前端配置      后端 DatabaseDriver
 structureEditor  structure_capabilities(handle)  // 含版本
 仅 config        plan_structure_changes(diff)    // 方言 DDL
```

### 5.2 前端配置（插件，无组件）

驱动通过现有 UI meta / `resolve-drivers` 合并路径贡献（与 `DatabaseTypeMeta` 相同），例如：

```ts
structureEditor?: {
  /** 省略或 false，且 !supportsSQL → Host 隐藏入口 */
  enabled?: boolean;
  columnTypes: { value: string; label: string }[];
  defaultColumnType: string;
  fields: {
    comment?: boolean;
    charset?: boolean;
    collation?: boolean;
    unsigned?: boolean;
    length?: boolean; // 类型需要时展示长度/精度编辑
  };
  indexMethods: string[]; // 如 ['btree','hash'] — 可被运行时 caps 收窄
};
```

- Host 不得为本功能 import 驱动专用编辑器组件。
- Redis/Mongo 省略 `structureEditor` 或设 `enabled: false`；Host 对这些连接模式隐藏「编辑表结构 / 新建表」（或短文案引导至专用视图）。

### 5.3 后端能力 + 规划（插件）

扩展 `packages/driver-api` 的 `DatabaseDriver`，默认方法返回不支持 / 空 plan：

```rust
fn structure_capabilities(
    &self,
    handle: &ConnectionHandle,
) -> impl Future<Output = Result<StructureCapabilities, DriverError>> + Send;

fn plan_structure_changes(
    &self,
    handle: &ConnectionHandle,
    request: &StructureChangeRequest,
) -> impl Future<Output = Result<StructureChangePlan, DriverError>> + Send;
```

**`StructureCapabilities`**（字段名示意；IPC 用 serde `camelCase`）：

- 列：`createTable`、`addColumn`、`dropColumn`、`renameColumn`、`alterType`、`alterNullability`、`alterDefault`、`alterPrimaryKey`、`reorderColumn`、`comment`
- 索引：`createIndex`、`dropIndex`、`rebuildIndex`、`indexType`、`indexInclude`、`indexFilter`、`indexComment`
- 元信息：`alterStrategy`（`none` | `direct` | `sqlite_rebuild`）、`dialectId`（诊断用）
- 可选随 caps 下发的覆盖：`indexMethods: Vec<String>`（已按版本过滤）

**版本处理（必须，且仅在驱动内）：**

1. 在驱动实现中从活跃连接读取服务器版本（复用 / 扩展 `ServerInfo` 等）。
2. 在该驱动内组装 `StructureCapabilities`（基线 + 版本补丁，例如 PostgreSQL ≥ 11 → `index_include = true`）。
3. 版本无法解析 → 使用该驱动的保守基线。
4. Host/前端**不得**维护第二份 type→caps 映射，也**不得**解析版本字符串；只应用 IPC 载荷。

**反模式（明确禁止）：** Host 模块如 `structure_capabilities_by_db_type: HashMap<DatabaseType, Caps>` 复制驱动知识。DBX 桌面端采用此类表；DataZen **不采用**。

**`StructureChangeRequest`**：意图 diff（稳定列 id、原始/当前列与索引、create vs alter、表名）。违反 caps 的意图由驱动以明确校验错误拒绝（禁止静默生成错误 SQL）。

**`StructureChangePlan`**：有序语句 `{ sql, summary, risk }`，供预览与执行（风险词汇尽量与 schema-diff 对齐：additive / destructive / rewrite）。

### 5.4 Host 壳行为

1. 加载表 schema + `get_structure_capabilities` + UI 配置。
2. 编辑本地草稿（列 + 索引）。
3. 对应 cap 为 false 时禁用控件，并用 i18n 给出简短原因。
4. 预览调用 `plan_table_structure_changes`，展示 SQL 列表。
5. 执行语句（事务策略诚实：PG 可尝试包装；MySQL DDL 常自动提交 → 不得声称整批回滚）。文案与 Schema Diff Deploy 原子性说明对齐。
6. 成功后刷新 schema 缓存 / 树。

列重排：

- `reorderColumn == true` → 顺序变化时 plan 必须发出物理重排 DDL。
- `false` → 若仍允许本地拖拽，必须明确不落库（P1 建议：`reorderColumn` 为 false 时**禁用拖拽**，避免误导）。

### 5.5 与 IndexesView / Schema Diff 的关系

| 功能 | P1 之后的角色 |
|------|----------------|
| 结构壳 | 单表列 + 索引草稿 DDL 的主路径 |
| `IndexesView` | 可保留为便利页签或薄封装；SQL 生成不得分叉（调用同一 plan API，或改为只读列表 +「在结构编辑器中修改」） |
| Schema Diff Deploy | 跨连接「源=期望 → 目标」部署；结构编辑器是单连接编写 |

### 5.6 与 DBX 对照（仅参考）

| 主题 | DBX | DataZen（本设计） |
|------|-----|-------------------|
| Caps 存放 | 桌面端 `capabilityByType` 表 + 辅助函数 | **无 Host 表** — `DatabaseDriver::structure_capabilities(handle)` |
| 版本分叉 | Host 辅助函数调整 caps（如 PG major） | 驱动实现调整返回结构 |
| MongoDB | 专用工作区，非表结构编辑器 | 同样产品划分（opt-out） |
| DDL 生成 | 桌面方言 SQL 助手 + caps | 驱动 `plan_structure_changes` |

## 6. IPC

前端 `invoke` 键名遵循项目既有约定（Rust 参数 `snake_case`，前端侧多为 camelCase 桥接）。

拟定命令（实现计划中可定稿命名）：

- `get_structure_capabilities(connection_id)` → `StructureCapabilities`
- `plan_table_structure_changes(connection_id, request)` → `StructureChangePlan`

执行可复用 `execute_query` / `execute` 跑计划中的 SQL 批；除非事务包装需要专用辅助，否则不新造第二套执行器（可与 schema-diff deploy 共用模式）。

## 7. 适配器落地节奏

| 档位 | 驱动 | 期望 |
|------|------|------|
| T0 | postgres、mysql、sqlite | P1 列 + 索引 plan 质量完整 |
| T1 | sqlserver、clickhouse、duckdb 等 | 支持子集的 caps + plan；其余禁用 |
| T2 | Wire 复用类型（questdb、doris 等） | **显式收窄** caps（禁止盲目复制父方言） |
| Opt-out | mongodb、redis 等 | 无结构壳入口 |

P1 可先交付 T0 完整 + 其余默认不支持；T1/T2 后续增量填充，无需改 Host。

## 8. 测试

- 单元（驱动 crate / 调用驱动假实现的 datazen 测试）：版本补丁写在 **postgres 驱动内**（如 PG 10 vs 14 的 `index_include`）；PG/MySQL/SQLite 的 add/drop/rename/index plan SQL 快照。
- 护栏：Host crate 不得长出 `capabilityByType` 风格的结构 caps 表（代码审查 / 计划中可选 `rg` 检查）。
- 单元：Host 辅助函数 `isControlEnabled(caps, 'renameColumn')`。
- 集成 / e2e（现有 `e2e/specs/table-structure.ts`）：PG 上预览 + 保存仍可用；有环境时覆盖 MySQL。
- 回归：Mongo/Redis 连接不得出现误导性的「编辑表结构」并打开空 SQL 壳。

## 9. 文档 / i18n

- 面向用户：使用说明或连接指南中简短说明预览 DDL、方言限制、按版本禁用控件。
- 竞品对比：表结构编辑器 = 已有；差距 = 方言深度 / 索引同草稿 / 驱动自报含版本的 caps（文档已部分纠正）。

## 10. 留给实现计划的开放项

- serde 字段最终命名；caps IPC 是每次打开编辑器拉取还是按 connection id 缓存。
- 执行是一次多语句还是逐条带进度。
- 扩展 `driver-api` 时的 `PROTOCOL_VERSION`  bump 策略（必须 bump 并同步插件）。

## 11. 成功标准

- PG / MySQL / SQLite：新建表 + 改列 + 创建/删除索引走同一预览/执行流。
- 非 T0 SQL 驱动：可打开编辑器且不崩溃；不支持的操作禁用。
- MongoDB：无 SQL 表结构编辑入口（或明确「不适用」提示）。
- 不引入任何新的「每驱动一套」结构编辑 React 组件。
