# DataZen 架构设计（十三）：Schema Diff、Data Sync 与 Transfer 的边界

> “同步数据库”是一个容易误导的产品词。只改结构、比较同族数据、把异构数据搬过去，前置条件、风险和执行模型都不同。DataZen 将它们拆成 Structure Sync、Data Synchronization 和 Data Transfer。

## 三个产品问题

| 能力 | 目标 | 典型约束 |
| --- | --- | --- |
| Structure Sync / Schema Diff | 只改变 DDL | 比较结构，生成并审核 DDL Plan |
| Data Synchronization | 同族数据库保持数据一致 | 结构与 PK 完全一致，Compare → Review → Execute |
| Data Transfer | 异构或单向搬运 | 通过 IR 转换，允许列映射和目标重建 |

把三者合成一个“复制”按钮，会让用户误以为可以安全地跨数据库覆盖数据，也会让代码共享错误的门控和事务假设。

## Data Synchronization 的硬门闸

V1 的 Data Sync 只允许同族 MySQL/MariaDB 或 PostgreSQL，并要求：

1. 源和目标 Driver family 相同；
2. 列名、类型、可空性一致；
3. 主键集合与顺序一致；
4. 不能对同一连接、database、schema 自同步；
5. 源/目标数据库必须明确选择，不能只选连接。

如果检查失败，产品应停留在 inspect/compare 阶段，而不是退化成 DROP + INSERT 覆盖拷贝。

## Compare 如何处理大表

`compare_data_sync` 使用 keyset 分页读取双方数据：根据主键生成 `(pk...) > (?) ORDER BY pk LIMIT batch_size`，Host 通过 `compare_table_pages` 做有序合并，产生 INSERT、UPDATE、DELETE 和 UNCHANGED。

keyset 不依赖越来越昂贵的 offset，且能在 `jobId` 取消标志下停止长比较。小表和单测可以使用内存 RowPageSource，但生产路径保持分页流式处理。

## ChangeSet 与执行

用户审核后，只有勾选且被 SyncOptions 允许的行进入 ChangeSet；DELETE 默认不选。SQL 层生成参数化 INSERT/UPDATE/DELETE，Preview 可以展示字面量，但真正执行不能把展示 SQL 当成执行 SQL。

Apply 阶段使用专用执行通道，遵循目标连接的 read_only、事务和取消策略。执行完可以重新 Compare，直到差异为零，形成可验证闭环。

## Structure Sync 的 DDL 计划

Schema Diff 比较表、列、索引、约束等结构，生成可审核的 DDL Plan。用户确认后再部署；它不读取或复制行数据，也不应该复用 Data Sync 的 ChangeSet。

DDL 具有不可逆风险，计划必须显示新增、修改和删除，执行前再次校验源/目标和结构版本，失败时保留已执行步骤和错误。

## Transfer 与中间表示

异构传输需要处理类型、标识符、NULL、默认值和目标方言差异。`src-tauri/src/transfer/` 使用 IR 适配器把源数据和结构映射到中间模型，再生成目标 DDL/DML。

Transfer 是单向搬运或转换，不承诺同族同步的“差异归零”语义，也不应被拿来绕过 Data Sync 的同族门闸。

## 按 database，而不是按 connection

同一个连接配置可能暴露多个 database。Data Sync IPC 接受 `source_database` 和 `target_database`，在查询前通过 Driver 内部切库；宿主不再调用独立 `use_database` IPC 作为隐式前置步骤。

这让比较和执行的目标成为请求的一部分，减少跨 IPC 竞态，也避免把连接默认 database 误当成用户当前选择。

## 结语

Structure Sync 改结构，Data Synchronization 对齐同族数据，Data Transfer 处理异构搬运。三者可以共享连接和 Driver，却必须保留不同的门闸、ChangeSet、IR 和执行风险。下一篇将继续看这些功能共同依赖的基础设施：Schema 缓存、加密持久化和本地安全边界。

相关资料：[Data Sync 架构](../architecture/backend/data-sync.md) · [Schema Diff](../architecture/backend/schema-diff.md) · [Transfer 用户指南](../features/data-transfer-guide.zh-CN.md)
