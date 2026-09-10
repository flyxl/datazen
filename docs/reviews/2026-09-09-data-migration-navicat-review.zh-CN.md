# DataZen 数据迁移三件套：源码审查与 Navicat 对标实施方案

审查日期：2026-09-09。代码基线：`0f1941ac8cef6abdd959cecf0a2a9155ada1975f`，开始审查时工作区干净。范围：Schema Diff、Data Transfer、Data Sync 的 React 窗口、IPC、领域执行器、Driver API，以及 MySQL/PostgreSQL 相关驱动实现。对标基线为 **Navicat 17 macOS 官方手册中的迁移工具**；数据库和平台不同，选项并不完全相同。

本次完成源码审查、官方文档核对、现有前端测试和抽取纯函数的小型 Rust 复现。**未连接真实数据库执行迁移，未做桌面黑盒或性能压测，也未修改产品实现。** 下文“已复现”只指小型复现，不代表真实数据库 E2E 已复现。

## 结论

**三件套职责划分合理，基础工作流已成形，但当前应先修复写入正确性，再扩展功能。** 最大差距集中在对象保真、可证明的执行计划、完整审阅体验、长任务控制与配置复用，而不只是界面选项数量。

- Schema Diff：已有列、主键、索引、新表创建、风险分级、DDL 预览与事务部署；尚不足以承担完整数据库结构同步。固定分组排序会破坏主键/索引替换顺序。
- Data Transfer：已有同族/异构、三种写入模式、列映射、类型覆盖和 DDL 编辑；列取值、二进制写入、映射与建表一致性存在明显缺陷。完整对象搬迁、过滤、事务策略和 SQL 文件目标尚未形成闭环。
- Data Sync：已有逐行差异、勾选、参数化 DML、事务、keyset 读取；失败回退可能扩大写入范围，物理列顺序变化可能写错值，比较结果仍全量驻留内存。

**建议发布前阻断项：F01–F04。** 本报告的“阻断”是产品发布优先级，不是宣称这些问题在任何输入下都会触发。

## 1. 实现功能与 Navicat 的差距

### 1.1 Schema Diff / Structure Synchronization

**已有能力，应保留并完善：** source 为期望状态，target 为执行位置；比较列属性、PK、普通/唯一索引；缺失目标表可产生 `CreateTable`；默认过滤破坏性操作；Driver 提供渲染与能力；可导出 SQL、导入/导出配置；展示 rollback completeness。

实现证据：[IR 生成](/Users/flyxl/code/datazen/src-tauri/src/schema_diff/ir.rs:7)、[计划构建](/Users/flyxl/code/datazen/src-tauri/src/schema_diff/plan.rs:151)、[部署](/Users/flyxl/code/datazen/src-tauri/src/schema_diff/deploy.rs:163)。

**对标差距：**

1. **对象范围。** Navicat 可按数据库能力比较视图、函数、触发器、事件、序列等；DataZen 当前 `MigrationOperation` 主要覆盖表、列、PK、索引，缺少上述对象及 DROP TABLE 的完整表达。目标独有表也未形成可部署删除计划。
2. **约束和属性保真。** FK、CHECK、分区、字符集/排序规则、owner/definer 等没有进入完整比较模型；已有 `TableSchema.foreign_keys` 也不等于 Schema Diff 已使用它。Navicat 对这些提供按驱动适用的比较选项。[官方比较选项](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/structure_sync_1.html)
3. **审阅粒度。** Navicat 按对象类型/操作分组，允许展开对象、勾选细项，并展示两端 DDL 差异与部署脚本。DataZen 以表选择、结构属性差异及生成计划为主，尚缺统一的对象/操作树和依赖联动勾选。[官方结构审阅](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/structure_sync_2.html)
4. **脚本交付。** 已有导出不应重复建设；应补“在目标连接的查询编辑器打开”、依赖约束下重排、部署日志和执行后复比。Navicat 手册明确提供重排与打开查询编辑器；不应把它夸大为“任意编辑后仍自动保证安全”。[官方部署脚本](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/structure_sync_3.html)

**DataZen 的产品价值：** 默认保守、风险分级、回滚完整性提示是值得强化的方向。但回滚 DDL 不等于恢复被删数据；“有 rollback SQL”不能显示为“数据可完整恢复”。

### 1.2 Data Transfer

**已有能力：** Structure / Data / StructureAndData，Insert / TruncateInsert / DropCreateInsert；表和列映射、目标类型覆盖、DDL override；目标只读检查、破坏性确认、批量行数、遇错停止选项、任务取消标记。

实现证据：[模型](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/model.rs:24)、[执行入口](/Users/flyxl/code/datazen/src-tauri/src/commands/data_transfer/exec.rs:48)、[预览](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/preview.rs:27)。

**对标差距：**

1. **完整对象复制。** 目前主要搬基表；通用 `IRTable` 只有列、PK、table options，通用 CREATE 没有恢复普通索引、FK、CHECK 和程序对象。列注释被输出为 SQL 注释，不代表写入数据库的注释元数据。同族搬迁也不应默认经过会丢信息的最小 IR。[当前 IR](/Users/flyxl/code/datazen/packages/driver-api/src/sync/ir.rs:84)、[当前 DDL](/Users/flyxl/code/datazen/src-tauri/src/transfer/ddl.rs:43)
2. **筛选与分块。** Navicat 支持自定义字段、记录过滤、recordset generator、每块事务，以及将视图结果搬成表。DataZen 有列选择和 batch size，但缺少行范围/条件和可管理的分块模型。[官方对象与记录集](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_transfer_2.html)
3. **写入与事务控制。** Navicat 有按数据库适用的事务、锁表、语句字节上限、BLOB 十六进制、IGNORE/REPLACE 等选项。DataZen 数据搬运路径直接逐批 `execute`，未建立自己的事务边界，且主要按行数限制批次。[官方高级选项](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_transfer_1.html)
4. **目的地。** Navicat 可输出 SQL 文件，并配置格式、编码，且可创建缺失目标 database/schema。DataZen Transfer 的目标模型仍是在线 endpoint；仓库有导出模块，不等于 Transfer 已具备“同一计划输出文件”的体验。[官方目标选项](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_transfer_1.html)
5. **配置复用与运行记录。** Navicat 支持 profile 与 automation。DataZen 当前窗口没有完整 Transfer profile 管理；也没有可验证的运行历史、分块恢复与结果校验闭环。[官方 Transfer 概览](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_transfer.html)

**注意：** recordset 分块不是断点续传的同义词；本次未找到足够官方证据证明 Navicat 17 为任意迁移提供崩溃后精确续传。因此断点恢复属于 DataZen 可靠性建设目标，不标成已证实的 Navicat 独占能力。

### 1.3 Data Sync

**已有能力：** 同族门闸、表映射、INSERT/UPDATE/DELETE/UNCHANGED、行级勾选、默认关闭 DELETE、SQL 预览、专用参数化执行和事务。MySQL/PostgreSQL 驱动按 handle 路由到事务连接，因此不能只看到 execute 没显式接收 tx 参数就断言“事务无效”。[事务路由示例](/Users/flyxl/code/datazen/packages/drivers/postgres/src/execution.rs:616)

**对标差距：**

1. **数据库范围。** DataZen V1 在后端写死仅 MySQL 家族、PostgreSQL 家族；Navicat 17 列出 SQLite、SQL Server、Oracle、Snowflake、MongoDB 等更多产品范围。安装了驱动不等于这个驱动已经支持同步。[当前门闸](/Users/flyxl/code/datazen/src-tauri/src/data_sync/pairing.rs:8)、[Navicat 支持范围](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_sync.html)
2. **映射灵活性。** DataZen 要求同名列集、等价类型、相同 PK 列及顺序；Navicat 17 有 Key Mapping、Field Mapping。应补明确列投影和键映射，而不是先去掉所有兼容门闸。[官方映射步骤](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_sync_2.html)
3. **审阅体验。** DataZen 有真实逐行勾选，不是只能看汇总；但使用“第 N 列”表头、500 行本地切片，高亮还存在字段名不匹配。Navicat 有两端数据展示、差异过滤、逐行取消与长内容查看。[官方差异审阅说明](https://help.navicat.com/hc/en-us/articles/115003576232-Can-I-check-for-data-differences-between-the-source-and-target-during-synchronization)
4. **执行效率/控制。** DataZen 按语句逐条执行，未提供可靠的真实 affected-row 计数与完整冲突状态；Navicat 有多语句执行、事务/遇错策略、部署日志和脚本重排等控制。[官方数据部署选项](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/data_sync_4.html)
5. **复用。** 仓库存在旧 SyncTask CRUD，但这不能视为当前逐行同步已具备可恢复 profile。旧模型还把运行时 session id 持久化，需按项目 connectionId / dbSessionId 规范迁移。[旧任务模型](/Users/flyxl/code/datazen/src-tauri/src/store/models.rs:40)

**不应误列为竞品差距：**

- 本次核对的 Navicat 17 手册没有给出“任意无 PK / 任意不同结构表均可自动同步”的承诺；旧官方手册强调过 PK/结构一致，而 17 版提供键/字段映射。无 PK 的具体边界应单独实测，不能沿用旧版断言，也不能反向宣称 Navicat 任意支持。
- 不把跨方言 Transfer 等同于跨方言 Data Sync。DataZen 保留三个领域模型的边界是合理的。
- Navicat 官方所谓双向操作是按需求分两次同步，不等于带冲突合并的持续双向复制。[官方双向说明](https://help.navicat.com/hc/en-us/articles/218301577-How-can-I-set-the-Data-Synchronization-as-two-way-process)
- CDC、gh-ost/pt-osc、分布式事务、任意中断下 exactly-once，不列为这次追平的必要条件。

### 1.4 对用户材料的校正

- “Schema Diff 只对已有表 ALTER”已不准确：当前有 CreateTable 路径。
- “依赖 DAG”高估实现：当前是固定桶排序。
- “Sync 可能全表 SELECT”未准确描述主路径：读取已 keyset 分页，但结果含 unchanged 行，仍全量累积。
- “执行前可 revalidate”是函数存在层面的描述：当前 UI 执行路径未调用，函数本身也不是与上次快照比较。
- “Transfer Preview 阻止自覆盖”不能作为可靠保证：后端自覆盖函数比较 runtime session id；双端 dedicated session 会使同一物理库具有不同 session id，且 Preview 未调用该函数。

## 2. 具体代码问题、触发场景与修复方向

### F01｜发布阻断：Sync 失败后回退，会扩大用户批准的写入范围

定位：[UI 执行链路](/Users/flyxl/code/datazen/src/windows/data-sync/DataSyncWindow.tsx:798)、[回退重新比较](/Users/flyxl/code/datazen/src-tauri/src/commands/sync/apply.rs:205)。

`try` 同时包住生成 SQL 与执行。任何异常被空 `catch` 吞掉，随后 `applyDataSync` 只携带表名和全局 options，后端重新 compare，且映射参数为空。

触发：一张表有 100 条 UPDATE，用户只选 1 条；生成失败或执行报错后进入 fallback，其余 99 条重新成为默认选择。自定义表映射也会丢失。执行结果不确定时，这种自动重试还会让审计变得困难。

修复：删除写入回退；能力不支持必须在 Compare 前显式报告。执行错误进入 Failed/Unknown 状态；重试绑定同一冻结 ChangeSet，重新校验后才能执行。UI 只提交 selection revision / planId，不能在错误处理中偷偷生成新计划。

验收：生成失败、执行失败、提交结果未知三个场景均不得调用 apply；未勾选行与自定义映射在重试后保持不变。证据级别：完整调用链静态确认。

### F02｜发布阻断：Transfer 投影后仍按原表序号取值

定位：[投影查询](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/execute.rs:405)、[取值函数](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/execute.rs:174)。

SELECT 已按活动映射投影，但 `map_row_values` 按完整 source schema 的位置再次索引。

复现：源列 `[id,name,age]`，跳过 id，SQL 返回 `[Alice,30]`，实际映射得到 `[30,NULL]`。这不只影响手工跳过：目标比源少列导致自动映射过滤时也可能触发。

修复：对投影后的结果按投影序消费，或使用结果集 column id→index；RowShape 与 QueryProjection 显式绑定；缺失值报错，不以 NULL 兜底。

验收：跳过首/中/末列、重排列、仅选一列、自动匹配子集全部逐值一致。**已通过抽取原函数的 Rust 小型复现确认。**

### F03｜发布阻断：Sync 以源列顺序读行，却按目标物理列顺序写

定位：[Compare 使用源列投影](/Users/flyxl/code/datazen/src-tauri/src/commands/sync/apply.rs:97)、[生成 SQL 重新取目标顺序](/Users/flyxl/code/datazen/src-tauri/src/commands/sync/apply.rs:172)、[UPDATE 取值](/Users/flyxl/code/datazen/src-tauri/src/data_sync/sql.rs:325)。

gate 按名称检查列，允许物理顺序不同；TableResult 却没有携带稳定列投影。源 `(id,a,b)=(1,10,20)`、目标列顺序 `(id,b,a)` 时，更新 a 会取到 20，而不是 10。INSERT 也有同类风险。

修复：CompareResult 保存 canonical columns；每个值绑定源 column id；DML 通过 ColumnBinding 解析目标，不重新猜序号。不要以“强制两库列顺序相同”作为永久方案。

验收：不同列顺序 Compare→选行→Preview→Apply→Recompare 值准确、残余差异为零。**SQL 生成原函数的小型复现已确认 `a=20`。**

### F04｜发布阻断：Transfer 同族写入损坏二进制值

定位：[同族批量 SQL](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/execute.rs:84)、[字面量格式化](/Users/flyxl/code/datazen/src-tauri/src/data_sync/sql.rs:135)。

`Value::Bytes` 用 `String::from_utf8_lossy` 转成字符串字面量，非 UTF-8 字节不可逆变成替换字符。原始 `FF FE` 在复现中变成 `'��'`。普通字符串字面量仅转义单引号，也不足以建立跨 SQL mode 的可靠值编码契约。

修复：参数化批写或 Driver bulk writer；二进制保留 bytes。SQL 文件输出另走 Driver 的无损 literal serializer；失败必须带表/列/PK 定位，不静默降级。

验收：0x00、0xFF、全部 256 种字节、引号/反斜杠/换行、JSON、精确 decimal、时间与 UUID 跨路径往返一致。**二进制丢失已做纯函数复现。**

### F05｜高：Sync 数据库排序与 Rust 比较器不一致，可能丢页

定位：[比较器](/Users/flyxl/code/datazen/src-tauri/src/data_sync/compare.rs:22)、[清空后续页](/Users/flyxl/code/datazen/src-tauri/src/data_sync/compare.rs:198)、[数据库 ORDER BY](/Users/flyxl/code/datazen/src-tauri/src/data_sync/keyset.rs:49)。

数据库使用主键类型/排序规则排序，Rust 字符串使用字节字典序。以不区分大小写的排序规则、batch=1、键 `a` 后 `B` 为例，Rust 认为 `B<a`，页进度检查直接 clear，后续数据被当成结束；另一侧还有数据时可能产生错误 INSERT/DELETE。两端排序规则不同、decimal 解码为字符串也需要契约验证。[MySQL 官方排序规则说明](https://dev.mysql.com/doc/refman/8.0/en/sorting-rows.html)

修复：Driver 提供可证明一致的比较键/排序能力；短期不支持的键类型或 collation 明确阻断，不能静默结束。中期可使用外部排序的规范键；不能只对每一页内部 sort，因为跨页顺序仍可能错误。

验收：大小写、重音、Unicode、复合 PK、decimal、UUID、不同页长；全流严格单调，任何违例必须报错。Rust 次序反例已验证；真实 collation 场景仍需驱动 E2E。

### F06｜高：Schema Diff 替换 PK/同名索引时依赖顺序错误

定位：[固定分组排序](/Users/flyxl/code/datazen/src-tauri/src/schema_diff/dependencies.rs:31)、[同名索引生成 drop/create](/Users/flyxl/code/datazen/src-tauri/src/schema_diff/compare.rs:114)。

顺序为 create→add→alter→create-index→drop。更换 PK 会先 AddPrimaryKey 后 DropPrimaryKey；重建同名索引会先 CREATE 后 DROP。合法常见变更也会失败。默认过滤 destructive 后，关联的 add/create 仍可能留下，成为无法执行的半个计划。

修复：建立操作级 DAG；DropOldPK→AddNewPK，DropOldIndex→CreateReplacement，同步表达 FK、列、默认表达式依赖。筛选风险/用户勾选后重新求依赖闭包，不能只 retain 单个操作。循环依赖必须显示需要的分阶段动作。

验收：替换 PK、同名索引改变列/唯一性、删除被引用列、循环 FK，最终 schema 与期望归一化后相同。

### F07｜高：跨表类型上下文串用

定位：[Transfer 合并所有表列类型](/Users/flyxl/code/datazen/src-tauri/src/commands/data_transfer/exec.rs:203)、[Schema Diff mapper 缺表参数](/Users/flyxl/code/datazen/src-tauri/src/commands/schema_diff.rs:140)。

Transfer 把每张表的类型表 flat_map 成仅按列名索引的 HashMap；`a.payload` 是 JSON、`b.payload` 是文本时，后者可覆盖前者，所有表共用错误 formatter 上下文。

Schema Diff 的 mapper 只接受 type/name，再遍历 pairs 找首个匹配列；两表同名同类型列但不同 target override 时，后一张表可能套用前一张表的覆盖值。

修复：类型与映射统一以 `(relationId,columnId)` 索引；每表一个 TransferTablePlan；Schema mapper 显式接收 relation。所有回调都使用当前表快照。

验收：同名列跨表异类型、相同源类型不同覆盖，切换表顺序不能改变计划或结果。证据级别：静态数据流确认。

### F08｜高：Transfer 建表与用户映射/预览不完全一致

定位：[只应用类型覆盖](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/structure.rs:89)、[Create](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/structure.rs:195)、[Drop/Create](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/structure.rs:233)、[Preview 用简化类型](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/preview.rs:85)。

Create IR 没有同时应用 column rename/skip，INSERT 却用 target column；把 `name` 映射为 `display_name` 时可能建 `name`、写 `display_name`。Drop/Create 路径不接收完整 mapping，类型覆盖/DDL override 未走同一逻辑。Preview 使用普通 schema type，执行时另取 full types 重新生成 DDL。

修复：Prepare 阶段生成唯一的 mapped schema + operations；Preview 和 Execute 消费同一个不可变计划。rename/skip 必须同步更新 PK/index/FK 引用；DROP+CREATE 使用相同计划，不独立重建。

验收：Preview CREATE 与实际执行 CREATE 完全一致；列改名、跳过、类型覆盖、全限定目标名和两种建表模式均验收。

### F09｜高：部署前保护没有成为后端强制契约

定位：[Schema Deploy IPC](/Users/flyxl/code/datazen/src-tauri/src/commands/schema_diff.rs:216)、[Sync revalidate](/Users/flyxl/code/datazen/src-tauri/src/commands/sync/apply.rs:254)、[Sync WHERE PK](/Users/flyxl/code/datazen/src-tauri/src/data_sync/sql.rs:407)。

- Schema Deploy 接收客户端 plan，只检查客户端风险标记对应的确认词；未读取连接配置检查 `read_only`，也没有把 plan 与 prepare 时的目标、schema snapshot 绑定。若数据库账号有写权限，应用只读偏好本身不能阻止该入口。
- UI 的“要求完整回滚”未传成后端强制选项。
- Sync UI 当前未调用 revalidate；现有函数重新检查两端是否仍匹配，而不是与旧快照比较，也不保留自定义映射。两边同时改变仍可能通过。
- UPDATE/DELETE 只按 PK；目标行在审阅期间被别人修改时，执行会直接覆盖/删除。`applied` 统计成功语句，不是准确受影响行数。

修复：后端冻结 plan，绑定目标身份、对象版本和 selection revision；执行前强制 readonly/capability/风险检查。同步写入增加原值或 version/hash 条件；Driver 返回真正 affected rows，0 行进入冲突。提交/回滚结果未知单独显示，不能承诺已恢复。

验收：prepare 后目标换库、配置转只读、目标数据变化、两端结构同时变化、手工篡改 plan 均拒绝或进入明确冲突状态。

### F10｜高：keyset 分页没有解决结果内存增长

定位：[完整保留每行变化](/Users/flyxl/code/datazen/src-tauri/src/data_sync/compare.rs:174)、[前端 500 行本地分页](/Users/flyxl/code/datazen/src/windows/data-sync/DiffDetail.tsx:14)。

包括 UNCHANGED 在内的每一行都 push 到 Vec，很多行保留两端副本；随后整个结果通过 IPC 进入 React。读取缓存有界，结果缓存仍为 O(全表行数×行宽)。前端 slice 仅减少 DOM，不减少 Rust、JSON、JS 内存和序列化时间。

修复：默认 unchanged 只计数；差异进入受控临时存储，UI 通过 cursor 拉页，选择存 ID/排除集，长值按需读取。需要查看 unchanged 时走单独分页视图。

验收：百万/千万行不同差异率基准；峰值内存不随 unchanged 行数线性增长；取消时清理临时数据。具体性能目标见第 5 节，不将设计目标写成当前实测成绩。

### F11｜高：Transfer 读取、取消和部分失败的状态不完整

定位：[无 ORDER BY 的 OFFSET](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/execute.rs:405)、[数据循环](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/execute.rs:414)、[结果页](/Users/flyxl/code/datazen/src/windows/data-transfer/DataTransferWindow.tsx:950)。

OFFSET 查询没有稳定排序或一致性快照；源表并发变化会放大漏行/重行风险。不支持 offset 时直接全量 query，不能证明所有驱动都满足 batch size。取消只在批次/语句边界观察；当前表尚未 push 结果时取消，会遗漏当前表状态；部分错误发生在已经写入之后，仍可能仅抛顶层异常。`stop_on_error=false` 下表失败也未必设置 partial。

修复：驱动游标/流式扫描，明确 snapshot 策略；任务 ledger 从开始就记录每表与每批状态；返回 written/committed/rolledBack/unknown；按 execution handle 尝试精确取消，剩余驱动展示“等待当前语句结束”。依赖失败的表/对象不能继续盲写。

验收：第二批失败、首批后取消、读取失败、结构成功数据失败、继续其他独立表、连接断开，结果均能解释目标当前状态。

### F12｜中：Sync 差异高亮与列名协议不一致

定位：[字段标题和高亮](/Users/flyxl/code/datazen/src/windows/data-sync/DiffDetail.tsx:129)、[按真实列名产生 changed_columns](/Users/flyxl/code/datazen/src-tauri/src/data_sync/compare.rs:73)。

后端提供 `changedColumns=['name']`，前端检查 `changed.has('col1')`；非空 changed set 下，真实变化可能不高亮。列头仍显示“第 N 列”，使用户难以核对。

修复：随结果传 columns 元数据，标题用真实字段名，使用 column id 标识变化；NULL、空字符串和缺失值分别展示。

验收：同一行只修改 name，只高亮 name；长文本、JSON、bytes 有专用查看器；跨页勾选保持稳定。

### 还需纳入基础治理的两点

1. **物理端点身份。** [Transfer 自覆盖保护](/Users/flyxl/code/datazen/src-tauri/src/data_transfer/execute.rs:41)比较 runtime id；[dedicated session](/Users/flyxl/code/datazen/src/lib/dedicatedDbSession.ts:24)可为同一端点生成不同 id。不能把 UI 禁选同一 connection 当成完整保护。需结合持久连接归属、服务器身份、catalog/schema/relation identity；别名连接需检测，无法证明不同则在破坏性模式下阻断。普通 UI 是否可绕过所有前端禁选未作黑盒验证。
2. **可维护性。** 三窗口分别约 1407/1058/805 行，超出项目推荐规模。应随职责拆分 endpoint 生命周期、任务状态机、计划/选择、结果页，不以单纯机械拆文件替代行为测试。

## 3. 具体体验优化：用户动作、界面变化、验收

### U1 让用户选目的，再进入正确工具

入口增加“对齐结构 / 复制数据 / 对齐行差异”三种目的，辅助显示作用于目标的实际效果。保留三个独立引擎。常见流程直接给建议：缺表→Transfer 新建；结构差异→Schema Diff；同结构增量→Sync。

将现有跨窗口跳转升级为 `MigrationContext`：connectionId、catalog/schema、选中 relation、返回位置。Schema 部署完成后返回 Sync，自动重新 inspect，保留映射，旧比较明确失效。验收：完整往返不用重选连接、库、表。

### U2 将阻断原因变成可执行的下一步

对象行显示结构化原因，如“目标缺少 email 列”“目标只读”“同一物理表”“类型转换可能丢精度”；旁边给“生成结构计划”“改为新表迁移”“查看连接设置”。已兼容表仍可独立继续，依赖对象除外。

不兼容信息由 code + fields 翻译，不靠拼接英文。仅编辑 en/zh-CN，其他语言按发布流程补齐。

### U3 提供真正可信的映射预览

列映射同时显示源类型、目标类型、nullable/default、样本转换前后值；危险转换标为“需要处理”，不要悄悄把无限 TEXT 预填成 VARCHAR(255) 就当作安全映射。支持同名自动映射、批量大小写/前后缀规则、重置当前表。

预览显示“将创建/修改的目标字段”，而不是独立生成的一份 DDL。验收：映射改名后，样本、CREATE、INSERT 三处同步更新；修改映射后旧计划立即失效。

### U4 做成审阅工作台

左侧对象/操作树与数量，中央两端值或 DDL，右侧变化原因和实际动作；真实列名、固定 PK、只看变化列、操作过滤、按 PK 搜索、跨页选择、长值查看。

选择摘要明确区分“发现差异 100 条 / 已选 7 条 / 本次将删除 0 条”。“选中全部匹配项”与“仅本页”语义必须明确。筛选不隐式更改批准范围。

### U5 简化向导，但保留审查锚点

建议统一为“连接与范围→配置与映射→审阅→执行结果”四个用户可见阶段，复杂设置渐进展开。专家可以保存 profile 后直达审阅；仅无损只读路径允许自动继续。不是把现有 5/6 步机械改成 4 个编号。

返回修改 endpoint 会使后续全部失效；改显示筛选不失效；改 selection 只重算写入计划；改 schema/type 重新 Prepare。验收使用连续旅程测试，覆盖快速切换连接和异步旧响应晚返回。

### U6 运行页面回答“做到了哪一步，停下会怎样”

显示阶段、当前对象、扫描/比较/已写/已提交行、吞吐、耗时。总量未知时显示活动进度，不制造百分比。取消按钮旁说明“回滚当前事务”或“已完成批次保留”，依据 Driver 能力生成。

完成页分：全部提交、部分提交、已回滚、取消、结果未知；提供错误对象定位、日志导出、重新比较和允许范围内的重试。成功状态必须结合校验等级，不能仅表示命令未抛错。

### U7 让任务可复用、可追溯

统一命名 profile、最近使用、复制 profile、预览历史。只保存 connectionId 和逻辑配置；每次运行重建 dbSessionId。运行历史记录计划摘要、实际提交、错误和校验结果。

已有 Workflow 引擎可承接批量任务，不再造调度器。Navicat 可把不同服务器的 profiles 组合成 batch job，并配置结果通知；DataZen 的通知渠道单独设计，不默认发送数据内容。[官方批任务](https://www.navicat.com/manual/online_manual/en/navicat_17/mac_manual/automation_task.html)

## 4. 追平方案：架构与实现拆分

### 4.1 共用基础设施，保留三个领域计划

新增 `migration/` 作为身份、任务、预检查、审计的公共基础设施；保留 `schema_diff/`、`data_transfer/`、`data_sync/`，不要把所有能力塞进一个万能 execute 函数。

推荐契约：

```text
MigrationProfile
  version, kind, source/target connectionId + catalog/schema
  selection rules, mappings, options

MigrationRun
  runId, profileVersion, source/target resolved identity
  planId, planHash, state, revision, timestamps, counters

PreparedPlan
  endpointIdentity, schemaFingerprints, driverCapabilityVersion
  operations, dependencies, parameters/bindings, warnings, requirements
  selectionRevision, riskSummary, validationPolicy

RelationRef
  catalog, schema, name, stable object identity when available

ColumnBinding
  source relation/column id, target relation/column id
  canonical ordinal, source type, target type, conversion policy
```

计划保存在后端；GUI 提交 planId 和批准的 revision，后端自行校验批准范围。不要仅对客户端上传 plan 计算 hash，那不能证明它与已审阅计划相同。

运行状态统一：Draft→Prepared→Reviewed→Running→Committed / Partial / RolledBack / Cancelled / Unknown；任何快照或映射变化产生 Stale，需要重建计划。空变更属于 NoChanges，不能表现为失败，更不能触发 fallback。

### 4.2 Schema Diff：对象快照与真实 DAG

1. 扩展 `packages/driver-api` 的 schema metadata：FK/CHECK/unique、生成列、identity/sequence、collation、表属性、对象定义和依赖。
2. `Snapshot→Normalize→Diff→Operations→DependencyGraph→Risk/CapabilityGate→Render`。same-family 采用 Driver 语义归一化；cross-family 显式进行有损转换分析。
3. 新增 Add/Drop/AlterConstraint、Create/Alter/DropView、Sequence、Routine、Trigger 等操作；MySQL/MariaDB events 与 PostgreSQL extensions 等作为 driver-specific 能力。
4. 新增 operationId，不能只使用当前 `key()`：同一列多个操作、同名索引 drop/create 当前 key 会重合，不足以做图节点 ID。
5. 支持按对象/操作选择；依赖闭包自动联动，被过滤前置条件的操作一并阻断。循环 FK 拆成建表→灌数/改列→加 FK；默认表达式依赖序列时先建序列。
6. Add NOT NULL 无默认值保留现有 backfill requirement，补完整 Backfill→Validate→SetNotNull 流程。回填值由用户或明确表达式提供，不发明业务值。
7. 执行前目标快照指纹校验；执行后重新抓取 schema，区分“计划已执行”与“所有结构差异已消除”。

落点：API 契约在 `packages/driver-api`；具体 SQL 与 driver tests 在 `packages/drivers/<id>/`；Host 的 graph/plan/status 测试在 `src-tauri/src/schema_diff/`。

### 4.3 Transfer：同族保真、异构可解释

1. 同族默认优先使用 Driver 原生对象定义，并通过结构化 relation rename/qualification 生成目标定义；不要用裸字符串 replace 改 DDL。跨族走丰富 IR，所有有损映射生成 requirement。
2. 为每表生成独立 TransferTablePlan，冻结投影、转换器、target schema、CREATE/TRUNCATE/DROP 与后置对象，解决 F02/F04/F07/F08。
3. 新增 ScanRequest（projection/filter/snapshot）与 Driver cursor/stream；有可用键可用 keyset，无键使用同一稳定读取游标。禁止把“supports_offset=true”当成所有方言都支持 `LIMIT OFFSET`。
4. 新增批写契约，优先参数化批写；按 maxRows、maxBytes、参数数目联合切批。COPY/原生 bulk 为可选优化，提供准确类型转换和正常参数化 fallback。
5. 事务策略支持 per-table/per-chunk，在 Driver 能力允许时支持整任务事务；非事务 DDL 不承诺全任务回滚。序列化执行破坏性前置动作，独立表可有限并发。
6. 引入 filter AST、参数绑定、记录范围和均匀分块预览。高级 SQL 条件单独审查，不能把条件文本拼接绕过执行协议。
7. 文件 target 使用同一 PreparedPlan 输出 Driver SQL：编码、对象依赖顺序、无损 literal、manifest 和必要警告；复用现有导出基础设施。
8. 后置恢复普通索引/FK/触发器等；identity/sequence 校正仅在正确 capability 下执行。事务、权限和对象依赖决定顺序，不能一律关闭约束。
9. 校验支持行数、按键分块 hash、全量逐值；跨库先按显式转换规则规范化再比。hash 加速不冒充绝对无碰撞证明；重要数据可精确比较。

恢复语义：先做表级可控重试；再做分块 checkpoint。只有“提交与 checkpoint 可原子记录”或“写入可验证幂等”的路径才宣称自动续传；无 PK append、提交响应丢失等进入 Unknown，重比/人工决策。checkpoint 必须带源快照/版本，快照失效不继续套旧 offset。

### 4.4 Sync：稳定比较、后端 ChangeSet、冲突检测

1. canonical projection 随 ComparisonSession 固定；KeyMapping 显式映射两端字段。唯一非空键支持作为后续 capability，必须检验唯一性、可空性、比较语义；普通非唯一列不能默认为身份键。
2. Driver 提供比较键规范或声明不支持；短期 fail closed，后续通过规范键外部排序/分块解决不同 collation 的全局顺序。
3. 每个比较会话持有两端各自的一致性读取快照并标注时间；不能称作跨两台独立服务器的同一时刻全局快照。
4. ComparisonStore 分页持久化差异，默认只统计 unchanged；选中状态用默认选择策略+例外 ID 集。rowId 绑定 session、relation、canonical key，不使用 UI 数组下标。
5. generate/preview 从后端存储生成冻结 ChangeSet；apply 仅接受 ChangeSet/plan id。移除客户端失败回退，预览生成失败即不可写入。
6. UPDATE/DELETE 加目标原值/version 条件；0 affected rows 是 conflict。对大对象 hash 需明确定义匹配语义，必要时锁定再核对原值。INSERT 的并发冲突不能自动改成 UPDATE。
7. 按 FK/unique 依赖安排动作；不仅要考虑 INSERT 父先子后、DELETE 子先父后，还要处理唯一值交换等约束冲突。驱动支持 deferrable 时使用对应事务策略，否则明确计划为不可执行或需分步。
8. 参数化执行提供真正 affected rows、批次与 commit 状态；提交失败若无法判定，保留 Unknown。执行后提供针对选中范围的 Recompare，默认关闭 Delete 时的残余 target-only 行不能算失败。

### 4.5 驱动扩展必须靠 capability，不继续扩大 Host 白名单

建议增加 migration capability model：对象种类、事务 DDL/DML、snapshot scan、键比较、参数化批写、字节上限、文件导出、precise cancel、冲突检测等。UI 由运行时驱动能力驱动，并区分 unsupported / supported / degraded / needs-user-action。

通过现有 Driver Command API 或明确扩展 Driver API 暴露操作，Host 不新增具体方言 SQL 分支。若改动协议语义，递增 `PROTOCOL_VERSION` 并同步 path/git plugins；旧驱动应明确禁用新增能力。

## 5. 工作包、优先级、验收与排期

以下是**规划估算**：3 名工程师（2 名后端/驱动、1 名前端）+ 1 名测试，已有可用测试数据库环境；不含等待商业驱动授权、采购和大规模历史兼容返工。每阶段完成以退出条件为准。

### M0：正确性止血，2–3 周

- W01：移除 Sync fallback；冻结 selection，补失败/取消/未知状态。依赖：无。退出：F01 连续旅程测试通过。
- W02：统一投影元数据，修 Transfer 子集映射和 Sync 目标列顺序。依赖：小型 ColumnBinding 契约。退出：F02/F03 在 MySQL/PostgreSQL 驱动 E2E 逐值验证。
- W03：同族 Transfer 参数化值写入，修复 bytes；每表 IR 上下文。退出：F04/F07 往返测试。
- W04：先修 PK/索引替换依赖和 destructive 过滤闭包；排序不可信立即报错。退出：F05/F06 不再生成错误/静默不完整结果。
- W05：后端只读检查、plan 目标身份绑定；DDL/映射预览一致。退出：F08/F09 相关拒绝测试通过。

M0 不承诺“追平 Navicat”；目标是使已宣传的核心路径可信。必须补真实 DB 测试，现有 56 个 UI 单测通过不能替代该门槛。

### M1：MySQL/MariaDB 与 PostgreSQL 日常工作流，新增 4–5 周

- W06：RelationRef/PreparedPlan/Run/ComparisonStore 公共基础；修旧任务 ID 持久化；后端分页与统一进度事件。
- W07：FK/CHECK/索引属性/默认表达式元数据与真正 DAG；对象树、细粒度勾选、两端 DDL 审阅。
- W08：Transfer 游标扫描、参数化批写、行/字节限制、表级事务、过滤/分块与后置约束。
- W09：Sync 真实列名、高亮修复、操作过滤、长值 viewer、后端 selection、原值冲突检测、执行后复比。

退出：十个代表性生产旅程通过；大表 unchanged 不再导致线性内存增长；任何 Partial/Unknown 可定位表、批次和提交边界。此阶段可声明“核心关系库日常迁移接近对标”，不能宣称全产品追平。

### M2：对象生态与任务产品化，新增 4–6 周

- W10：视图/序列/函数/触发器，MySQL events 与 PG 特有依赖；同族对象保真，跨族不支持对象给可执行解释。
- W11：命名 profile、历史、Workflow migration step、只读 compare/export 自动运行、写入按明确批准策略运行。
- W12：SQL 文件 target、编码与参数字面量序列化、对象重建校验；SQLite 迁移/同步适配与能力约束。
- W13：严格限定场景的表/块重试、checksum 校验、连接丢失恢复流程和错误报告。

退出：已验证的 MySQL/MariaDB/PostgreSQL/SQLite 三件套能力清单、迁移 profile 可重放、运行可审计。**累计约 10–14 周**；这仍是限定数据库范围的追平。

### M3：Navicat 产品广度，另立驱动里程碑

- SQL Server：仓库已有驱动，但当前同步 family/迁移 renderer 不等于支持；补分页、参数类型、identity、schemas、computed columns、bulk、transaction/cancel，以及对象依赖测试。
- Oracle：当前 registry 未列出对应驱动；需要驱动/部署环境、NUMBER/LOB/时区、sequence/identity、routine/package、DDL 自动提交边界等完整工作包。
- Snowflake：当前 registry 未列出对应驱动；鉴权/warehouse 生命周期、identifier case、variant/精确数值、批加载/费用可见性、查询取消与对象模型需独立评估。
- MongoDB：已有驱动，但需要文档身份、BSON 类型、collection/index/validator、变更比较和事务能力；不能把 SQL TableSchema/DDL 硬套过去。跨 SQL↔Mongo 的通用转换不作为默认承诺。

估算方式：SQL Server 约 3–5 人周；Oracle 约 6–10 人周；Snowflake 约 5–8 人周；MongoDB 约 4–6 人周；再留跨库矩阵、性能与文档的专项验证。以上均为粗估，需各驱动短期技术验证后重估，不能按“增加一个类型映射”排一两天。**未验收这些数据库前，不宣称 Navicat 全产品能力追平。**

### 验收矩阵必须覆盖的十个旅程

1. 同族复制：完整列、子集、重命名、二进制、索引/FK、注释和 identity/sequence 后续插入。
2. 跨族复制：MySQL↔PostgreSQL，decimal 边界、Unicode、JSON、UUID、时区、默认表达式与不支持类型。
3. Schema 演进：缺表、新列、NOT NULL 回填、PK 替换、同名索引替换、依赖循环、目标独有对象。
4. 选择语义：全局 options、表/行选择、翻页取消选择、筛选、返回修改配置、旧请求晚到。
5. Sync 列/键语义：列顺序不同、字段映射、复合键、排序规则、空值、精确数值。
6. 外部漂移：比较后源变动、目标变动、两端同时改结构；必须 stale/conflict 或按明确策略执行。
7. 执行故障：第二批报错、commit 响应丢失、rollback 失败、驱动中断、取消与最后一条语句竞争。
8. 权限/身份：只读偏好、数据库权限不足、同物理表不同 session、别名连接、同库不同 schema。
9. 大数据：100 万/1000 万行；0/1/100% 差异；窄/宽行和大对象；单表与多表；报告吞吐、RSS、首屏时间、取消延迟。
10. 复用/恢复：重启后 profile 用 connectionId 重连；checkpoint 与提交状态一致；源快照失效拒绝盲续传。

建议初始性能 SLO（待固定测试机/数据库/网络后校准）：大表 unchanged 增长 10 倍时比较器额外内存不线性增长；UI 已落盘结果分页 P95 <300ms；取消后 UI 1 秒内进入 cancelling；真实中止/回滚时间依据驱动与语句分别记录。吞吐目标用统一数据集与 baseline 比较，未压测前不编造 Navicat 倍数。

测试落点遵守项目规范：方言、解码、具体驱动事务/分页测试在各 `packages/drivers/<id>/tests|e2e|ui/__tests__`；Host 只测公共 orchestration、DAG、状态、UI journeys。Host E2E 使用 `pnpm tauri:build:webdriver` / 现有 runner，禁止裸 cargo build 替代。

## 6. 本次验证记录与限制

执行：

```sh
pnpm exec vitest run src/windows/data-sync/__tests__ src/windows/data-transfer/__tests__ src/windows/schema-diff/__tests__
```

结果：**9 个测试文件，56 个测试全部通过**，Vitest 报告执行耗时约 2.58 秒。这说明已有测试断言通过，不说明上述执行缺陷不存在。

[小型 Rust 复现源码](/Users/flyxl/.codex/visualizations/2026/09/09/01a085c7-e45e-7160-90a4-543614daf260/migration-review-repro.rs)抽取当前 `map_row_values`、SQL 生成/字面量函数、键比较函数，移除序列化依赖，以最小 DTO 替身编译；没有修改函数主体。

观察输出：

```text
F02: [Alice,30] -> [30,NULL]
F03: UPDATE t SET a=? WHERE id=?; parameters=[20,1]，期望 [10,1]
F04: bytes FF FE -> '��'
F05: cmp(B,a)=Less，数据库按 a,B 排页时会触发后续页清空分支
```

未执行真实数据库 DDL/DML、未验证 Navicat 实机行为、未测试所有 path/git drivers，因而不提供性能评分或“完成度百分比”。官方文档已确认的能力、源码可证明的缺陷和建议新增的可靠性能力在上文分别标明。

**推荐第一批实现顺序：F01 → F02/F03 → F04/F07 → F05/F06 → F08/F09；随后推进 ComparisonStore、对象快照/DAG和任务复用。**
