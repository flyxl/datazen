# DataZen 数据同步功能 PRD

**产品：** DataZen  
**功能：** Data Synchronization / 数据同步  
**版本：** V1.2  
**状态：** Draft  
**参考：** Navicat Data Synchronization  
**关联架构：** [data-sync.md](./architecture/backend/data-sync.md)  
**相关文档：** [Data Transfer PRD](./data-transfer-prd.zh-CN.md) · [Schema Diff / 结构同步](./schema-diff-guide.md)

## 0. 三套能力，不要混用

对标 Navicat，DataZen 拆成三个独立产品。**本次只做 Data Synchronization。**

| | Structure Synchronization | Data Synchronization（本次） | Data Transfer / Migration |
|---|---|---|---|
| Navicat 对应 | Structure Synchronization | Data Synchronization | Data Transfer |
| DataZen 入口 | Schema Diff 窗口（已有） | Data Sync 窗口（改造） | 本次不实施，见 Transfer PRD |
| 比什么 | 表结构（列、索引、约束） | **行数据**差异 | **不比较**，单向搬运 |
| 改什么 | Target 的 DDL | Target 已有表上的 INSERT/UPDATE/DELETE | 在 Target **创建或写入**对象并拷贝行 |
| 库类型 | Schema Diff 已支持跨方言 IR | **必须同 dialect family** | **允许异构** |
| 表结构 | 本来就可以不同（要对齐） | **必须完全一致** | **允许不同**；用户提供字段映射 + 类型映射 |
| 主键 | 不强制（结构变更可含 PK） | **双方必须有相同 Primary Key** | 不要求 |
| 目标表 | 已存在 | **必须已存在** | 可以不存在（先建表再灌数） |
| 典型用途 | 把结构对齐到期望态 | 两库补差、可审查删除 | 迁库、异构导入、空库灌数 |

```text
结构不同、或无 PK、或目标表不存在、或异构库
    → Structure Sync 和/或 Data Transfer
结构完全一致 + 双方有相同 PK + 同类型库
    → Data Synchronization
```

**禁止**用 Data Sync 做迁库、建表灌数、改列名拷贝。那是 Transfer。  
仓库里现有 `DROP + CREATE + INSERT` 更接近简陋 Transfer，**不并入 Sync**；Transfer 实施时再吸收或重写。

---

## 0.1 产品决策（Synchronization）

目标：**对标 Navicat Data Synchronization**

> **Compare → Review → Generate Changes → Preview SQL → Execute**

| 决策 | 说明 |
|---|---|
| 产品形态 | 增量 Diff 同步，不是克隆/覆盖拷贝，也不是 Transfer |
| 前提（硬门闸） | ① 同 dialect family ② 映射表**结构完全一致** ③ **双方具有相同 Primary Key** |
| 旧引擎 | 删除/停用 DROP/CREATE/INSERT 与跨库 IR 拷贝（留给 Transfer，不在 Sync 里打补丁） |
| 旧任务 | `sync_tasks.json` 不迁移 |
| 旧 UI | 复用 `data-sync` 单例窗口，内部改为 Diff Workspace |
| Transfer | 见 [data-transfer-prd.zh-CN.md](./data-transfer-prd.zh-CN.md)，本次不开发 |

---

## 1. 产品背景

数据库开发、测试和运维经常需要在两个库之间对齐数据：开发 → 测试、本地 → 远程、测试库互拷。传统方式依赖手工 SQL、dump 或脚本，看不清差异，也难以控制删除范围。

Navicat 的做法是先比较、再审查、再生成脚本、最后执行。DataZen 采用同一闭环，并用现代 Diff Workspace（表 / 行 / 单元格）加强审查，而不是 Wizard 点 Next。

## 2. 产品目标

用户可以通过可视化界面完成：

1. 选择 Source / Target（连接、数据库、Schema）
2. 配置同步策略（Insert / Update / Delete）
3. 自动匹配 Table，支持手工映射
4. 比较数据差异（可取消、有进度）
5. 查看 Table / Row / Cell 级差异
6. 按 Table / Operation / Row 选择要应用的变更
7. 生成参数化 SQL，只读 Preview
8. 执行前校验（连接、权限、只读、目标是否已变）
9. 在事务中执行（数据库支持时）
10. 查看执行结果；Apply 后再 Compare 应为 0 差异

目标是让用户无需手写 SQL，即可**安全、可审查**地把选定差异应用到目标库。

## 3. 非目标

V1 Data Sync **不做**（其中多项属于另外两个产品）：

| 不做 | 应去哪 |
|---|---|
| 结构对齐、DDL 部署 | Structure Sync / Schema Diff |
| 异构库、不同表结构、字段映射、类型转换、目标建表灌数 | [Data Transfer](./data-transfer-prd.zh-CN.md) |
| 无 PK 表的数据搬运 | Data Transfer |
| 行过滤 WHERE、列勾选、值变换 | Transfer（或 Sync 明确不做） |
| CDC / 定时 / CLI / Workflow / MCP | 后续 |
| 手改 Preview SQL 再执行 | Sync P2 / 不做 |
| 视图、临时表、系统表 | 不做 |

V1 只做 **同 dialect family + 结构完全一致 + 有 PK** 的基表，例如 MySQL ↔ MySQL/MariaDB、PostgreSQL ↔ PostgreSQL。

## 4. 核心概念

```text
Sync Task
├── Source
├── Target
├── Sync Options
├── Table Mapping
├── Comparison Result
├── Change Set
└── Execution Result
```

与旧实现的「选表 → 整表覆盖」不同：Compare 产出差异，用户选出 Change Set，再生成 SQL 并执行。

## 5. 用户流程

```text
进入 Data Synchronization（现有菜单 / 侧栏 / 窗口）
          ↓
选择 Source / Target
          ↓
配置 Synchronization Options
          ↓
Table Mapping
          ↓
Start Comparison
          ↓
Comparison Result（表级摘要）
          ↓
Table Diff → Row / Cell Diff
          ↓
选择需要同步的 Change
          ↓
Generate SQL（只读 Preview）
          ↓
Execute 前安全检查
          ↓
Execute（事务）
          ↓
Execution Result
          ↓
可选：再 Compare，确认 0 差异
```

## 6. 功能入口

复用现有入口，不新增窗口 kind：

- 主窗口 Tools / 侧栏 Action → Data Sync（`openDataSyncWindow()`，单例 `data-sync-singleton`）
- 原生菜单 `menu:data-sync`
- Connection / Database / Table 右键 → Data Synchronization...（**原生 Menu**，预填 Source）

打开后进入 Diff Workspace，不再进入「选表后直接覆盖」流程。

## 7. Source / Target

同步任务必须选择：

- Connection
- Database（`hasMultiDatabase` 时）
- Schema（数据库支持时）

```text
Source DB  ─────────→  Target DB
```

提供 **Swap**。Swap 或更换任一端后，已有 Comparison / Change Set / SQL 全部作废。

约束：

- Source 与 Target 必须 **Driver Type Compatible**（同一 `normalize_sync_family`，如 `mysql`/`mariadb`）。
- **禁止** Source 与 Target 为同一连接 + 同一数据库 + 映射到自身的同一张表。
- Target 连接若标记 `read_only`，允许 Compare，禁止 Execute。
- Source 只读允许 Compare。

## 8. Synchronization Options

### 8.1 Insert

默认开启。Source 有、Target 无 → 插入 Target。

### 8.2 Update

默认开启。两边都有但单元格不同 → 更新 Target（只 SET 变化列）。

### 8.3 Delete

默认关闭。Target 有、Source 无 → 从 Target 删除。

开启时必须风险提示并确认：

> Records that exist only in the target database will be deleted.

执行前若 Change Set 含 DELETE，再确认一次（与设置项 `confirmOnDelete` 一致：开启则强制确认）。

## 9. 前提条件（硬门闸）

进入 Compare **之前**必须全部满足，任一失败则该表 `INCOMPATIBLE`，不得比较、不得生成 Change Set。

### 9.1 同类型

Source / Target 属于同一 `normalize_sync_family`（如 `mysql` 与 `mariadb`）。异构对直接拒绝，并提示改用 Data Transfer。

### 9.2 必须有相同 Primary Key

- 双方都是 **PRIMARY KEY**（不是「随便唯一列」）
- PK **列集合与顺序相同**（含复合主键 `(tenant_id, user_id)`）
- 无 PK → 不可 Sync；提示：先在 Structure Sync 加主键，或改用 Transfer 做单向拷贝

V1 **不提供** Unique Index / Custom Columns 作为匹配键。没有 PK 就不做增量同步。

### 9.3 表结构必须完全一致

对每一对已映射的表，下列必须逐项相同。按**列名**对齐（允许两边物理列顺序不同）。

| 必须一致 | 说明 |
|---|---|
| 列名集合 | 不许多列、不许缺列 |
| 列类型 | 同 family 下类型等价（如 `INT`=`INTEGER`）；禁止跨类型（`INT`≠`BIGINT`、`TEXT`≠`VARCHAR(20)` 除非 Driver 判定为同一存储类型） |
| 可空性 | `NULL` / `NOT NULL` 相同 |
| 主键 | 见 9.2 |

**不作为硬门闸（警告即可）：** 非 PK 索引、外键、触发器、注释、存储引擎。这些差异应提示「建议先做 Structure Synchronization」，但不阻止数据同步。

生成列 / IDENTITY：结构仍须一致；写入规则见实施方案（PK 若为 IDENTITY，INSERT 必须写入 PK 值）。

不满足 9.3 → 列出具体列差异，引导 Schema Diff 或 Transfer（带字段映射），**禁止**只同步「交集列」。

视图 / 物化视图 / 临时表：不进入可同步列表。

## 10. Table Mapping

默认按表名自动匹配。**表名可以不同**（`customers` → `clients`），但映射成功后仍须通过 §9 结构+PK 检查。

```text
MATCHED          同名或手工映射，且通过 §9
UNMAPPED_*       未配对
DISABLED         用户排除
INCOMPATIBLE     无 PK / 结构不一致 / 非基表 / 不同 family
```

V1 不做列级映射 UI。需要改列名或丢列 → Transfer。

## 11. Comparison

点击 Compare 后比较。显示整体和单表进度，可取消（必须取消后端查询，不只停 UI）。

完成后摘要（口径必须分开）：

```text
12 tables compared
  3 tables with differences
  8 tables unchanged
  1 table incompatible / skipped

Row changes (selected tables):
  35 inserts
  171 updates
  4 deletes
```

**Unchanged 默认指表**；行级 unchanged 在表详情中展示，不与 inserts 混在一个数字里。

## 12. Comparison Result

顶部：

| 类型 | 数量 |
|---|---:|
| Inserts（行） | 35 |
| Updates（行） | 171 |
| Deletes（行） | 4 |
| Unchanged tables | 8 |

Table 列表：Insert / Update / Delete 行数 + 状态。

筛选：All / Insert / Update / Delete / Unchanged / Incompatible。支持表名搜索。

## 13. Table / Row / Cell Diff

点击表进入详细 Diff。左右 Source / Target，**必须**能看到单元格旧值/新值，而不是只写 “changed”。

```text
Source       Target
age = 20     age = 21   ← changed
```

选择粒度：

```text
Table  →  Operation（Insert/Update/Delete）  →  Row
```

**P0：**

- 默认选中所有 INSERT / UPDATE
- DELETE 仅在选项开启后可选，默认不选
- 支持按表、按操作全选/取消
- 必须能打开行级 Diff 审查（可分页，默认 ≤ 500 行/页）
- 支持逐行勾选（安全审查的核心；无此则无法做到「只应用选定差异」）

未勾选的变更不得进入 Change Set，不得执行。

## 14. SQL Preview

根据 Change Set 生成 SQL。V1 Preview **只读**：

- 查看、搜索、按 Insert/Update/Delete 筛选
- 复制、保存到文件
- Execute（走后端 Change Set，不解析用户改过的文本）

**V1 不支持在 Preview 里编辑 SQL 再执行**（避免文本与 Change Set 不一致）。编辑执行列为 P1。

内部：

```text
Diff → Change Set → 参数化 SQL → Execution
```

Preview 展示的是同一套语句的可读形式（字面量已格式化）；真实执行必须用参数绑定。二者逻辑必须一致。

BLOB / 超长文本在 Preview 中截断或显示占位（hash / hex 前缀），完整值仍按参数执行。

## 15. Execute 前安全检查

执行前必须检查：

- Target 连接有效，且 **非** `read_only`
- Database / Schema / Table 仍存在
- 结构仍完全一致、PK 未变
- 当前用户具备 INSERT / UPDATE / DELETE（按 Change Set）
- Safe Mode 不拦截本专用执行通道，但只读连接与 Delete 确认仍生效
- Target 在 Compare 之后是否明显变化（表级 revalidation 为 P0；行级冲突为 P1）

若目标已变：

> The target database has changed since the comparison was performed. Re-comparison is recommended.

提供 **Recompare** 和 **Execute Anyway**（Execute Anyway 需确认）。

## 16. Transaction

数据库支持事务时默认：

```sql
BEGIN;
-- Change Set
COMMIT;
```

失败 Rollback。不支持事务时必须提示 Partial Apply，用户确认后才执行。

取消执行：有事务则 Rollback；无事务则停止并标记 Partial Apply。

## 17. 权限与只读

| 条件 | Compare | Execute |
|---|---|---|
| Source `read_only` | 允许 | — |
| Target `read_only` | 允许 | **禁止** |
| 缺少 INSERT/UPDATE/DELETE | 允许 | 禁止对应操作并指出表与权限 |

Driver 无法预检权限时，执行中失败要落到具体 Table / Operation / Row Key / 错误信息。

## 18. Execution

过程中显示每表进度、Processed / Succeeded / Failed。可 Cancel。

完成后：

- 成功：插入/更新/删除条数与耗时
- 有失败：Completed with Errors；可复制错误、回到 SQL Preview
- 支持事务且失败：Rolled back，目标应回到 Execute 前状态

## 19. Sync History（P1）

本地保存执行摘要：Source / Target / Options / Mapping 摘要 / 结果 / 时间 / 耗时。

**不得保存密码。** 默认不保存完整 SQL 与行快照（可设选项）。

## 20. Saved Sync Task（P1）

保存连接 id、库/Schema、Options、Table Mapping、Matching Strategy。不保存密码。

旧版 `sync_tasks.json`（覆盖拷贝断点）不升级。

## 21. 大数据量策略

禁止把整表一次性加载到客户端。使用 Keyset 分页 / Chunk（默认 1000 行）：

```sql
WHERE id > ?
ORDER BY id
LIMIT 1000
```

复合 PK 用有序 tuple 比较与 WHERE。禁止大 OFFSET。

Compare 结果：表摘要进内存/任务；行 Diff 分页拉取或落盘溢出，**禁止**把百万行塞进 React store。

超大表 Hash / Checksum 优化为 P1。

## 22. 大字段与特殊类型

| 策略 | V1 |
|---|---|
| 普通标量 | 全量比较 |
| TEXT / JSON / BLOB | V1 可用 Full；P1 改为服务端 Hash |
| NULL / `''` / `0` / `false` | 必须区分，禁止当字符串比 |
| FLOAT / DOUBLE | V1 按 Driver 规范化后的精确值比；不隐式跨类型（`0` ≠ `"0"`） |
| DATETIME / TIMESTAMP | 同一连接会话时区下比较，避免假 Diff |
| 生成列 / IDENTITY | 不写入 |

## 23. 数据库支持

| Database | V1 Data Sync | 说明 |
|---|---|---|
| MySQL | P0 | 同族 |
| MariaDB | P0 | 与 MySQL 同 family，一并验收 |
| PostgreSQL | P0 | |
| SQLite | P1 | 锁与类型亲和；不做 V1 承诺 |
| SQL Server / Oracle / ClickHouse 等 | 后续 | |
| MongoDB / Redis | 不做 SQL 套用 | 后续独立模型 |

跨 family（如 PG → MySQL）V1 **明确不做**；现有 IR 拷贝路径随旧引擎一并移除或停用。

## 24. 核心数据模型

### SyncTask

```text
SyncTask
├── sourceConnection / targetConnection   （connection id，无密码）
├── sourceDatabase / targetDatabase
├── sourceSchema / targetSchema
├── options
└── tableMappings
```

### ComparisonResult

```text
ComparisonResult
├── tableResults[]
├── insertCount / updateCount / deleteCount   （行）
└── unchangedTableCount / incompatibleCount
```

### TableResult / RowChange

与实施方案一致。Operation：`INSERT` / `UPDATE` / `DELETE` / `UNCHANGED`。

## 25. 状态机

```text
DRAFT → CONFIGURED → COMPARING → COMPARED → REVIEWING
      → GENERATING_SQL → READY_TO_EXECUTE → REVALIDATING
      → EXECUTING → COMPLETED
```

异常：`COMPARE_FAILED`、`VALIDATION_FAILED`、`EXECUTION_FAILED`、`CANCELLED`、`ROLLED_BACK`。

禁止 `COMPARING → EXECUTE`。

## 26. UX 原则

1. **不直接改 Target**：必须先 Compare，再 Review，再 Apply。  
2. **Delete 显式开启**，执行前再确认。  
3. **始终能回答 What will change？**  
4. **SQL 是审查层，Change Set 是执行层**（V1 不可手改 SQL 执行）。  
5. **对标 Navicat 能力，不复制 Wizard。**

## 27. DataZen 差异化 UX

保留 Navicat 能力（选项、表映射、比较、脚本、执行），交互用 Diff Workspace：

```text
Source → Target → Summary → Table Diff → Row/Cell Diff
       → Change Set → SQL Preview → Sync
```

> **Navicat 的数据同步能力 + Git Diff 的审查模型 + DataZen 的现代数据库 IDE 体验。**

## 28. 安全（V1 必做）

现有覆盖拷贝绕过了 Safe Mode / `sql_guard` / 连接 `read_only`。新实现必须：

1. Target `read_only` → 禁止 Execute。  
2. 同步执行走**专用 IPC**（类似 `commit_row_updates`），不把生成 SQL 丢进普通 `execute_query`（避免 Safe Mode 误杀带 PK 的 UPDATE，或反过来绕过只读）。  
3. Delete 默认关；开启与执行两道确认。  
4. 未选中的行不得执行。  
5. 不把密码写入任务/历史。  
6. 执行 SQL 全部参数绑定；标识符走 `quote_ident`。  
7. 提示触发器 / `ON DELETE CASCADE` 可能导致实际删除多于 Change Set（V1 警告即可）。  
8. 同一库自同步禁止。

## 29. MVP 范围

### P0（对标 Navicat 主路径 + 可审查 Diff）

- 复用现有窗口/菜单入口，替换内部流程
- Source / Target / Swap
- MySQL ↔ MySQL / MariaDB
- PostgreSQL ↔ PostgreSQL
- 同名 Table 自动映射；不同名映射须结构仍完全一致
- 硬门闸：结构完全一致 + 相同 Primary Key（含复合 PK）；失败则 INCOMPATIBLE
- Insert / Update / Delete（Delete 默认关）
- Table / Row / Cell Diff（分页）
- Table / Operation / Row 选择
- 只读 SQL Preview
- Execute 前校验（连接、只读、表、PK、权限尽力而为）
- 事务 Commit / Rollback
- Cancel Compare / Cancel Execute
- Execution Result
- Apply 后再 Compare = 0 差异（验收）
- 拆除或停用旧 DROP+INSERT 引擎（产品内不可再走覆盖拷贝）

### P1

- 手工表名映射（结构+PK 门闸不变）
- Saved Sync Task / History
- 服务端 Hash 大字段
- 行级 Execute 冲突检测
- Preview SQL 导出增强
- SQLite 同类型同步（仍要求结构一致+PK）
- 连接树右键预填 Source

### P2

- 与 Structure Sync 的「先对齐结构再同步数据」向导（仍是两个产品串联，不是混成一个引擎）
- 定时 / CLI / Workflow / MCP
- 可编辑 SQL 执行

异构、字段映射、无 PK、目标建表 → **Transfer PRD**，不进 Data Sync 路线图。

## 30. 产品定位

不是：

> 把 A 数据库复制到 B 数据库。

而是：

> **在表结构完全一致且双方有相同主键的前提下，可视化比较同行数据差异，并安全地将选定差异应用到目标库。**

闭环：

```text
Source DB → Compare → Data Diff → Change Set → SQL Preview → Apply → Target DB
         → Compare again → 0 changes
```
