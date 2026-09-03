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

## 当前限制

Data Transfer 当前主要面向基表。它不承诺迁移视图、函数、触发器、存储过程等完整数据库对象生态；具体支持范围由当前 Driver adapter 和 UI 能力决定。

当前没有表级断点续传；Cancel 针对当前 transfer job。

## 代码位置

- Backend：`src-tauri/src/data_transfer/`
- IR / DDL：`src-tauri/src/transfer/`
- IPC：`src-tauri/src/commands/data_transfer/`
- Frontend：`src/windows/data-transfer/`
