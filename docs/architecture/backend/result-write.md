# 查询结果写回（result write）实现事实

> 本文写查询结果网格把一次单元格编辑变成 DDL/DML 的完整链路：谁判定可写、写到哪里、什么条件下拒绝、失败时用户看到什么。**两条通道只有一条通**，这是全篇最重要的事实。前端网格结构与选择/键盘见 [data-table.md](../frontend/data-table.md)，表数据网格的批量提交（ChangeSet）在 `modules/src/data_browsing/`，不属本文范围。
>
> 维护规则：改判定顺序、改写入守卫、改错误文案都要同步改本文。

## 1. 结论先行

| 事实 | 代码位置 |
|---|---|
| 结果网格**只能**用窄通道写回：`commitQueryResultCell` | `src-tauri/src/commands/result_write.rs`、`src/stores/panelStore.ts` 的 `updateResultCell` |
| origin 通道（驱动自述来源 → 逐列可写 → 预览 → 提交）**被常量关闭** | `src/lib/resultEditability.ts` 的 `RESULT_ORIGIN_WRITE_BACK_ENABLED = false` |
| 来源判定 `resolve_result_origin` 仍然每次都调，但它的肯定答复**不能**授予写权限；它只能「拒绝」和「解释」 | `src-tauri/src/commands/result_origin.rs`、`src/lib/resultEditability.ts` 的 `resolveResultWriteBack` |
| 「写到的还是不是原来那张表」由**写入前**的关系重比保证，不由 `WriteTarget` 保证 | `src-tauri/src/commands/result_write.rs`、`modules/src/table_editing/result_edit/mod.rs` 的 `WriteTarget` 文档注释 |
| 一次执行只能改**一格**；改完必须重跑查询才能再改 | `src/stores/panelStore.ts` 的 `updateResultCell` 末尾 `runtimeBinding: null` |
| 判定与写入**都没有**「批量」形态：请求里的 `row_count` 必须是 1 | `modules/src/table_editing/result_edit/mod.rs` 的 `validate_result_cell_request` |

## 2. 两条通道

### 2.1 窄通道（生效）

窄通道的输入只有「关系 + 行标识 + 列 + 新值」，全部由宿主侧的映射探测得出：

1. 执行语句时，宿主随执行做一次映射探测（`mapping_probe`）；`verified_source` 拿这份探测结果与本次执行的观测上下文比对。
2. 探测成功且**逐列对齐**时（`verify_result_mapping`，`src-tauri/src/commands/result_mapping.rs`），`verified_source(...)` 返回 `StatementResultSource`：`execution_id`、`statement_index`、`context`、`relation`、`writable_mapping: Verified`。这条 source 随语句元数据一起发给前端。
3. 前端把 `writableMapping === 'verified'` 与 `runtimeBinding` 一起当作「这一页可能能写」。两者缺一即只读。
4. 用户改一格 → `updateResultCell` → `commitQueryResultCell({ binding, statementIndex, rowIndex, column, value })`。
5. 宿主重新走一遍判定（不信任前端的缓存判定），拼一条带主键 `WHERE` 的 `UPDATE`，执行，校验受影响行数。

窄通道的**形状限制**来自第 2 步的逐列对齐：结果列必须是来源表列的直接投影（列数一致、列名一致、列名唯一、主键列在结果里、库/表名非空）。`SELECT id, upper(name) FROM t` 这类投影拿不到 `Verified`，于是永远只读——不是因为权限，而是因为「这一列是从哪来的」无法确定。

### 2.2 origin 通道（关闭）

origin 通道是驱动自己回答「这些行来自哪里、哪些列是基表列的直接拷贝」，允许把别名、投影列也纳入写回。它由三段组成：

| 段 | 命令 | 状态 |
|---|---|---|
| 判定 | `resolve_result_origin` | **注册并调用**，每次渲染结果都要答案 |
| 预览 | `preview_result_cell_change` | **注册、前端有封装**，但没有任何界面调用 |
| 提交 | 无独立命令；设计上经 `previewResultCellChange` → `commitPendingChanges` | **未接** |

关闭的理由写在 `RESULT_ORIGIN_WRITE_BACK_ENABLED` 的文档注释里，是一条可核对的因果链：唯一的写入实现是窄通道，而窄通道会用自己的映射检查再验一次，恰好会拒绝 origin 判定所允许的那些语句。若提前打开通道，界面会把单元格画成可编辑，写入却失败，**失败还可能不显眼**——用户以为改了，库里没改。这比灰掉单元格更糟。

因此该常量是「有名开关，只有一个含义」：只有当**两半都存在**（驱动 override 了 `describe_result_origin`，且前端改走新流水线）才允许打开。测试固定它为 `false`，同时固定打开后不可达的那条分支。

打开它会同时激活两件事，缺一不可：`originChannelWritable(server)` 开始返回 `server.writable`，`isColumnEditable(server, column)` 的列级守卫开始生效（列不在 `editableColumns` 里就不能编辑，否则后端会用 `grid.result.writeDenied` 拒绝，而界面已经让用户敲了字）。

### 2.3 两个判定如何合并

`resolveResultWriteBack(legacyVerified, server)` 的规则很短，顺序不能换：

1. **只读连接优先**：`server.reason === 'sessionReadOnly'` → 直接 `false`。窄通道的 `Verified` 只说「这页是那张表的直接拷贝」，完全没说连接能不能写，所以它压不过只读连接。
2. `legacyVerified`（窄映射 + 活绑定）→ `true`。
3. 否则 `originChannelWritable(server)` → 当前恒 `false`。

其余所有拒绝理由都保持「origin 判定只能拒绝、不能授予」：通道关闭时 `server.writable` 为假，前端就按 `server.reason` 显示原因；驱动答不出时落到 `unknownOrigin`，而不是「否」——「我不知道」不能变成「不行」。

页面级事实（主键为 `NULL`、页内行标识重复、行已不在此结果）**先算、且独立于通道**：它们只能拒绝，永远不会把一个被拒的结果变成可写。这也是它们「结构上不可达」不成立的原因——即使结果只读，这些行级原因也会被算出来。

## 3. 服务端判定：`evaluate_result_editability`

判定本身在共享层 `modules/src/table_editing/result_edit/mod.rs`，命令壳只做三件事：把会话和 catalog 的答案借给规则、校验请求信封、把共享错误映射成 `CommandError`。

`GridReadOnlyReason` 共 11 种，`read_only_error_code` 把它们映射成 5 个 `ResultErrorCode`：

| 原因 | 触发 |
|---|---|
| `unknownOrigin` | 驱动答不出来（含命令失败时的默认落点） |
| `ambiguousOrigin` | 驱动报告的列来源互相矛盾 |
| `multiTable` | 结果跨多表 |
| `aggregate` | 聚合结果 |
| `distinct` | `DISTINCT` |
| `setOperation` | 集合运算（`UNION` 等） |
| `derivedSource` | 子查询/派生表 |
| `view` | 来源是视图 |
| `noPrimaryKey` | 来源表没有主键 |
| `keyNotSelected` | 结果没包含全部主键列 |
| `sessionReadOnly` | 连接只读 |

`read_only_error_code` 的分组是**刻意的**：`unknownOrigin` → `grid.result.originUnavailable`，`ambiguousOrigin` → `grid.result.originAmbiguous`，其余九种 → `grid.result.writeDenied`。所以「只读」在协议层是三个码，不是一个。

另外两个码不属于「只读原因」，而属于**请求形状**（`validate_result_cell_request`，在拼任何 SQL 之前跑）：`row_count != 1` → `grid.result.multiRowNotAllowed`；行标识为空、或任一主键值为 `NULL` → `grid.result.rowNotAddressable`（错误信息里会点名列名）。这五个码合起来是 `ResultErrorCode` 的全部，前端按前缀分类，不认识的前缀保留原文。

`locating_key_columns` 来自 catalog 的主键列表，别名由 origin 补齐；**不从列标志重新推导**。这是「行怎么定位」的唯一来源。

## 4. 写入守卫的实际顺序

`commit_query_result_cell` 是**一个函数的顺序**，顺序本身是契约（前面的便宜、后面的贵；前面失败就不该走到后面）：

| 顺序 | 检查 | 失败信息 |
|---|---|---|
| 1 | 执行存在、状态为 `Succeeded`、`runtime_binding` 与请求的 `binding` 完全相同 | `Result requires its original live session` |
| 2 | 连接配置 `read_only` | `grid.result.writeDenied: Connection is read-only; result cells cannot be written back` |
| 3 | profile 仍启用、`config_revision` 与开会话时一致 | `Result connection configuration changed` |
| 4 | 能在这份执行产物里找到 `statement_index` + `WritableMapping::Verified` + `kind = statementMetadata` 的块 | `Result has no verified writable source` |
| 5 | 该块带 `relation` | `Result relation unavailable` |
| 6 | 会话连接 = 关系的连接，且 `WriteTarget::of(观测上下文) == WriteTarget::of(source.context)` | `Result session context changed; execute the query again before editing` |
| 7 | 结果列可读 | `Result columns unavailable` / `Invalid result columns` |
| 8 | 请求的 `row_index` 在这份结果的范围内、行可读 | `Result row unavailable` / `Invalid result row` |
| 9 | 关系带表名 | `Result relation unavailable` |
| 10 | 用 `get_table_schema` 重新读来源关系，`verify_result_mapping(...)` 的结果必须**等于**冻结的 `relation` | `Result schema unavailable` / `Driver cannot confirm result identity` / `Result relation changed; execute the query again` |
| 11 | 被改列不是主键列 | `Primary key edits require the table ChangeSet editor` |
| 12 | 被改列在本结果的列里 | `Result column unavailable` |
| 13 | 所有主键列都能在结果里找到、且值非 `NULL` | `Result primary key unavailable` / `Null result primary key cannot be edited` |
| 14 | 驱动可用、且能按冻结的库/schema 限定语句 | `SQL driver unavailable` / `Driver cannot qualify result writes` |
| 15 | 执行 `UPDATE`，`affected == Some(1)` | 见 §5 |

第 11 条值得单独记：**主键不能在这里改**。改主键要走表数据的 ChangeSet 编辑器（那里有完整的 `INSERT/UPDATE/DELETE` 计划与指纹校验），否则一次「改主键」会变成「删除一行 + 插入一行」的错觉，而行标识在 `WHERE` 里已经用过一次。

## 5. 失败分支与用户看到什么

| 情况 | 结果 | 文案（原文） |
|---|---|---|
| `affected == Some(0)`，回读确认值已等于目标值 | **成功** | 无（这是 MySQL「changed rows」语义：并发写者已经写成了同一个值） |
| `affected == Some(0)`，回读不到那一行或值不同 | 失败 | `Result row changed or was deleted; inspect the target before retrying` |
| `affected == Some(1)` | 成功 | — |
| 其他任意受影响行数（含 `None`：驱动没报） | 失败 | `Result edit reported {:?} affected rows. Inspect the target and current transaction before retrying.` |

`affected == 0` 不是「没改」的同义词：MySQL 报告的是**改动行数**，把值设成它已经是的值就是 0。所以必须先回读再判定，回读 SQL 用同一个绑定的资源执行（`SELECT <列> FROM <表> WHERE <主键> = ?`），值相等才算成功。把 0 一律当失败，会让「并发写者已经写成同值」变成一条假错误。

第 1 步与第 6 步的文案都不是 `grid.*` 错误码：它们是普通校验文本，前端原样显示。区别在于「用户能做什么」——有错误码的（只读、无来源）可以按原因渲染面板，没错误码的只能照抄一句话。

## 6. `WriteTarget`：为什么只有两个字段

```rust
pub struct WriteTarget {
    pub namespace: NamespaceTarget,  // database / catalog / schema / path
    pub identity: Option<String>,    // 有效身份；None 与空串等价
}
```

一个会话上下文里混着两类事实，`WriteTarget` 只留第一类：

- **写去哪、以谁的身份写** → `namespace` + `identity`。这两项决定「保留的结果是不是还指着它的目标」。
- **会话当前怎么跑** → 事务状态、`autocommit`、`search_path`。这些在语句执行**期间**就会变（`BEGIN`、隐式事务、`SET search_path`），它们不描述目标。

`confidence`（`Confirmed`/`Partial`/…）同样**不在**目标里：它回答「我们知道多少」，不回答「写到哪」。而且两者不同向——`Partial` 不代表 namespace 不完整（PostgreSQL 事务回滚后仍保留最后观测到的 namespace，SQLite 读完 pragma 也会把整次观测标成 `Partial`）。所以 `WriteTarget` 不拒绝这些写入，也不是防重定向的守卫。

两侧都走 `WriteTarget::of(context)`，使「是不是同一个目标」只有一个表示、一处比较。空串按「未上报」处理并归一为 `None`（`None` 与 `""` 是同一个事实，两种投影拼法不同）。

## 7. 「目标没被改」的真正保险：写入前的关系重比

顺序是（守卫表第 6、10、14 条）：

1. 用**冻结的** `relation` 里的库/schema 去读 `get_table_schema`（参数是 `{ table, database, schema }`，全部来自 `relation`，不是来自请求）。
2. 重新跑 `verify_result_mapping(connection_id, 结果列, 新读到的 schema, resolvedTarget)`，要求它返回的 `Some(relation)` 与冻结的 `relation` **相等**。列数、列名、主键存在性、库/表名非空都在这一步重验。
3. `UPDATE` 的语句与它的库/schema 限定都用冻结的 `relation`（`qualify_sql_target`）。

`WriteTarget` 的文档注释明确记录了这条重比的必要性：删掉它，被改向的写入会**真的落地**，该结论由变异验证得出。所以「改写目标」这件事要防的是关系漂移，不是上下文差异——文字写在注释里，也是这条链路唯一必须保留的守卫。

第 6 条仍然有用，但它的职责要说得更窄：它保证「写还是发生在原来那个目标、以原来那个身份」，挡的是重连/切库/换身份；至于表被重建、视图替换、同名列易主这类漂移，只有第 10 条的 schema 重比能看出来。

## 8. 能力探测不是前置条件

`verified_source(...)`（`src-tauri/src/platform/session_backend/execution.rs`）决定**这一条语句的结果能不能带着来源信息发给前端**。它是探测，不是前置条件：任何一条失败路径都返回 `None` 并写一条 debug 日志，**绝不中断执行**。

失败路径逐一（顺序即代码顺序）：

1. `statement_results.len() != 1` —— 批处理里的语句没有单一来源。
2. `context_before.confidence != Confirmed` —— 上下文没被驱动确认。
3. 没有映射探测结果。
4. 探测时的观测上下文与本次执行的 `context_before` 不相等 —— 会话在探测与执行之间动过，探测结论不适用于这些行。
5. 驱动没同时给出 `schema` 与 `resolvedTarget`。
6. `schema` 读不成 `TableSchema`。
7. `verify_result_mapping` 返回 `None`。
8. 序列化成 `StatementResultSource` 失败（「不可编码的来源等于没有来源，但仍不能因此让用户拿不到行」）。

前端的对应事实：`source?.writableMapping === 'verified' && runtimeBinding != null` 才认为可能可写（`ResultTableView` 的 `legacyVerified`）。少任何一个，网格是只读的，且原因落到 `unknownOrigin`——「探测没答案」和「来源未知」在用户看来是同一件事，文案也就用同一句。

## 9. 一次执行只能改一格

写入成功后 `updateResultCell` 做的事有三件：把本地那一格改成新值、把该结果的 `runtimeBinding` 置 `null`、其余不动。

`runtimeBinding` 为 `null` 之后，同一个结果的第二次编辑在**更早一层**就被 `updateResultCell` 自己的检查拒绝：`Result is read-only; execute the query again`（它根本不会发出 IPC）。注释写明理由是「保留的产物是不可变的」——那份执行产物仍在前端手里，但已经不再是可写目标的活绑定；重放原始 SQL 会得到一个可能指向不同数据的窗口，所以宁可要求用户重跑。

这条限制是**产品级**的，不是缺陷：结果网格是 SQL 执行结果的副本视图，不是可反复编辑的表编辑器。要连续编辑应当打开表数据标签页（那里有暂存、预览、指纹与批量提交）。

## 10. 与表数据通道的边界

两条写路径共享 `driver.build_update_statement` / `qualify_sql_target` 这类驱动能力，但**计划层完全不同**：

| | 结果网格（窄通道） | 表数据网格 |
|---|---|---|
| 一次写多少 | 1 行 1 列（`row_count != 1` → `grid.result.multiRowNotAllowed`） | 一批：新增 → 更新 → 删除，同一事务 |
| 计划与预览 | 无用户可见计划；服务端即时拼 `UPDATE` | `RowChangePlan` + `fingerprint` + `previewPendingChanges` |
| 陈旧保护 | `runtime_binding` + 会话上下文 + 写入前 schema 重比 | `grid.commit.stalePlan`（会话/指纹）、`conflictingIntents`、`affectedMismatch` |
| 行标识 | 主键等值 `WHERE`，主键值必须非 `NULL` | 行标识锚点 + 原始值，可定位复合标识 |
| 主键编辑 | 拒绝 | 走 ChangeSet |

不要试图把结果网格的编辑改成走 ChangeSet：结果的行标识来自「这一页显示了什么」，没有原始值快照，也没有事务范围，拼不出可校验的计划。反过来也不要让表数据网格走窄通道——那会丢掉批量事务与预览。

## 11. 契约级限制清单

- **没有批量写**：`validate_result_cell_request` 对 `row_count != 1` 直接拒绝；`build_result_cell_change_plan` 只产出单格计划。
- **没有「撤销」**：写入即落地（或落在用户已开启的显式事务里）。前端没有反向语句生成器。
- **没有 RETURNING**：结果写回是 `UPDATE`，不涉及回读新行；新行定位属于表数据网格的插入路径。
- **主键不可改**（§4 第 11 条）。
- **只读连接、配置漂移、会话漂移、关系漂移四类拒绝**各自有独立文案，不要合并成一句「不能编辑」——用户对不同原因的处理方式不同（换连接 / 重连 / 重跑查询 / 刷新表结构）。
