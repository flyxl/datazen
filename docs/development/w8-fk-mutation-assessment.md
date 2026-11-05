# W8-b 评估档：tableData FK 四 + 变更四端点逐端口点亮评估

> **评估日期**：2026-11-06。**基线**：main `f15a671d`（PR #180 合并后），分支 `w8-eval-fk-mutation`（纯文档 PR，零产品代码）。
> **评估对象**：`crates/datazen-server/src/http/domains/table_data.rs` 模块头状态表的 8 条 ⛔ fail-closed 端点（FK/来源/值搜索四条 + 变更四条）。
> **评估方法**：只读勘察。桌面侧逐端口精读服务与宿主装配点，server 侧盘点既有执行件，逐条回答「能组合点亮吗、缺什么、多大、先做谁」。
> **前置阅读**：[w8-handoff.md](w8-handoff.md)（W8-a 三刀与 W8-b 四项）、[web-ready-plan.md §11](web-ready-plan.md#11-进度回写区) 2026-11-05 接线刀条目。

## 1. 背景与目标

W8-a 接线刀（PR #180）把 tableData 读三端点点亮为真用例，其余 8 条保持 501 `capabilityUnsupported`（`table_data.rs:666-677` 的 `fail_closed`，原因常量 `REASON_FK_SOURCE` / `REASON_DRIVER_QUERY` / `REASON_ORIGIN` / `REASON_PLAN` / `REASON_COMMIT` 在 `table_data.rs:679-695`）。登记的逐端口原因：

- **FK / 值搜索 / 来源重推导四条**：「活 `ConnectionHandle` 直连面」——server 执行面命令经 runtime 派发，无直连句柄面。
- **变更四条**：「`TableColumnSource` 目录源（真实列/主键应答）+ runtime 写半边 `execute_plan`」——且预览与提交必须成对（半能力 fail-closed）。

本文回答一个问题：**这 8 条各自要哪些件、server 已有哪些、能组合吗、W8-b 三件（stream 投递桥 / driverType 直取 / 命令历史落账）落地后是否出现新组合路径、建议按什么顺序排刀**。每个断言都指回 `file:line`。

**先说结论**（一句话版，详见 §4 表）：

1. 一个 **跨 resolve 会话连续性缺口（g1 CM-63）** 是 8 条中 6 条的共同前置——mainline `ServerRuntimeLeaseSource` 每 resolve 新建 runtime 实例，sessions/tokens 账不共享（`web-ready-plan.md` §11 2026-11-05 条目、`crates/datazen-server/tests/table_data_routes.rs:875-890` 夹具注释），凡请求携带 `dbSessionId` 的端点（FK 二 + 值搜索 + origin + result-cell 二 + manual_handle 提交）都撞它。
2. **FK 二 + `searchTableValues` + `resolveResultOrigin` 四条**：共享服务规则一行不用改，缺的是 server 侧「活句柄面」三件——`LiveConnectionSource` 的 server 实现、租约源接线 `with_live_connections`、`ForeignKeyNavigationSource` 的 server 适配器。缺口件齐后是 **一刀四条**（M）。
3. **`previewPendingChanges` + `commitPendingChanges` 二条**：写半边（`execute_plan`）在共享层已是 transport 中立的 runtime 面，server 件基本齐全；唯一新件是 server `TableColumnSource`（可经命令面 `get_table_schema` 组合，不必等活句柄面）。成对点亮一刀（M），v1 恒 owned-session。
4. **`commitQueryResultCell` + `previewResultCellChange` 二条**：卡得最深——原会话绑定（CM-63）+ 结果映射探针未移植（`driver_exec.rs:18` 缺口登记），最后排（L）。

## 2. 桌面侧服务勘察（逐端口）

### 2.1 `services::fk_navigation`（`resolveForeignKeyTargets`）

- **端口**：`ForeignKeyNavigationSource::bind_session(&self, db_session_id) -> FkSessionBinding`（`crates/datazen/src/services/ports.rs:67-70`）。`FkSessionBinding`（`ports.rs:45-58`）= `driver: Arc<dyn DatabaseDriver>` + `handle: ConnectionHandle` + `database/schema` 档案回退 + `guard: Box<dyn Any + Send + Sync>`（宿主租约的不透明守护，**必须活过每一次驱动调用**）。
- **三条驱动往返**（`crates/datazen/src/services/fk_navigation.rs:264-430` 的 `resolve_foreign_key_targets`）：
  1. 源表外键元数据：`driver.get_table_schema(&binding.handle, source_table, &database, schema)`（`fk_navigation.rs:288-306`）；
  2. 目标表元数据：`resolve_target_table` → 目标表的 `get_table_schema`（`fk_navigation.rs:358-365`）；
  3. 存在探针或浏览：`probe_target` / `browse_target`（`fk_navigation.rs:382-406`），语句由 `probe::build_candidates_statement` / `build_probe_statement`（`fk_navigation/probe.rs:52,97`）按方言组装，值绑定走 `dialect.parameter_placeholder`（无占位符方言才内联）。
- **返回形状**：`ForeignKeyTargetResolution { target: ForeignKeyTargetRef{database,schema,table,columns,data_types,primary_keys,unique,candidate_limit}, exists, candidates, truncated }`（`fk_navigation.rs:415-429`；候选上限 `DEFAULT_CANDIDATE_LIMIT=50` / `MAX_CANDIDATE_LIMIT=200`，`fk_navigation.rs:36-39`）。
- **关键性质**：三条往返**共享一次绑定**——端口文档明说「hand out a single binding instead of separate driver lookups that could straddle two sessions」（`ports.rs:60-65`）。这就是 501 登记的「活句柄面」本义：不是一条命令，是一段持有同一连接的三连读。

### 2.2 `services::referenced_by`（`listInboundForeignKeys`）

- 同一端口（`ForeignKeyNavigationSource`），`crates/datazen/src/services/referenced_by.rs:123-241` 的 `list_inbound_foreign_keys`。
- 驱动往返**不止三条**：源表 schema（`referenced_by.rs:152-168`）→ 同库全表列表 `driver.get_tables`（`:172-188`）→ 逐表 `get_table_schema` 扫描，上限 `MAX_INBOUND_SCAN_TABLES=200`（`:33-39,193-227`），`truncated` 如实报告早停。
- 返回 `InboundForeignKeyCatalog { table, database, references[], scanned_tables, truncated }`（`:77-90`）。
- 对 server 的含义：这是**最长**的活句柄持有面（最多 200 次 schema 读）。借用语义必须覆盖整个扫描窗；中途会话被驱逐 → 驱动报 `SessionLost`，是可接受失败方向。

### 2.3 `services::value_search`（`searchTableValues`）

- `crates/datazen/src/services/value_search.rs:213-280` 的 `search_table_values(driver, handle, database, schema, request)`——**直接吃活句柄**（签名第二参 `handle: &ConnectionHandle`，`:215`）。
- 执行面：`build_search_statement`（`:146-210`，纯方言组装 + LIKE 转义 + 绑定/内联二形）→ `driver.query_with_params_at(handle, &sql, &params, target)` 或 `driver.query_at(handle, ...)`（`:251-258`）→ 取 COUNT 单元格（`count_of`，`:283-290`）。
- 扫描节奏在前端（一表一命令，暂停 = 不发下一条），host 命令从不自行迭代（桌面命令头 `commands/value_search.rs:11-13`）。
- 对 server 的含义：这是 8 条里**最轻**的驱动面（一条 COUNT 查询）。存在两条点亮路：活句柄（与 FK 同刀零边际）或命令面（语句构建导出 + runner 跑 `query`，见 §4 表第 3 行的备选路）。

### 2.4 `describe_result_origin` / `resolveResultOrigin`

- **契约在 driver-api**：`crates/datazen-driver-api/src/result_origin.rs:186-209` 的 `describe_result_origin_with_catalog(driver, handle, target, result_columns, sql)`。两半：`analyze_select_origin`（纯 SQL 分析，**不碰句柄**）+ `resolve_relation_kind`（`:217-254`，`driver.get_tables(handle, ...)` 一条目录读确认关系类型）。
- **桌面命令**：`crates/datazen-desktop/src/commands/result_origin.rs:67-102` 的 `resolve_result_origin`——`resolve_command_driver(&state, Some(&db_session_id), None)` 借活句柄（`driver_command/resolve.rs:32-84`，`runtime.borrow_live_connection_by_id(ctx, id, LivePurpose::CommandExecution)`，`:45-58`）→ `driver.describe_result_origin(&resolved.handle, ...)`（`result_origin.rs:177-193`）→ 源表列/主键从**目录缓存**读：`state.schema_cache.get_columns(&profile.id, database, schema, &table, &catalog, &resolved.handle)`（`:199-236`，`LegacyCatalogReader` + 同一活句柄）→ `evaluate_result_editability`（共享层 `table_editing/result_edit/mod.rs:210`）。
- **`previewResultCellChange`**（同文件 `:109-171`）：写前**重读 origin**（缓存裁决不作数）→ 不可写即拒绝 → `build_result_cell_change_plan`（共享层纯函数，`result_edit/mod.rs:539`）产出 `RowChangePlan`。
- 对 server 的含义：origin 的「分析半边」是纯的，可命令面跑；「关系类型确认半边」是一条 `get_tables` 目录读；「列/主键半边」是一条 `get_table_schema` 目录读。三条都是单命令形状——但桌面把三者编排进一个函数，server 若走命令面就得在宿主侧复制这份编排（规则双家），走活句柄则共享层原样跑。

### 2.5 变更两段式：`table_editing::preview_changes` / `TableColumnSource` / `commit_changes` / `execute_plan`

- **端口**：`TableColumnSource::table_columns(&RowChangeTableContext) -> CachedColumns`（`crates/datazen/src/table_editing/mod.rs:47-52`）。端口文档硬要求：答不出必须显式报错，**空答案不等于「没有主键」**（`:41-46`）。
- **预览半边**：`preview_changes(driver, table, changes, columns, config_revision)`（`mod.rs:68-79`）= 取列 → `build_row_change_plan` → 盖 `config_revision` 章返回 `RowChangePlan`。纯驱动语句组装 + 目录列，无执行。
- **提交半边**：`commit_changes(driver, request, facts, columns, manual_handle, runtime, artifacts, context)`（`mod.rs:104-156`）——闸序：`validate_table_context` → `validate_connection_facts`（驱动类型漂移）→ `config_revision` CAS（`:116-121`）→ `read_only` 拒写（`:122-126`）→ database/schema 上下文比对（`:127-146`）→ 重读列 → `validate_immutable_plan`（**计划重建自不可变变更元数据，客户端 SQL/摘要字段仅展示**，`:148-154`）→ `execute_plan`。
- **写半边 `execute_plan`**（`table_editing/write_exec.rs:161-352`）：**全是 runtime 面，没有一处活句柄**——语句清单（insert 先行 / update / delete，`:169-183`）→ 会话取得：manual_handle（编辑器事务，三字段 + `TransactionState::Active` 校验，`:195-210`）或自开（`runtime.open_session`，`OwnerRef::ClientSession{purpose:"tableChangeSet"}`，`:212-239`）→ `platform.beginTransaction`（`:267-273`）→ 逐条 `execute`（`TableSession::execute`，`:75-158`：`issue_submission_token` → `execute_in_session` → 轮询 `get_execution` → 产物读 `rowsAffected`）→ **`affected != 1` 拒绝歧义写**（`:288-294`）→ `platform.commitTransaction` → `close_session(RollbackAndClose)` 清理（`:330-351`，Drop 兜底 `:49-73`）。
- **桌面宿主装配**（`crates/datazen-desktop/src/commands/data.rs`）：
  - `preview_pending_changes`（`:22-53`）：`platform_profile` 闸 → 驱动 → `table_columns::HostTableColumns` → 共享 `preview_changes`。
  - `commit_pending_changes_with_handle`（`:55-132`）：闸 → 驱动 → `adapter.runtime()`（**一个实例贯穿整个提交序列**）→ `ConnectionFacts{read_only, config_revision, ...}` → `HostArtifactChunks::new(runtime).shared()` → 共享 `commit_changes`。
  - `HostTableColumns`（`data/table_columns.rs:41-129`）：`schema_cache.try_get_cached_columns` 快路径（`:91-97`）→ miss 则 `resolve_command_driver` 借活句柄 → `LegacyCatalogReader` + `schema_cache.get_columns`（`:104-115`）。
- **`commitQueryResultCell`（桌面）是另一条窄写路**：`commands/result_write.rs:127-383`——**不走 `execute_plan`**，全程 runtime 面：`get_execution` 校验 `runtime_binding`（`:134-144`）→ `platform_profile` 的 read_only / enabled / revision 闸（`:156-181`）→ 产物块重放找 `statementMetadata`（`writable_mapping == Verified`，`:182-195`）→ `WriteTarget` 比对（`:209-215`）→ `get_table_schema` 命令重验映射（`:251-279`，`result_mapping::verify_result_mapping`）→ 主键定位 → `driver.build_update_statement`（纯构建，`:307-320`）→ `execute_in_session("query")`（`:321-329`）→ `affected==0` 时 `confirm_unchanged_read_sql` 复核（`:335-378`）。

### 2.6 宿主装配点小结（桌面怎么把活句柄交出去）

唯一的活句柄出借路是 `RuntimeConnectionUseCases::borrow_live_connection_by_id`（`crates/datazen/src/execution/application/orchestration/mod.rs:382-393`），它要求活连接出口端口**已接线**（`with_live_connections(source)`，`:237-240`；未接线时 `LiveConnections::borrow` 返回 `NotWired`，`orchestration/live.rs:390-394`）。桌面适配器 `DesktopLiveConnectionSource`（`crates/datazen-desktop/src/platform/live_source.rs:103-150`）的四步：backend 的 `by_session` 反查物理资源 → profile 权威驱动类型 → `DriverRegistry::get` 取**同一**驱动 `Arc` → `ResourceProvider::connection_handle()` 借出驱动真句柄（`resource.driver_handle()`，`:144-148`）。装配点在 `platform/adapter.rs:519`（`runtime.with_live_connections(live.clone())`）。

## 3. server 侧可用件盘点

### 3.1 执行底座（已齐）

| 件 | 位置 | 与端口形状的差距 |
| --- | --- | --- |
| `ServerTargetRunner`（`TargetCommandRunner` 端口实现） | `crates/datazen-server/src/execution/target_runner.rs:78-191`；target 分支 = `host.prepare`（WR-F7 闸序）→ `ExecuteAtTarget`（CAS）→ 25ms 轮询 → 产物分块收集；manual 分支 = 租约现取 → `get_session` 三字段校验 → `ExecuteInSession` | 无。读三端点已用它（`table_data.rs:337-396` 的 `resolve_read_target` + estimate 的 manual 绑定 `:625-633`） |
| `ServerResourceExecutionHost`（`ResourceExecutionHost` 端口） | `crates/datazen-server/src/execution/resource_host.rs`（装配 `bootstrap.rs:400-409`） | 无 |
| `ServerDriverExecBackend`（`SessionBackend` 端口 server v1） | `crates/datazen-server/src/execution/driver_exec.rs:98-109`；进程内直连，`resources: HashMap<resource_id, Resource{provider, handle: dr::ResourceHandle,...}>` + `by_session: HashMap<dbSessionId, resource_id>` 反向索引 | **反查索引已在**（`:108`），但没有对外出口——活句柄借出件缺失（见 3.3） |
| 会话目录 | `datazen::execution::runtime::directory::InMemorySessionDirectory`，租约源与 sessions 域 / tableData 域同一实例（`lease.rs:41,82-87` + `bootstrap.rs:367-387`） | 无。可见性闸（`lookup_owner` + 越组织 404）已是 tableData estimate 的既有闸序（`table_data.rs:547-588`） |
| 会话用例面 | `LeaseConnectionUseCases`（`lease_use_cases.rs:34-170`，每调用现取）已进 `TableDataState.sessions`（`table_data.rs:132`） | **CM-63**：sessions/tokens/receipts 账在每次 `resolve()` 新建的 `RuntimeConnectionUseCases` 私有 map 里（`orchestration/mod.rs:218-219,228`；`session_entry` 读本实例 map `:584-605`），跨 resolve 失联。已登记（`table_data_routes.rs:875-890` 夹具注释、`web-ready-plan.md` §11） |
| 共享结果 sink | `RuntimeResultSink` 跨 resolve 共享（`lease.rs:38` 注释、`bootstrap.rs:354-359`）；`get_execution` 与 `read_artifact_chunk` 都读 sink（`orchestration/mod.rs:481-489`、`operations.rs:777-783`） | 无——**执行视图与产物块跨 resolve 可见**，这是 result-cell 路唯一的好消息 |
| 产物读取 | `ServerArtifactChunks`（`resource_host.rs`，`pub(crate)` 投影 `RuntimeConnectionUseCases.read_artifact_chunk` → `ArtifactChunkReader` 端口，`target_runner.rs:143-144` 已用） | 无（同 crate 内可见） |
| 只读/版本事实 | `ServerProfiles`（enabled + `config_revision`）+ `ServerConnections`（`ConnectionConfig` 含 `read_only`、database_type）已进 `TableDataState`（`table_data.rs:122-126`） | 无 |
| 目录/元数据缓存 | `datazen::schema::cache::SchemaCache`（`crates/datazen/src/schema/cache.rs:43-58`）是宿主中立件（按 connection_id + (database,schema) 分桶，TTL 300s）；server **尚未实例化** | 需要在 `TableDataState` 挂一个实例（S） |
| 命令面的目录读 | 命令目录含 `list_tables` / `get_table_schema` / `list_databases`（`crates/datazen-driver-api/src/schema_catalog_commands.rs:17-120`）；`get_table_schema` 要求 object target（`command_catalog.rs:132-135`）；`resource_execution.rs:245-252` 已把该命令映射为 object target | 无——命令面能回答「列/主键/表清单/表类型」 |

### 3.2 执行编排参照（已齐）

- `resource_execution.rs`（`crates/datazen/src/workflow/application/resource_execution.rs`）：`execute` / `execute_with_existing_session`（`:81-152`）展示了「命令编排（定义解析/输入校验/访问分级/SQL 守卫）→ `prepare` → token → execute（target 或 in-session）→ 轮询 → 产物组装」的完整 server 可用形状；`SessionBinding::Borrowed` 语义「借的会话绝不关」（`:101-129`）。
- driverGateway 域 `execute_driver_command` 的 dbSessionId 路径已真实现（`crates/datazen-server/src/http/domains/driver_gateway.rs:14`）；`driverType` 直取与 stream 投递仍 501（`:14-15`）。

### 3.3 活句柄面（**缺，但有全部零件**）

关键事实链：

1. 端口存在：`ResourceProvider::connection_handle(&ResourceHandle) -> ConnectionHandle`（`crates/datazen-driver-api/src/resource.rs:596-599`，「Borrow the already-held driver's handle for Host metadata adapters」）。`LegacyResourceAdapter` 实现了它（`resource_adapter.rs:359-364`），sqlite/mysql/mongodb 原生实现（`crates/drivers/{sqlite,mysql,mongodb}/src/resource.rs:319,392,264`）。
2. server backend 已按 dbSessionId 记账：`ServerDriverExecBackend.by_session`（`driver_exec.rs:107-108`）+ `resources` 表持有 `(provider, dr::ResourceHandle)`（`:85-94`）。
3. runtime 出向端口存在：`LiveConnectionSource::open_live(LiveConnectionRequest) -> LiveConnection`（`orchestration/live.rs:167-177`），契约禁止新建物理连接（`:171-176`）；借用作废语义（驱逐/关闭时 `revoke`，`live.rs:241-253`）与 RAII 归还（`LiveConnectionLease`，`:284-333`）都在。
4. **server 没接线**：`ServerRuntimeLeaseSource::resolve()` 构造 `RuntimeConnectionUseCases::new_with_parts(...)` 后**没有调 `with_live_connections`**（`lease.rs:89-104`），`live` 恒 `LiveConnections::unwired()`（`orchestration/mod.rs:225`）。桌面在 `platform/adapter.rs:519` 接线；server 没有对应物。

即：**server 缺的是一个 `ServerLiveConnectionSource`（镜像桌面 `live_source.rs` 四步：`by_session` 反查 → profile 权威类型 → registry 取同实例驱动 → `provider.connection_handle()` 借出）+ 租约源接线**。零件全在仓内，无共享层改动。

### 3.4 CM-63：跨 resolve 会话连续性（8 条中 6 条的共同前置）

mainline 每 resolve 新建 runtime：sessions/tokens/receipts map 随实例生灭（`orchestration/mod.rs:208-229`）。后果：

- `POST /sessions` 开的会话（resolve N 登记）在下一次请求的 resolve M 里 `get_session` → `session_entry` → **`SessionNotFound`**（`operations.rs:449-456` → `mod.rs:584-597`）。
- `issue_submission_token` 与 `execute_in_session` 的签发/消费同理失联。
- `get_execution` / `read_artifact_chunk` **不受影响**（读共享 sink）。

这与 8 条端点的关系：FK 二与值搜索要 `borrow_live_connection_by_id(db_session_id)`（先 `get_session_by_id`，`mod.rs:363-375`）——撞墙；`commitQueryResultCell` 的原会话绑定撞墙；`execute_plan` 的 manual_handle 路撞墙；estimateTableRows 已登记挂此（`web-ready-plan.md` §11：estimate 生产可用性挂 W8-b ②）。**修法候选**：

- **R1（推荐评估起点）**：租约源持「当前 runtime」单元——`resolve()` 返回同一 `Arc<RuntimeConnectionUseCases>` 直至吊销/重建（保留「可能被吊销」语义：吊销事件发生时换新实例，旧会话按既有 SessionLost 语义失效）。改动集中在 `lease.rs`（S/M），语义审查点：吊销触发点、重建窗口、`new_with_parts` 的私有账迁移。
- **R2**：W8-b ② 的宿主级 ephemeral 会话账（principal 归属 + TTL）——若 ephemeral 会话与活连接反查都落在**宿主级**账（不经 per-resolve runtime 实例），则「无编辑会话」的轻场景（FK 跳转、值搜索、estimate）可绕开 CM-63。覆盖面有限：编辑会话（openSession 开立的）仍在主线账上。

### 3.5 W8-b 三件与本题的相关性

| W8-b 件 | 对 8 条端点的影响 |
| --- | --- |
| stream 投递桥（`QueryStreamEvent` → events SSE；`driver_gateway.rs:15` 仍 501） | **无直接组合路径**。8 条全是同步单应答；FK 候选上限 200（`services/fk_navigation.rs:39`），无流式需求。 |
| driverType 直取（内联临时连接 → 内存 ephemeral 会话句柄 TTL + principal 归属；`driver_gateway.rs:14` 的 driverType 分支仍 501） | **间接相关，是 FK/estimate 的可能前置替代**：FK 二与 `searchTableValues` 的请求契约带 `dbSessionId`（`services/fk_navigation.rs:125` 起、`services/value_search.rs:47-49`）、origin 带（`result_origin.rs:37-60`）——web 前端必须先持有会话。若 ephemeral 会话（低成本、TTL、principal 归属）能作为这些请求的会话来源且能被 live face 反查，则 FK 四可在 CM-63 未修时点亮「轻场景」。**开放问题**：ephemeral 会话登记在宿主级账还是 per-resolve runtime（§6）。 |
| 命令历史落 audit_events（`driver_gateway.rs:40-42` 登记 v1 无半边；表已在 v6 迁移 `storage/migrations.rs:177-200`） | **正向**：变更四点亮时写操作的审计落账可复用同一件；不构成前置。 |

## 4. 逐端点评估表

工作量标尺：**S** = 单文件装配/适配器（约 ≤150 行 + 测试）；**M** = 跨 2–4 文件的新端口实现 + 接线 + 语义钉测试；**L** = 共享层或架构级改动 + 语义审查。

| # | 端点 | 点亮所需件清单 | 既有件可组合？ | W8-b 后新路径？ | 缺口件 | 量 | 排刀 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `resolveForeignKeyTargets` | ① CM-63 修复或 ephemeral 会话（`dbSessionId` 可解析）② `ServerLiveConnectionSource` + 租约源接线 ③ server `ForeignKeyNavigationSource` 适配器（bind → borrow → `FkSessionBinding{driver,handle,database,schema,guard:lease}`，镜像桌面 `commands/fk_navigation.rs:23-62`） | **部分**：共享服务零改动可用（`services/fk_navigation.rs:264`）；缺 ①②③ | ephemeral 会话若宿主级登记 + 可反查 → 免 CM-63；否则仍需 ① | ①②③ | M | 刀 2 |
| 2 | `listInboundForeignKeys` | 同 1（同一端口同一适配器）；额外关注最长借用窗（≤200 次 schema 读，`referenced_by.rs:39`） | **部分**：同 1 | 同 1 | 同 1（零边际） | M（并入刀 2） | 刀 2 |
| 3 | `searchTableValues` | 同 1 的活句柄路（共享 `search_table_values` 原样跑，`value_search.rs:213`）；备选命令面路：`build_search_statement` 从 `pub(crate)` 开口（共享层 S 级改动）+ runner 跑 `query` 带 params + 计数解析 | **部分**：同 1 | 同 1（活句柄路零边际）；命令面路有绑定语义差异（runtime `apply_params` 文本绑定 vs 驱动原生绑定，`driver_exec.rs:408-425`）需钉测试 | 同 1 | M（并入刀 2） | 刀 2 |
| 4 | `resolveResultOrigin` | 同 1（`describe_result_origin(&handle)` + `SchemaCache.get_columns(..., &handle)`，镜像 `result_origin.rs:67-102`）；`SchemaCache` 实例化挂 `TableDataState`（S）；纯分析半边 `analyze_select_origin` 免句柄（`result_origin.rs:196`） | **部分**：同 1 | 同 1；纯分析半边可先行但编排复制违反规则单家（不推荐） | 同 1 + SchemaCache 挂载 | M（并入刀 2） | 刀 2 |
| 5 | `previewPendingChanges` | ① server `TableColumnSource`（命令面 `get_table_schema` → `CachedColumns`，或活句柄 + `SchemaCache`）② fail-closed 成对：与 6 同刀 | **部分**：`preview_changes`（`mod.rs:68-79`）+ 驱动 + facts 件齐全；列源缺 | 命令面列源**不依赖** W8-b 与 CM-63（target 面无会话） | `ServerTableColumnSource` | S（件）/ M（成对刀） | 刀 3 |
| 6 | `commitPendingChanges` | ① 同 5 的列源（`validate_immutable_plan` 重读列，`mod.rs:147-154`）② 写半边组装：driver（registry ✓）+ `ConnectionFacts`（profiles/connections ✓）+ runtime（**单次 resolve 的 `Arc<RuntimeConnectionUseCases>` 贯穿提交序列**，owned-session 路全部调用落同一实例 ✓）+ `ServerArtifactChunks` ✓ + `SessionIdentity.ctx` ✓ | **部分**：`execute_plan` 是 transport 中立 runtime 面（`write_exec.rs:161`）；v1 **恒 owned-session**（manual_handle 编辑事务路挂 CM-63） | 无新路径（CM-63 修复后 manual 路才开） | 同 5 + commit handler 组装 | M | 刀 3 |
| 7 | `commitQueryResultCell` | ① CM-63：原会话绑定（`RuntimeResultBinding.handle` 的 `get_session` / `execute_in_session` 必须跨请求可见，桌面流程 `result_write.rs:127-383`）② 结果映射探针移植（`writable_mapping == Verified` 的 `statementMetadata` 来源；`driver_exec.rs:18` 缺口登记：执行期结果映射探针/查询历史/结果上限未移植）③ 结果必须由本 server 执行 | **不能**：件 ①② 双缺 | W8-b ② 若宿主级会话账，只覆盖 ephemeral 面，不覆盖编辑会话 | ①②③ | L | 刀 4（最后） |
| 8 | `previewResultCellChange` | origin 重读（活句柄，同 4）+ 列源（同 5）+ `build_result_cell_change_plan`（纯，`result_edit/mod.rs:539`）——件上可组合；但 **fail-closed 成对**：commit 半边（7）不可亮则预览不可单独亮（`table_data.rs:689-692` 的登记理由） | **部分（被成对原则封条）** | 随 7 | 同 4 + 同 7 | L（随 7） | 刀 4 |

**能组合点亮几条**：前置刀（CM-63 或宿主级 ephemeral 会话）+ 活句柄面刀落地后，**一次点亮 4 条**（1/2/3/4）；再加列源 + 组装刀，**再亮 2 条**（5/6）；最后 2 条（7/8）依赖结果映射探针移植与原会话连续性，排最后。

## 5. 建议排刀顺序

1. **刀 0（前置，M，语义审查重）——跨 resolve 会话连续性**：租约源改「当前 runtime 单例直至吊销」（§3.4 R1）。理由：它是 estimate 生产可用（已登记）+ FK 四 + result-cell 二 + manual 提交的共同闸门；W8-b ② 的 ephemeral 会话若落宿主级账可与本刀并行推进但不互相替代（覆盖面不同）。评估先行，落码前需一次语义审查（吊销触发点/重建窗口）。
2. **刀 1（备料，S，可先落）——`ServerTableColumnSource`**：命令面 `get_table_schema`（`ServerTargetRunner` target 分支）→ `TableSchema` → `CachedColumns`，可挂 `SchemaCache` 缓存。理由：纯增量件、无行为面变化、不依赖 CM-63 与活句柄面；为刀 3 备料，也为 4 的列半边备选。
3. **刀 2（M，一刀四条）——活句柄面 + FK/搜索/origin 四端点**：`ServerLiveConnectionSource`（镜像 `live_source.rs` 四步，复用 `by_session` 反查与 `provider.connection_handle()`）+ 租约源 `with_live_connections` 接线 + server `ForeignKeyNavigationSource` 适配器 + 四条 handler（复用 estimate 的会话面闸序模式）+ `TableDataState` 挂 `SchemaCache`。理由：价值密度最高（4/8），共享层零改动，桌面参照逐字在案。
4. **刀 3（M，成对点亮）——变更二**：`previewPendingChanges` + `commitPendingChanges`（`ServerTableColumnSource` + commit handler 单 resolve 组装；v1 恒 owned-session，manual_handle 参数明拒或按 CM-63 进度放开）。理由：fail-closed 成对原则（预览不能单独亮）；写半边守卫（CAS/read_only/affected==1/事务序）全部已在共享层，宿主只做装配。
5. **刀 4（L，最后）——result-cell 二**：结果映射探针移植（`driver_exec` 执行期缺口）+ CM-63 的原会话面 + `commitQueryResultCell` / `previewResultCellChange`。理由：依赖面最深（原会话绑定 + 执行期元数据），且 8 中预览被 7 封条。

每刀独立 PR、落地即 push、`web-ready-plan.md` §11 台账回写（w8-handoff.md §3 纪律照守）。

## 6. 风险与开放问题

### 6.1 风险

- **半能力 fail-closed（硬原则）**：5+6 与 7+8 必须成对点亮；预览先亮 = 调用方拿到永远无法提交的计划，是半个答案（登记文案原话，`table_data.rs:689-692`）。
- **活句柄借出契约**：`open_live` 禁止新建物理连接（`live.rs:171-176`）；server 实现必须走 `by_session` 反查**已存在**资源，epoch 不匹配必须拒绝（INV-07）。借用期间会话被关闭/驱逐 → `Revoked`（`live.rs:52-54`）——FK 扫描中途作废是正确失败方向，但要进测试钉住。
- **命令面参数绑定语义**：若 `searchTableValues` 走命令面备选路，runtime 侧 `apply_params` 是宿主文本绑定（`driver_exec.rs:419`），与共享层的驱动原生绑定（`value_search.rs:251-257`）不等价——LIKE 转义/字面量语义需逐方言钉测试，否则「答案会说谎」。
- **CM-63 修复的语义审查**：租约源单例化后，吊销/重建窗口内的在途请求、`new_with_parts` 私有账（幂等回执、令牌）的生命周期都要过一遍；这不是纯机械改动。
- **写操作的面上门禁**：变更四点亮后，L4 CSRF（POST unsafe 方法）已在认证链内（`table_data.rs:195-196` 注释登记「两段式的 CSRF 令牌列随真接线生效」），审计落账复用 W8-b ③ 的 `audit_events` 半边。
- **执行期结果映射探针未移植**（`driver_exec.rs:18`）：`commitQueryResultCell` 的 `writable_mapping == Verified` 元数据没有来源——这不是装配缺口而是执行半边缺口，工作量按 L 计。
- **测试基线**：server 全量本地验证须 `--features mcp,use-case-contracts`（`web-ready-plan.md` §11 教训③）；fail-closed 探针测试（`table_data_routes.rs:757-787`）随每点亮一条改一条。

### 6.2 开放问题

1. **W8-b ② ephemeral 会话的登记账本归属**：宿主级（可跨 resolve 反查、可作 FK/estimate 会话源）还是 per-resolve runtime 私有（则仍撞 CM-63）？这决定刀 0 与 W8-b ② 是替代还是并行关系。
2. **租约源「当前 runtime 单例直至吊销」的吊销触发点**：谁在什么条件下判定吊销？重建时在途会话的失效语义按既有 `SessionLost` 走还是需要迁移窗口？
3. **web 前端 FK/值搜索的会话来源**：为每次跳转开 ephemeral 会话（TTL 秒级够一个请求窗）还是复用编辑会话？请求契约只带 `dbSessionId`（无 connectionId），会话必先存在——前端策略未定。
4. **manual_handle 提交（编辑器事务内提交）是否 v1 web 范围**：建议 v1 排除（恒 owned-session），编辑事务语义随 CM-63 修复与前端编辑器形态再定。
5. **`build_search_statement` 是否开口**：仅当值搜索走命令面备选路才需要；若刀 2 活句柄路覆盖，则保持 `pub(crate)` 不动（规则单家不受扰动）。
6. **`SchemaCache` 的 server 挂载口径**：缓存键用 connection_id + (database, schema)（`schema/cache.rs:70-75`），失效面（DDL/刷新）server 侧何时触发——变更四点亮后写操作应触发 `invalidate`（`cache.rs:319`），随刀 3 一并定。
