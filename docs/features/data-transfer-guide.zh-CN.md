# Data Transfer 用户指南

Data Transfer 用于**异构数据库、结构不一致或需要显式表/列映射的数据搬运**。它与 Data Sync、Schema Diff 是独立能力。

## 入口

- **Tools → Data Transfer…**
- 连接树右键菜单中的 **Data Transfer…**

## 何时使用

| 场景 | 工具 |
|---|---|
| 同族、结构一致、相同 PK，需要行级增量对齐 | Data Sync |
| 只需要修改数据库结构 | Schema Diff |
| MySQL → PostgreSQL 等跨方言、结构不一致、需要映射或建表 | **Data Transfer** |
| SQL ↔ Redis 等跨类别 | 不支持 |

## 向导

当前 UI 为 6 步：

1. **Endpoints** — Source/Target connection + database。
2. **Setup** — Structure/Data 模式、Write mode、batch size、错误策略。
3. **Objects** — inspect 后选择表。
4. **Mapping** — 表名和列映射；跨方言建表时可指定 target native type。
5. **Preview** — DDL 与 write plan；DDL override 可编辑；Execute 在此步骤启动。
6. **Result** — 每表执行结果和行数。

## Pairing

- `direct`：同方言族，直接 SQL 路径。
- `ir`：跨方言，通过 IR adapter。
- `unsupported`：当前 Driver pair 不支持。

## Write mode

| 模式 | 行为 |
|---|---|
| Insert | 追加写入 |
| Truncate + Insert | 先清空目标表，再写入 |
| Drop + Create + Insert | 删除目标表、按结构重建、再写入 |

破坏性模式需要显式确认，并在执行前再次确认。

## Preview 与执行

Preview 包含：

- 建表 DDL；
- write plan；
- warnings；
- block reason。

目标为 read-only、表映射不兼容、自覆盖等情况时 Execute 会被阻止。

跨方言结构通过 `src-tauri/src/transfer/` 的 IR adapter 转换，目标 DDL 由 target adapter 生成。数据值同样在 IR 路径中转换。

## 结构阶段覆盖范围

结构阶段按固定顺序生成计划：先全部建表 DDL，再全部二级索引，最后全部外键约束，保证每条二级对象语句都落在被依赖的表之后。

会迁移：

- 基表及其列映射（含重命名后的目标列名）；
- 主键：随 `CREATE TABLE` 一起生成，不会单独产生一条索引语句；
- 二级索引（唯一与非唯一），最终 DDL 由 target adapter 渲染；
- 外键：字段映射到目标列名后用 `ALTER TABLE ... ADD CONSTRAINT` 追加。

会明确拒绝（fail-closed，整份计划不执行，不会静默丢弃）：

- 计算列 / 生成列等未进入传输结构 IR 的源对象；
- CHECK 约束；
- deferrable 外键；
- 目标列不是整型、或目标 adapter 无法渲染的 identity/auto-increment 列；
- 前缀长度索引与表达式索引（例如 `name(10)`）；
- 索引或外键引用了未勾选迁移的表；
- 索引/外键列未映射或被跳过；
- 命名冲突：索引与外键名在目标命名空间内重复时要求先重命名源对象。命名空间与大小写规则由 target adapter 决定。例如 MySQL 允许不同表使用同名索引，而 PostgreSQL 的同一 schema 共享索引名称，跨表重名必须先处理。

## 当前限制

Data Transfer 当前主要面向基表。它不承诺迁移视图、函数、触发器、存储过程等完整数据库对象生态；具体支持范围由当前 Driver adapter 和 UI 能力决定。

普通传输与断点续传分别校验。对于具备完整主键和一致性快照、但不满足游标续传条件的源表（例如 MySQL BIGINT 主键或文本复合主键），普通传输在同一次快照中按主键排序、分页读取，并保持缓冲和批次大小限制；取消或失败后禁止复用该快照的分页位置续写，必须重新审阅。Cancel 针对当前 transfer job。

## 代码位置

- 传输引擎与任务管道：`packages/data-transfer/src/`
- IR / DDL：`packages/data-transfer/src/structure.rs` 与对应 Driver adapter
- IPC：`src-tauri/src/commands/data_transfer/`
- Frontend：`src/windows/data-transfer/`
