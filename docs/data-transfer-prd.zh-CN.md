# DataZen 数据传输 / 迁移 PRD

**产品：** DataZen  
**功能：** Data Transfer / Data Migration / 数据传输  
**版本：** V0.1  
**状态：** Draft（**本次不实施**；先定概念与边界）  
**参考：** Navicat Data Transfer  
**相关文档：** [Data Synchronization](./data-synchronization-prd.zh-CN.md) · [Schema Diff / 结构同步](./schema-diff-guide.md)

## 0. 为什么单独写这篇

Data Synchronization **只**在「同类型、表结构完全一致、双方有相同 PK」时做增量 Diff。

迁库、异构、改列名、目标表还不存在、没有主键——这些是 **Data Transfer**。不能塞进 Sync，否则会再次做成「DROP + 灌数」却挂着同步的名字。

仓库里现有 Data Sync 的 `DROP TABLE → CREATE → INSERT` + 跨库 IR，本质上是**没有映射 UI 的简陋 Transfer**。正式做 Transfer 时：吸收可用的 IR 适配器，**不要**从 Sync 引擎长出来。

本文对标 Navicat Data Transfer，给出产品定义与 V1 草案，供后续立项。实施前再出独立实施方案。

---

## 1. 与 Sync / Structure Sync 的差异

| | Data Transfer（本文） | Data Synchronization | Structure Synchronization |
|---|---|---|---|
| 动作 | **单向拷贝**（Source → Target） | 比较后把选定差异应用到 Target | 比较后把 DDL 应用到 Target |
| 是否 Compare 行 | 否（最多预览抽样） | 是（全量/分块 Diff） | 否（比结构） |
| 异构库 | **允许**（MySQL → PG 等） | 不允许 | Schema Diff 已部分支持 |
| 表结构 | **允许不同** | 必须完全一致 | 本来就不同 |
| 主键 | 不要求 | **必须相同 PK** | 不要求 |
| 目标表 | 可不存在：先建再灌 | 必须已存在 | 表通常已存在 |
| 用户必给 | 对象选择；结构不同时给 **字段映射 + 类型映射** | Insert/Update/Delete 选项 | 是否允许破坏性 DDL |
| 删除 Target 多出行 | 默认不删（是覆盖/追加，不是双向对齐） | 可选 Delete | 列/索引 DROP 另算 |
| 成功标准 | 选定对象已出现在 Target，行数/校验符合选项 | 再 Compare = 0 行差异 | 再比结构符合计划 |

```text
Transfer     = 搬家（可以换房型，你要画好房间对应关系）
Sync         = 两套已经一样的房子，对齐里面的东西
Structure    = 先把房子改成一样
```

常见串联（未来，不是本次 Sync 范围）：

```text
1. Structure Sync    把列/索引对齐
2. Data Sync         再补数据差
```

或：

```text
1. Data Transfer     异构迁过去（映射 + 建表 + 灌数）
2. 之后若结构已一致且有 PK → 才改用 Data Sync 日常对齐
```

---

## 2. 产品目标

对标 Navicat Data Transfer，用户可以：

1. 选择 Source / Target（**允许不同数据库类型**）
2. 选择传输内容：仅结构 / 仅数据 / 结构+数据
3. 选择对象（表；P1 视图等）
4. 目标表不存在时从 Source 生成并创建
5. 结构不同时，配置 **表映射、字段映射、数据类型映射**
6. 预览将执行的 DDL + 数据写入方式
7. 分批写入 Target，可取消，有进度
8. 查看结果（成功/失败对象、行数、错误）

不要求 Apply 后再做行级 Diff = 0（那是 Sync 的验收）。Transfer 验收是：选定映射下，Source 行按规则出现在 Target。

## 3. 非目标（Transfer V1 草案仍不做）

- 增量 Diff / Change Set / 按 PK 的 UPDATE·DELETE 对齐（那是 Sync）
- CDC、定时、双向复制
- 复杂 ETL 编排、Workflow 替代品
- 自动猜列名（可提供同名自动匹配，**不**靠模糊相似度静默映射）
- 无用户确认的破坏性 DROP

## 4. 核心概念

```text
Transfer Job
├── Source / Target          （允许异构）
├── Mode                     structure | data | structure+data
├── Object Selection
├── Table Mapping            source_table → target_table | create_new
├── Column Mapping           source_col → target_col | skip
├── Type Mapping             source_type → target_type + 转换规则
├── Write Mode               insert | replace / truncate+insert（显式）
├── Preview
└── Execution Result
```

## 5. 用户流程（对标 Navicat）

```text
选择 Source / Target（可异构）
        ↓
选择 Mode：结构 / 数据 / 两者
        ↓
选择对象
        ↓
表映射（同名自动；可改目标表名；可「在目标新建」）
        ↓
若目标表已存在且结构不同
        → 字段映射 + 类型映射（必填未自动匹配的列）
        ↓
选项（事务、批大小、遇错继续、是否先清空目标表……）
        ↓
Preview（将创建的 DDL、映射摘要、预计写入方式）
        ↓
Execute
        ↓
Result
```

Wizard 步进可以比 Sync 更重（Navicat Transfer 本身就是向导）。不要做成 Sync 的 Diff Workspace。

## 6. 功能入口（实施时）

建议**新窗口 kind**（如 `data-transfer`），不要塞进 Data Sync 窗口。

- Tools → Data Transfer…
- 连接/库/表右键 → Data Transfer…（原生 Menu，预填 Source）
- 从 Data Sync 的 INCOMPATIBLE 提示跳转：「结构不一致或无 PK，改用 Transfer」

## 7. Source / Target

- 允许不同 Driver Type。
- 不支持的异构对（SQL ↔ Redis 等）在 pairing 层拒绝，说明原因。
- Target `read_only` → 禁止 Execute。
- 同一连接同一库同一表作为目标「覆盖自己」→ 禁止或极强确认。

## 8. Mode

| Mode | 行为 |
|---|---|
| Structure only | 按映射在 Target 创建/补齐表结构（DDL），不灌行 |
| Data only | 目标表必须已存在；按字段映射 INSERT（或所选 Write Mode） |
| Structure + Data | 先结构后数据 |

Structure 部分可复用 Schema Diff / `packages/driver-api` IR 适配器，但 Transfer 的「建表」与 Schema Diff 的「把已有表 ALTER 成期望态」仍是两条路径：

- Transfer 建表：Source → IR → Target `CREATE TABLE`
- Schema Diff：已有 Target 表 → ALTER 计划

## 9. 表映射

```text
source.users      → target.users          （已存在）
source.customers  → target.clients        （已存在，不同名）
source.orders     → (create) orders       （目标新建）
source.legacy     → skip
```

新建时：用类型映射生成 Target DDL，用户可 Preview 后执行。

## 10. 字段映射（结构不同时必填）

目标表已存在且列不完全一致时，**必须**给出列映射，禁止静默丢列或错位灌数。

```text
Source          Target         规则
id           →  id             同名自动
user_name    →  name           用户指定
email        →  email
age          →  (skip)         不传输
(无)         →  created_at     用 Target 默认值 / 表达式 / 报错必填
```

规则：

- 同名且类型可映射 → 自动配对，用户可改
- Target `NOT NULL` 且无默认、Source 又 skip → 阻止 Execute，列出列
- 不自动靠编辑距离猜测 `user_name`→`username`（可提示候选，必须用户确认）

## 11. 数据类型映射

异构或同构但类型名不同时，每条用到的 Source 类型要落到 Target 类型。

```text
默认（可改）：
  MySQL INT        → PG INTEGER
  MySQL DATETIME   → PG TIMESTAMP
  MySQL TINYINT(1) → PG BOOLEAN     （用户可改为 SMALLINT）
  MySQL TEXT       → PG TEXT
  无法默认         → 用户必选，否则该列不可传输
```

默认表来自 Driver IR（现有 `column_to_ir` / `ir_type_to_native`），**用户覆盖优先**。

转换失败策略（任务级选项）：

```text
● 失败则该行报错（默认）
○ 写成 NULL（仅当目标可空）
○ 截断（仅字符串长度，需确认）
```

BLOB / 编码 / 时区：映射规则里写明（如 DATETIME 按 UTC 还是按 Source session）。

## 12. Write Mode（仅数据）

| 模式 | 说明 | 风险 |
|---|---|---|
| Insert | 逐行/批量 INSERT | 主键冲突则失败或跳过（选项） |
| Truncate + Insert | 先清空目标表再插入 | **破坏性**，两道确认 |
| Drop + Create + Insert | 重建目标表再插入 | **破坏性**；接近当前旧引擎，仅 Transfer 且用户显式选择 |

默认 **Insert**。禁止 Sync 使用 Drop+Create。

主键冲突：

```text
○ Stop
○ Skip row
○ 本次不做 Upsert（Upsert = Sync 的 Update，避免两套语义）
```

## 13. Preview

执行前只读预览：

- 将执行的 `CREATE TABLE` / 所选破坏性语句
- 表映射 + 字段映射 + 类型映射摘要
- 预计批大小、是否事务
- 抽样 20 行「映射后的目标行」（可选）

不生成 Sync 那种全量 INSERT/UPDATE/DELETE Change Set。

## 14. 执行与安全

- 分批写入（默认 500–1000 行），`query_stream` 读 Source
- 支持 Cancel；有事务则 Rollback，无事务则 Partial + 明确状态
- 破坏性模式（Truncate / Drop）确认词或二次对话框
- 不保存密码；映射配置可保存（无密钥）
- 专用 IPC，尊重 `read_only`
- 进度：对象级 + 行级

## 15. 与现有代码

| 现有 | Transfer 中的角色 |
|---|---|
| `packages/driver-api/src/sync/` IR 适配器 | **类型默认映射**、跨方言 CREATE |
| `commands/sync/table_sync.rs` DROP+INSERT | 仅作为 Write Mode 之一的参考实现；正式 Transfer 要加映射层后才能用 |
| Schema Diff Deploy | 结构对齐用它；Transfer 建新表用 CREATE，不替代 ALTER 部署 |
| Data Sync 新引擎 | **不共用** Change Set / PK Diff |

## 16. 数据模型（草案）

```text
TransferJob
├── source / target          Endpoint（可异构）
├── mode                     structure | data | both
├── write_mode               insert | truncate_insert | drop_create_insert
├── objects[]
│   ├── source_table
│   ├── target_table         或 create_new
│   ├── column_maps[]        { from, to, skip, type_override, on_error }
│   └── enabled
├── type_map_overrides[]     { source_type, target_type }
└── options                  batch_size, stop_on_error, ...
```

## 17. MVP 草案（实施 Transfer 时再排期）

### P0

- 同构或异构 SQL：MySQL/MariaDB ↔ PostgreSQL 至少一条路径跑通
- Structure + Data 与 Data only
- 目标新建表（IR → CREATE）
- 同名字段自动映射；其余必须手绘
- 默认类型映射 + 用户覆盖
- Insert 写入；Truncate/Drop 显式选项
- Preview + 进度 + Cancel
- read_only 拒绝 Execute

### P1

- 保存 Job、重复执行
- 更多 Driver 对
- Query 结果集作为 Source（`SELECT` → 目标表）
- 遇错跳过行
- 抽样预览映射后的行

### P2

- 视图等对象
- 表达式转换（`lower(email)`）
- CLI / 定时
- Mongo/Redis 独立模型（不强行套表）

## 18. 验收（未来实施时）

1. 异构：Source 与 Target 不同类型时可完成一次 Structure+Data。
2. 结构不同：未完成的字段映射不能 Execute。
3. 类型无法默认映射时，用户指定前不能 Execute。
4. 新建表：Target 原先没有该表，完成后表存在且行数符合 Source（在 skip/错误策略下可核对）。
5. 不做 PK Diff，也不把 Transfer 成功说成「已同步」。
6. 从 Sync 的 INCOMPATIBLE 能理解「应改用 Transfer」。

## 19. 本次工程的含义

- **现在只实现 Data Synchronization**（另一份 PRD + 实施方案）。
- Transfer 不排进当前开发任务，但产品文案、INCOMPATIBLE 提示、旧引擎拆除说明里要指向本文，避免再把覆盖拷贝叫成同步。
