# W8 变更端点排刀：刀2（活句柄面 + FK/值搜索/origin 四读端点）实施级设计

> **设计日期**：2026-11-08。**基线**：main `66d2e11`（刀0 CM-63 #185 + 刀1 ServerTableColumnSource #186 均已合并）。**输入**：[w8-fk-mutation-assessment.md](w8-fk-mutation-assessment.md)（§3.3 活句柄缺口链、§4 逐端点评估表、§5 排刀建议）+ [w8-mutation-knife0-1-design.md](w8-mutation-knife0-1-design.md)（前两刀的设计档，本文与其同族）+ [w8-handoff.md](w8-handoff.md)（§3 执行纪律照守）。
> **范围**：只覆盖**刀2**——评估档 §4 表第 1–4 行的四条读端点（`resolveForeignKeyTargets` / `listInboundForeignKeys` / `searchTableValues` / `resolveResultOrigin`）与它们共同的「活句柄面」缺口。刀3（变更二成对）、刀4（result-cell 二）只在依赖关系上引用，不展开。
> **性质**：纯设计文档，零产品代码。本文每个断言指回真实 `file:line`（在基线 `66d2e11` 上逐行核实，非转录评估档）。
> **验证基线**：`cargo test -p datazen-server --features mcp,use-case-contracts`（w8-handoff §3 纪律 3）。

---

## 0. 排刀位置与依赖矩阵

评估档 §5 的序列：**刀0 CM-63（已合并）→ 刀1 ServerTableColumnSource（已合并）→ 刀2 活句柄面+四读端点（本文）→ 刀3 变更二成对 → 刀4 result-cell 二**。

### 0.1 刀2 在序列中的位置

| 刀 | 性质 | 量 | 对刀2 的关系 |
| --- | --- | --- | --- |
| 刀0（已落地） | 租约源 cell 化：`resolve()` 恒返同一 `Arc<RuntimeConnectionUseCases>` 直至吊销（`crates/datazen-server/src/execution/lease.rs:66-68,134-165`） | S/M | **硬前置**。四读端点全部携带 `dbSessionId`；「A 请求开的会话 B 请求可解析」是活句柄借出可用的底座。刀0 已兑现 |
| 刀1（已落地） | `ServerTableColumnSource` 命令面列源（`crates/datazen-server/src/execution/table_columns.rs`） | S | **无依赖**。刀1 走 target 面（`manual_handle: None`），与活句柄面正交（刀1 设计档 §2.1/§3 已声明） |
| **刀2（本文）** | 活句柄出向端口接线 + FK 二/值搜索/origin 四条 handler 点亮 | M | — |
| 刀3 | 变更二成对（preview/commit pending changes） | M | 刀2 **不是**刀3 的硬前置（刀3 走 `execute_plan` runtime 写半边，无活句柄依赖）；刀2 挂载的 `SchemaCache` 实例是刀3「写操作触发 `invalidate`」的失效接线对象（评估档 §6.2 Q6）。合并顺序照排刀不变 |
| 刀4 | result-cell 二 | L | 刀2 是刀4 的**局部前置**：`previewResultCellChange` 的「写前 origin 重读」（桌面 `preview_result_cell_change` 同款，`commands/result_origin.rs:109-171`）就是刀2 的 origin 编排；但其 commit 半边的执行期结果映射探针缺口（`driver_exec.rs:18` 登记）不在刀2 范围 |

### 0.2 独立可合并性声明

刀2 的改动面：server crate（新件 2 个 + 改件 4 个）+ 共享 crate 的 **feature 门刻痕修复**（§2.6 裁决，cfg 门 plumbing，零规则移动）。不依赖刀3/刀4 的任何件；刀3/刀4 合并在前后均不需 rebase 语义。

---

## 1. 问题定义：四条 501 与它们撞的缺口

### 1.1 fail-closed 现场（基线 `66d2e11`）

`crates/datazen-server/src/http/domains/table_data.rs` 的 8 条 ⛔ 端点中，归刀2 的四条：

| 方法 | 路由 | handler（现状） | 登记原因 |
| --- | --- | --- | --- |
| `resolveForeignKeyTargets` | `POST /api/v1/table-data/foreign-key-targets` | `handler_resolve_foreign_key_targets`（`table_data.rs:699-701`） | `REASON_FK_SOURCE`（`:679-682`） |
| `listInboundForeignKeys` | `POST /api/v1/table-data/inbound-foreign-keys` | `handler_list_inbound_foreign_keys`（`:703-707`） | `REASON_FK_SOURCE`（同上） |
| `searchTableValues` | `POST /api/v1/table-data/value-searches` | `handler_search_table_values`（`:709-712`） | `REASON_DRIVER_QUERY`（`:683-685`） |
| `resolveResultOrigin` | `POST /api/v1/table-data/result-origins` | `handler_resolve_result_origin`（`:714-717`） | `REASON_ORIGIN`（`:686-688`） |

501 形状：`fail_closed`（`:666-677`）→ `capabilityUnsupported` + `with_status(NOT_IMPLEMENTED)`，文案携带「No statement ran, no plan was built and nothing was written」无副作用声明。路由挂载（`:179-194`）与认证链（L2–L6，`bootstrap.rs:421-489` 的 merge 循环保证）不动。

### 1.2 共同缺口链：活句柄出向端口未接线

四条端点的共享服务层**全部已存在且单家**（规则零复制）：

- FK 二条吃 `ForeignKeyNavigationSource` 端口（`crates/datazen/src/services/ports.rs:66-70`）：`bind_session(&self, db_session_id) -> FkSessionBinding`，绑定 = `driver + handle + database/schema 档案回退 + guard`（`ports.rs:45-58`）。共享解析器 `resolve_foreign_key_targets`（`services/fk_navigation.rs:264-269`）与 `list_inbound_foreign_keys`（`services/referenced_by.rs:123-133`）拿绑定后做三条/最多 200+1 条驱动往返（上限 `MAX_INBOUND_SCAN_TABLES = 200`，`referenced_by.rs:39`）。
- 值搜索直接吃活句柄：`search_table_values(driver, handle, database, schema, request)`（`services/value_search.rs:213-219`）——签名第二参就是 `&ConnectionHandle`。
- origin 吃驱动契约 + 目录缓存：`DatabaseDriver::describe_result_origin`（`crates/datazen-driver-api/src/traits.rs:500-508`，默认实现 `traits/result_origin.rs:22-35`）+ `SchemaCache::get_columns`（`crates/datazen/src/schema/cache.rs:92-130`）+ `evaluate_result_editability`（`crates/datazen/src/table_editing/result_edit/mod.rs:210-215`）。

它们全部撞同一个 server 侧缺口——**活句柄出向端口未接线**：

1. 端口契约在共享层：`LiveConnectionSource::open_live(LiveConnectionRequest) -> LiveConnection`（`crates/datazen/src/execution/application/orchestration/live.rs:167-177`），硬规则「实现方**不得**新建物理连接」（`:171-176`）；借用作废（`LiveConnectionLedger::revoke`，`:244-253`）与 RAII 归还（`LiveConnectionLease`，`:284-333`）都在。
2. runtime 的借出门面在：`RuntimeConnectionUseCases::borrow_live_connection_by_id(ctx, db_session_id, purpose)`（`orchestration/mod.rs:382-393`）——授权（`:343` 的 `authorize(ctx, Query)`）→ 会话视图 → 实例登记行 → epoch 校验后才向端口要引用（`borrow_live_connection`，`:337-355`）。
3. **server 没接线**：`RuntimeConnectionUseCases::new_with_parts` 构造时 `live: Mutex::new(LiveConnections::unwired())`（`orchestration/mod.rs:225`）；唯一的接线入口 `with_live_connections`（`:237-240`）在 server crate 内**零调用者**（基线 `66d2e11` 全文检索为空）；`ServerRuntimeLeaseSource::resolve()`（`lease.rs:134-165`）构造 runtime 后不接线。未接线时 `LiveConnections::borrow` 返回 `NotWired`（`live.rs:390-394` + `:369-374`）。
4. 但零件全在 server 侧：`ServerDriverExecBackend.by_session`（dbSessionId → resource_id 反向索引，`crates/datazen-server/src/execution/driver_exec.rs:107-108`）在 `open_resource` 里与资源登记**同帧写入**（`:325-329`，共享层 `OpenResource.db_session_id` 是「出向端口反向索引的唯一来源」，`crates/datazen/src/execution/runtime/registry/backend.rs:45-55`）；反查入口 `live_resource_for_session(db_session_id, runtime_epoch)`（`driver_exec.rs:161-171`，epoch 不匹配拒绝 = INV-07）**已经存在**。

### 1.3 桌面参照面（活句柄如何被消费，逐字在案）

- **接线点**：桌面在装配期把 `DesktopLiveConnectionSource` 接进 runtime——`runtime.with_live_connections(live.clone())`（`crates/datazen-desktop/src/platform/adapter.rs:519`；live source 构造 `:493-497`）。
- **适配器四步**（`crates/datazen-desktop/src/platform/live_source.rs:104-150`）：①`backend.live_resource_for_session(db_session_id, epoch)` 反查物理资源（miss → `SessionLost`）；②权威 driver type（profile 是唯一权威，`:94-99`；取不到必须拒绝，不许「随便挑一个驱动」）；③`registry.get(driver_type)` 取**同一**驱动 `Arc`（`:137-141`；同实例性质由驱动工厂保证并钉测试，模块头 `:19-34`）；④`resource.driver_handle()` 借出句柄（`:143-148`）。
- **消费方借出**：桌面命令经 `resolve_command_driver` → `runtime.borrow_live_connection_by_id(ctx, id, LivePurpose::CommandExecution)`（`commands/driver_command/resolve.rs:45-58`）→ `lease.connection()` 取 `(driver, handle)`（`:59-68`）→ `guard: Box::new(resolved._lease)` 把租约装进 `FkSessionBinding.guard`（`commands/fk_navigation.rs:52-60`）。
- **origin 编排**（`commands/result_origin.rs:66-102`）：借出 → 连接档案 → `sql_target` 梯子（`:255-271`）→ `describe_result_origin`（`:177-193`，驱动答不出**不是错误**）→ `source_table_columns` 经 `SchemaCache.get_columns` + `LegacyCatalogReader` + 同一活句柄（`:199-236`）→ `evaluate_result_editability`（`:96-101`）。

---

## 2. 修复方案

### 2.1 新件 1：`ServerLiveConnectionSource`（`crates/datazen-server/src/execution/live_source.rs`）

镜像桌面 `live_source.rs` 四步，server 事实源替换：

```rust
pub struct ServerLiveConnectionSource {
    backend: Arc<ServerDriverExecBackend>,   // live_resource_for_session（driver_exec.rs:163-171）
    connections: Arc<ServerConnections>,     // 权威 database_type（ConnectionConfig）
    registry: Arc<DriverRegistry>,           // 同实例驱动 Arc
    scope: ConnectionScope,                  // 构造注入（v1 单组织裁决，与 backend 同款）
}

#[async_trait]
impl LiveConnectionSource for ServerLiveConnectionSource {
    async fn open_live(&self, request: LiveConnectionRequest)
        -> Result<LiveConnection, LiveConnectionError> { /* 四步 */ }
}
```

四步（对齐桌面 `live_source.rs:104-150` 的顺序与失败方向）：

1. **反查物理资源**：`backend.live_resource_for_session(request.handle.db_session_id.as_str(), request.handle.runtime_epoch.get())` → `None` → `LiveConnectionError::SessionLost(format!("no live physical resource is bound to session {id} at epoch {epoch}"))`。绝新建连接、绝不透明替换（端口契约 `live.rs:171-176`；INV-07 epoch 比对在 `driver_exec.rs:170` 内已完成）。
2. **权威 driver type**：`connections.get(&self.scope, request.connection_id.as_str())`（`ConnectionRepository::get`，桌面镜像 `fk_navigation.rs:44-51`）→ miss → `Refused("connection profile {id} is unavailable; refusing to guess a driver")`；取 `config.database_type`。**与桌面的差异**：桌面 adapter 无 ctx 可用而直查本地 store；server 的 `ServerConnections::get` 吃 `ConnectionScope`（构造注入，与 `ServerDriverExecBackend` 的 v1 单组织裁决同款，`driver_exec.rs:26-29` + `bootstrap.rs:344-351`），不经 `ServerProfiles::get(ctx, …)`——`LiveConnectionRequest`（`live.rs:121-126`）不携带主体事实，且组织/主体闸已由借出路径完成（`mod.rs:343-345` 的 authorize + session_entry 三字段过滤，`mod.rs:589-597`）。
3. **取同一驱动 Arc**：`registry.get_sql_driver_by_name(&driver_type)`（`crates/datazen/src/execution/runtime/driver_registry/registry.rs:399-401`，即 `get` 的按名薄壳 `:339-346`）→ miss → `Refused("driver {type} is not loaded; refusing to substitute another driver")`。**同实例性质**是驱动工厂的义务（桌面模块头 `live_source.rs:24-34` 逐字适用：`factory.create()` 与 `factory.resource_provider()` 必须共享进程内单例，否则该驱动不得接此端口）——进测试钉住（t11）。
4. **借出句柄**：`resource.driver_handle()` → `LiveConnection::new(driver, handle)`。

**前置小改件**：server 的 `driver_exec.rs::Resource`（`:85-94`）字段私有且**没有**句柄出口——补一个 accessor，镜像桌面 `session_backend.rs:68-72`：

```rust
// crates/datazen-server/src/execution/driver_exec.rs（Resource impl 块，新增）
pub(crate) fn driver_handle(&self) -> Result<datazen_driver_api::ConnectionHandle, dr::ResourceError> {
    self.provider.connection_handle(&self.handle)
}
```

（`ResourceProvider::connection_handle` 契约：「Borrow the already-held driver's handle」，`crates/datazen-driver-api/src/resource.rs:596-599`；`LegacyResourceAdapter` 与 sqlite/mysql/mongodb 原生实现均在，评估档 §3.3 事实链 1。）

### 2.2 接线形状裁决：租约源 cell 内每实例装配（不是 bootstrap 全局装配）

**裁决**：`ServerRuntimeLeaseSource` 增设可选 `live_source` 字段，`resolve()` 慢路构造 runtime 后立即接线：

```rust
// crates/datazen-server/src/execution/lease.rs
pub struct ServerRuntimeLeaseSource {
    // …既有件不动…
    /// 活连接出向端口适配器（刀2）：Some 时每个新造 runtime 实例构造后
    /// 立即 with_live_connections 接线；None 保持 unwired（fail-closed）。
    live_source: Option<Arc<dyn LiveConnectionSource>>,
}

pub fn with_live_source(mut self, source: Arc<dyn LiveConnectionSource>) -> Self { … }

async fn resolve(&self) -> … {
    // …快路不动…
    let runtime = Arc::new(RuntimeConnectionUseCases::new_with_parts(/* :151-162 逐参一致 */));
    if let Some(live) = &self.live_source {
        runtime.with_live_connections(live.clone());   // orchestration/mod.rs:237-240
    }
    *cell = Some(Arc::clone(&runtime));
    Ok(runtime)
}
```

**理由**（为何不是「bootstrap 全局装配」）：

1. **实例出生地在 `resolve()`**：server 的 runtime 不是全局单例变量，而是租约源 cell 里的「当前实例」；接线入口 `with_live_connections`（`orchestration/mod.rs:237-240`）是实例方法——宿主能摸到新生实例的唯一位置就是租约源自己的慢路。桌面能在外部接线是因为桌面 runtime 在装配期一次性构造（`adapter.rs:498-519`）；server 的懒构造形状决定了接线必须跟着构造走。
2. **吊销→重建自动重接**：`revoke()` 置空 cell（`lease.rs:120-125`），下一次 `resolve()` 重建新实例——接线写在慢路里，重建实例**天然带端口**；若在 bootstrap 外部「resolve 一次再接线」，重建窗口后会退化回 NotWired（且与懒构造时序竞态）。
3. **共享层零改动**：`new_with_parts` 签名不动（不给它加 live 参数），`with_live_connections` 的「覆盖而非叠加」语义（`mod.rs:236`）保证慢路重入无副作用。
4. **缺省 fail-closed**：字段 `Option`、不注入即 unwired——既有构造器（`new` / `new_with_shared_parts`）与全部既有测试零改动，行为与基线逐字节一致；未接线时借出路径得到 `NotWired` → `convert::live` 映射 `ServiceUnavailable`（`orchestration/convert.rs:168-179`），是诚实失败而非静默。

**bootstrap 装配**（`crates/datazen-server/src/bootstrap.rs`）：

```rust
// :344-351 之后（backend 尚未被 lease source 移动，clone 一份）
let live_source = std::sync::Arc::new(
    crate::execution::live_source::ServerLiveConnectionSource::new(
        backend.clone(), connections_repo.clone(), driver_registry.clone(),
        datazen::connection::domain::ConnectionScope::new(config.organization_id.clone()),
    ),
);
// :367-377 的 new_with_shared_parts(...) 链上 .with_live_source(live_source)
```

### 2.3 活句柄借出的生命周期（与刀0 语义的组合）

一次四读请求的完整生命周期：

1. **resolve（每调用现取）**：handler 经 `state.lease_source.resolve()`（§2.4）拿到**当前** runtime 实例——刀0 cell 语义保证它与开会话的实例是同一个 `Arc`（`lease.rs:134-165`），这是「A 请求开的会话 B 请求能借」的根据。
2. **borrow（授权 + 三重校验后）**：`borrow_live_connection_by_id`（`mod.rs:382-393`）内部 authorize（`Query`）→ `session_view`（owner 三字段过滤 + 目录 resolve + profile revision CAS，`mod.rs:606-639` 的 `:629-635`）→ `session_entry`（`mod.rs:584-605`）→ `LiveConnections::borrow`（`live.rs:390-403`）：**登记与签发同时成功或同时失败**（适配器报错不登记，`live.rs:388-389` 注释）。
3. **purpose**：四条全部 `LivePurpose::CommandExecution`（桌面 `resolve_command_driver` 同款，`resolve.rs:49`；`LivePurpose` 词汇 `live.rs:97-115`）。
4. **用完归还（RAII，唯一归还路）**：借出物 `LiveConnectionLease` 的显式 `release` 与 `Drop` 落同一私有例程（`live.rs:312-326` + `:329-333`），取消/超时/panic/`?` 提前返回全部自动归还。三条消费形：
   - **FK 二条**：租约装进 `FkSessionBinding.guard: Box<dyn Any + Send + Sync>`（`ports.rs:57`）——`Box::new(lease)`（桌面镜像 `fk_navigation.rs:59`）。绑定活过解析器的全部驱动往返（`resolve_foreign_key_targets` 三条，`fk_navigation.rs:264-430`；`list_inbound_foreign_keys` 最长 ≤200+1 次 schema 读，`referenced_by.rs:39,152-241`），函数返回即绑定 drop → lease drop → 归还。端口文档的硬要求「binding must outlive every driver call」（`ports.rs:41-44`）由「guard 就是租约本体」结构性满足。
   - **值搜索**：一条 COUNT 查询（`value_search.rs:250-258`），lease 活在 handler 作用域，返回即归还。
   - **origin**：两条驱动读（describe + 目录缓存 miss 时的 `catalog.get_columns`），同上。
5. **借出期间会话死亡**：显式 `close_session` 的关闭路径**先作废活连接借用**（`orchestration/operations.rs:728-742`，位置刻意早于 `directory.release`——模块注释 `:730-732`）；之后 `lease.connection()` 返回 `Revoked`（`live.rs:301-310`），持有者必须走执行路径拿 `SessionLost` 而不能拿看似可用的句柄打驱动（`live.rs:47-54` 模块契约）。FK 长扫描中途被作废是**正确失败方向**（评估档 §6.1）——t6 钉住。server v1 未武装 IdleReclaimer（刀0 设计档 §1.5 风险 3：`idle.rs:235-252` 桌面有、server 未接线），扫描窗内不存在空闲驱逐打断。
6. **观测出口**：`runtime.live_connection_borrows()` / `live_connection_rows()`（`mod.rs:265-271`）——「异常路径不留登记行」断言的现成投影（t7）。

**与刀0 的组合语义一句话**：刀0 让「会话」跨 resolve 可见（cell 单例），刀2 让「会话身后的物理资源」跨请求可借（端口接线）；两者都尊重「当前实例可能被吊销」——吊销后新实例上旧会话 `SessionNotFound`（`cm63_lease_continuity.rs` t5 已钉），借出端口随重建自动重接（§2.2 理由 2）。

### 2.4 四读端点逐条点亮方案

**共同面**（全部落在新文件 `crates/datazen-server/src/http/domains/table_data_live.rs`；路由仍由 `table_data.rs` 的 `table_data_router` 挂载，handler 换引用——模块头状态表 `table_data.rs:5-31` 同步刷新）：

- **`TableDataState` 增两字段**（`table_data.rs:108-133`）：
  ```rust
  /// 租约源（刀2）：四读端点 resolve 当前 runtime 后借活句柄；与 sessions
  /// 域 / runner / resource host 同一实例（bootstrap 既有纪律）。
  pub lease_source: Arc<dyn crate::execution::lease::RuntimeLeaseSource>,
  /// 目录缓存（刀2）：origin 的列/主键读通路径（cache.rs:92-130）。
  pub schema_cache: Arc<datazen::schema::cache::SchemaCache>,
  ```
- **共享借出助手**（table_data_live.rs 内，estimate 闸序模式 `table_data.rs:562-588` 的活句柄版）：
  1. 请求体反序列化（共享层 DTO 直接吃 camelCase wire）+ 信封预检（镜像桌面各命令的 required 文案）；
  2. **目录可见性闸**：`state.directory.lookup_owner(DbSessionId)` → `Ok(None)` 或越组织 → `not_visible()` 404 固定文案（`table_data.rs:564-580` + `:650-653`，sessions 域防枚举裁决逐字落地）；
  3. `state.lease_source.resolve().await`（err → 503，`LeaseConnectionUseCases::resolve` 同款，`lease_use_cases.rs:44-52`）；
  4. `runtime.borrow_live_connection_by_id(&identity.ctx, &db_session_id, LivePurpose::CommandExecution)` → `ApiError` 直接 `api_error_response`（`SessionNotFound`→409、`ConfigRevisionMismatch`→409、授权拒→403，状态码表 `http/response.rs:72-118`）——闸序已把「未知/越组织」收窄成 404，409 只剩真实竞态与配置漂移，是诚实失败。

#### 2.4.1 `resolveForeignKeyTargets`（FK 跳转，DB-07）

- **请求体**：`datazen::services::fk_navigation::ResolveForeignKeyTargetsRequest`（`fk_navigation.rs:123-163`，serde camelCase——`dbSessionId/database/schema/table/qualifiedTable/columns/constraintName/values/limit/browse`）。
- **闸序**：信封预检 `dbSessionId`/`table` 非空（桌面 `fk_navigation.rs:73-78` 同文案 → 400 `InvalidArgument`）→ 共享借出助手。
- **执行路径**：每请求构造 `ServerForeignKeyNavigationSource`（table_data_live.rs 内，镜像桌面 `HostNavigationSource`，`fk_navigation.rs:23-62`）：
  ```rust
  struct ServerForeignKeyNavigationSource { /* lease_source, connections, scope, ctx */ }
  #[async_trait]
  impl ForeignKeyNavigationSource for ServerForeignKeyNavigationSource {
      async fn bind_session(&self, db_session_id: &str) -> Result<FkSessionBinding, FkSessionError> {
          // resolve → borrow_live_connection_by_id(CommandExecution)
          //   ApiError SessionNotFound/NotFound → FkSessionError::NotFound；
          //   其余 → FkSessionError::Unavailable（桌面 resolve.rs:52-58 映射同族）
          // lease.connection() → (driver, handle)
          // connections.get(&scope, session.connection_id) → config（database/schema 档案回退）
          // Ok(FkSessionBinding { driver, handle, database: config.database,
          //                       schema: config.schema, guard: Box::new(lease) })
      }
  }
  ```
  然后 `datazen::services::fk_navigation::resolve_foreign_key_targets(&source, &request)`（`fk_navigation.rs:264-269`）原样跑——三条往返（源表 FK 元数据 `get_table_schema`、目标表元数据、存在探针/浏览）共享一次绑定（端口文档 `ports.rs:60-65`）。
- **响应形状**：200 `ForeignKeyTargetResolution`（`fk_navigation.rs:196-207`，serde camelCase；`exists:false` 是成功答案——契约 §6.7 `docs/architecture/platform/http-api-contract.md:140`）。
- **错误映射**（镜像桌面 `commands/error.rs:590-606` 逐臂）：`FkNavigationError::Coded{code,message}` → 400 `InvalidArgument`，文案 `{code}: {message}`（`grid.fk.*` 前缀是前端分类契约，`fk_navigation.rs:65-76`）；`Session(NotFound)` → 404 `NotFound`；`Session(Unavailable)` → 503 `ServiceUnavailable`（server 码表最近的诚实类别，`table_read_error_response` 的 Internal 桶同款裁决，`table_data.rs:418-421`）。

#### 2.4.2 `listInboundForeignKeys`（DB-20）

- **请求体**：`datazen::services::referenced_by::ListInboundForeignKeysRequest`（`referenced_by.rs:42-58`）。
- **闸序**：信封预检 `table` 非空（桌面 `fk_navigation.rs:96-101`）→ 共享借出助手 → **同一个** `ServerForeignKeyNavigationSource`（同一端口同一适配器，评估档 §4 表第 2 行「零边际」）。
- **执行路径**：`datazen::services::referenced_by::list_inbound_foreign_keys(&source, &request)`（`referenced_by.rs:123-133`）——最长活句柄持有面（≤200 次 schema 读 + 如实 `truncated`，`:33-39`）。
- **响应形状**：200 `InboundForeignKeyCatalog`（`referenced_by.rs:77-90`）。
- **错误映射**：与 2.4.1 同一映射表。

#### 2.4.3 `searchTableValues`（DB-19）

- **请求体**：`datazen::services::value_search::SearchTableValuesRequest`（`value_search.rs:45-65`）。
- **闸序**：信封预检 `table`/`value` 非空（桌面 `value_search.rs:31-36`）→ 共享借出助手。
- **执行路径**：直接在 handler 里编排（此服务吃裸 `(driver, handle)`，无宿主端口）：
  1. `lease.connection()` → `(driver, handle)`；
  2. `state.connections.get(&scope, session.connection_id)` → config（桌面 `value_search.rs:45-49`）；
  3. database 梯子 = **estimate 同款共享梯子**：`table_read_database(request.database.as_deref(), config.database.as_deref())`（`crates/datazen/src/schema/application/scope.rs:76-85`，request → config → `"default"`；桌面 `value_search.rs:54-60` 的内联梯子与它逐字同语义，server 复用共享份不复制字面量）；
  4. schema 梯子：`metadata_schema(driver, request.schema, embedded_schema_of(&request.table), config.schema)`（`scope.rs:49-57`；estimate 同款 `table_data.rs:619-624`）；
  5. `datazen::services::value_search::search_table_values(driver.as_ref(), &handle, database, schema.as_deref(), &request)`（`value_search.rs:213-280`）——语句构建/LIKE 转义/绑定或内联/计数解析全在共享层。
- **响应形状**：200 `SearchTableValuesResult`（`value_search.rs:68-77`）。
- **错误映射**：`ValueSearchError` → 400 `InvalidArgument`，文案 `error.to_string()`（`grid.valueSearch.shapeMismatch/queryFailed` 前缀，`:89-96`；桌面 `value_search.rs:79` 同映射）。扫描节奏在前端（一表一命令，桌面命令头 `value_search.rs:11-13`），host 永不自行迭代——v1 无分页/批量面。

#### 2.4.4 `resolveResultOrigin`（DB-14 读半边）

- **请求体**：server 自定义 `ResolveResultOriginBody`（桌面 `ResolveResultOriginRequest` 的镜像，`commands/result_origin.rs:37-60`——该 DTO 是桌面命令的 IPC 类型不在共享层，server 域文件自持一份同字段反序列化结构：`dbSessionId/database/schema/sql/resultColumns/table`）。
- **闸序**：信封预检 `dbSessionId`/`sql` 非空（桌面 `validate_envelope`，`result_origin.rs:312-320`）→ 共享借出助手。
- **执行路径**（镜像桌面 `resolve_result_origin` 六步，`:67-102`）：
  1. `lease.connection()` → `(driver, handle)`；
  2. `connections.get(&scope, session.connection_id)` → config（miss → `profile_unavailable()` 404，`table_data.rs:400-405`）；
  3. `sql_target` 梯子：database = request → config（非空过滤）；schema = request → config，且**两处都过 `driver.has_schema_level()` 过滤**（桌面 `sql_target` `:255-271`）→ `SqlTarget::new(database.as_deref(), schema.as_deref())`；
  4. `driver.describe_result_origin(&handle, target, &result_columns, &sql)`（traits.rs:500-508）——驱动答不出**不是错误**：映射 400，文案 `{read_only_error_code(UnknownOrigin)}: the driver cannot describe where this result came from (…)`（桌面 `:187-192` 同族；码字 `grid.result.originUnavailable`，`result_edit/mod.rs:85-87,374-388`）；
  5. **列/主键半边走 `state.schema_cache`**：table = `origin_table_name(origin)`（Determined 分支取 `detail.table.name`，桌面 `:239-248`）或窄映射 `request.table`；database/schema 梯子同 3（`:214-222`）；`LegacyCatalogReader::new(driver)`（`crates/datazen-driver-api/src/catalog.rs:78,95`）+ `state.schema_cache.get_columns(&config.id, database, schema, &table, &catalog, &handle)`（`cache.rs:92-130`）→ `.ok()` 收敛为 `Option<CachedColumns>`——**None = 「目录答不出」不是错误**，裁决按「nothing is provable」只读处理（桌面 `:197-199` 注释逐字）；
  6. `evaluate_result_editability(&origin, &primary_keys, &result_columns, !config.read_only)`（`result_edit/mod.rs:210-215`）。
- **响应形状**：200 `ResultEditability`（`result_edit/mod.rs:118-138`，serde camelCase；只读裁决带 `reason` 也是 200 成功答案——「verdict plus a reason — never a bare no」）。
- **规则单家审计**：origin 的编排胶水（borrow → describe → cache → evaluate）在桌面是命令层宿主胶水（`commands/result_origin.rs`），server 镜像同形状落域文件——**规则**（origin 分析、可写性判定、码表）全部单家在 `datazen-driver-api::result_origin` 与 `table_editing::result_edit`，宿主只借会话与读缓存，无规则复制。

### 2.5 SchemaCache 挂载口径（评估档 §3.1 / §6.2 Q6 的裁决）

| 项 | 裁决 | 依据 |
| --- | --- | --- |
| 实例归属 | `TableDataState.schema_cache: Arc<SchemaCache>`，bootstrap 全局一个实例（`Arc::new(SchemaCache::new())`） | 缓存键第一维是 connection_id（`cache.rs:83` + `get_columns` 形参），天然按连接分桶，无跨连接串读；单进程单实例与桌面 `state.schema_cache` 同形 |
| TTL | **默认 300s**（`cache.rs:55`），不改 | 与桌面同口径；origin 是渲染期裁决，写路径（刀4）本来就要重读 origin（桌面 `result_origin.rs:104-107`「a write must never be based on a cached yes」）；列陈旧最坏 300s 且只影响 verdict 的 key 列事实，不影响 read-only 方向的安全（多给的可写性只会因**漏** key 而收紧，不会因陈旧而放宽——`evaluate_result_editability` 的 key 来源是目录规范化列表，`result_edit/mod.rs:200-209`） |
| 键 | connection_id + (database, schema) scope + table（`cache.rs:70-75`——schema 在键里不是装饰，`public.users` 与 `other.users` 是两张表） | 既有实现，零改动 |
| 失效接线 | **不在刀2**：DDL/刷新 → `invalidate`（`cache.rs:319-346`）的触发点随刀3 变更端点点亮一并定（评估档 §6.2 Q6 原案）；刀2 只挂读通路径 | 刀2 四条全是读；无写半边就无失效语义可定 |
| server feature | 无需 Cargo.toml 增门：`schema` feature 经 `table-browsing`（server 已开，`crates/datazen-server/Cargo.toml:30`）传递点亮（`crates/datazen/Cargo.toml:199-207` 的 `"table-browsing" = [..., "schema", ...]`；`lib.rs:100-101` 的 cfg 门随 features 图放行） | features 图核实 |

### 2.6 `services` feature 门与 server 依赖闭包（裁决：cfg 刻痕修复，不是整包开启）

**新事实（基线核实）**：共享服务模块门在 `#[cfg(feature = "services")] pub mod services;`（`crates/datazen/src/lib.rs:20-21`），而 datazen-server 的 feature 集不带 `services`（`crates/datazen-server/Cargo.toml:30`；`table_data.rs:86-88` 模块头也登记过这一现状——读三端点当年为 `validate_filters` 直连 executor 底座而绕开了 `services` 门）。四读端点要吃的规则单家（`fk_navigation`/`referenced_by`/`value_search`/`ports`）都在这个门后，**必须开门**；但现状的 `services` feature 定义有缺陷：

1. `services = [..., "ai", "feature_error", "schema", "table-browsing", "table-editing"]`（`crates/datazen/Cargo.toml:54-64`）——`ai` 被**硬编码进 def**，开门即把 `ai` 模块连同 `reqwest`/`serde_yaml`/`regex`/`inventory`/`tokio-util`/`futures-util` 拖进 server 闭包，违背本 crate 自己的装配纪律（`crates/datazen-server/Cargo.toml:24-29`「按域点亮……不整包开启」）。
2. `services/ports.rs:17` re-export `crate::backup::ports::…`，而 `backup` 模块门独立（`lib.rs:10-11`）且**不在 services def 里**——`services` 单独开门（无 backup）今天**根本编不过**：这是 feature 图的潜伏欠账（桌面是唯一消费者且全 feature，从未暴露）。

**裁决：刻痕修复（共享层三处 cfg plumbing，零规则移动）**：

```rust
// crates/datazen/Cargo.toml —— services def 去掉 "ai"（其余不动）
services = ["dep:async-trait", "dep:datazen-driver-api", "dep:serde_json", "dep:sha2",
            "dep:sqlparser", "dep:tokio", "feature_error", "schema",
            "table-browsing", "table-editing"]

// crates/datazen/src/services/mod.rs:12-20 —— 三个 ai 系 re-export 挂回自己的门
#[cfg(feature = "ai")]
pub mod command_policy { … }
#[cfg(feature = "ai")]
pub mod db_tools { … }
#[cfg(feature = "ai")]
pub mod schema_context { … }

// crates/datazen/src/services/ports.rs —— ai/backup 两条 re-export 挂回自己的门
#[cfg(feature = "ai")]
pub use crate::ai::application::ports::{DatabaseToolHost, SessionConnectionLookup};
#[cfg(feature = "backup")]
pub use crate::backup::ports::{BackupOutputBinder, BackupOutputGuard};
// （schema/ports 与 table_editing/ports 两条不动：schema、table-editing 都在 services def 内）
```

- server 侧：`crates/datazen-server/Cargo.toml:30` 的 features 追加 `"services"` **一项**。
- **安全性核实**：`services::db_tools`/`schema_context`/`command_policy` 三条 re-export 路径的唯一消费者是桌面（`datazen-desktop/src/services/db_tools_tests.rs:5`、`src/mcp/server.rs:256-272`），而桌面 feature 集同时带 `ai` + `services`（`crates/datazen-desktop/Cargo.toml:26`）——cfg 门对桌面恒开，零行为变化。`check-server-deps.mjs` 的闭包门只禁宿主/UI crate（`scripts/check-server-deps.mjs:44-58`），`reqwest` 本来就不在禁列——但按本裁决 server 闭包根本不引入它。
- **被否备选**：`features += ["services", "backup"]`（让现状 def 直接编过）——否，理由：把 `reqwest` + `flate2` + 两个死模块拖进 server 闭包，与装配纪律冲突；且不修 feature 图欠账，欠账留给下一个开门的人。
- **验证补充**：实现 agent 需新增一条本地检查 `cargo check -p datazen --features services`（修复后 `services` 必须能独立编过——这条在修复前是红的，修复后转绿，本身就是刻痕修复的回归钉）；桌面腿交 CI。

---

## 3. 语义审查点（落码前必须过的三问）

1. **借出是否可能变成「新建连接的后门」？** 不会。`open_live` 的第一步只读 `by_session` 反查 + `resources` 表（`driver_exec.rs:161-171`），不存在 acquire 调用路径；端口契约（`live.rs:171-176`）与桌面同款测试钉（t11：反查 miss / epoch 漂移 / 驱动缺席三拒绝腿）。驱动 `Arc` 的同实例性质是工厂义务（§2.1 步 3），违约驱动不得接端口——这不是运行时可兜底的事，是装配事实。
2. **guard（租约）会不会被共享层提前丢掉或过量持有？** 不会丢：`FkSessionBinding.guard` 是租约本体（`Box::new(lease)`），绑定活到解析器返回，RAII 归还（`live.rs:312-333`）——没有第二条归还路，也没有「宿主替 runtime 多借一次」的可能（模块头四句话契约，`live.rs:15-25`）。不会过量：一次请求恰好一次 borrow；并发请求各自 borrow（`ledger.borrow` 计数允许同会话多笔在借，`live.rs:204-213`），各还各的。
3. **借出窗内的会话死亡/配置漂移把请求带向哪里？** 死亡 → `revoke`（`operations.rs:728-742`）→ `lease.connection()` 返回 `Revoked` → 错误映射 `SessionLost` 409（`convert.rs:171-173`）——宁可让调用方走执行路径拿 `SessionLost`，不许拿看似可用的句柄打驱动（`live.rs:47-54`）。配置漂移 → `session_view` 的 revision CAS 先拒（`mod.rs:629-635` `ConfigRevisionMismatch` 409）——借出成功蕴含「会话此刻仍登记 + 配置未漂移」，与执行路径同源（`mod.rs:331-336` 注释）。两问的答案都与评估档 §6.1 的预期失败方向一致，t5/t6/t7 钉住。

---

## 4. 回归风险表

| # | 风险 | 评估 | 缓解 |
| --- | --- | --- | --- |
| 1 | 租约源接线改动波及既有消费者（runner/host/MCP/sessions 域） | 低：`live_source` 字段缺省 `None`，既有构造器与测试零改动；接线只发生在显式 `with_live_source` 的实例上 | 基线全量先行（t0）；`new`/`new_with_shared_parts` 逐字节不变 |
| 2 | server 全局多出一条「活句柄可借」面，被误当成连接所有权凭证 | 低：端口类型系统已封——`LiveConnection` 无 `into_driver`/`take`（`live.rs:128-152`），lease 强制归还 | 模块契约已写死（`live.rs:15-61`）；t7 断言台账归零 |
| 3 | inbound FK 长扫描（≤200 读）期间物理资源被独占 | 与桌面同权：`MAX_INBOUND_SCAN_TABLES` 封顶（`referenced_by.rs:33-39`）；server v1 无池无空闲驱逐（`driver_exec.rs:16` 缺口登记 + 刀0 风险 3），不存在「扫描中被回收」的新面 | 接受；`truncated` 如实报告早停 |
| 4 | `services` feature 刻痕修复波及桌面编译 | 低：桌面 `ai`+`services` 同开（`Cargo.toml:26`），cfg 门恒真；三条 re-export 的消费者仅桌面 | `cargo check -p datazen --features services` 新钉 + 桌面 CI 4/4 闸 |
| 5 | `SchemaCache` 内存增长 | 低：TTL 300s + 每 scope `max_tables=1000` LRU 简易淘汰（`cache.rs:56,311-317`）；单组织单实例部署 | 接受；失效接线随刀3 |
| 6 | fail-closed 探针翻新削弱防回归面 | 刻意变更：四条从「501 探针」转为「绿腿探针」，变更四的探针面不变 | t9/t10：probe 缩为 change-commits 一条照旧绿 + 新文件绿腿全栈 + 变更四 501 逐条断言 |
| 7 | origin 的缓存列读在 DDL 后给出陈旧 key | 与桌面同权（桌面同一 300s 缓存形状）；verdict 安全方向不受陈旧影响（§2.5 TTL 行） | 接受；失效接线随刀3（评估档 §6.2 Q6） |
| 8 | 请求体直用共享 DTO 导致 wire 面意外字段 | 低：三个共享 DTO 本就是 `Deserialize` + camelCase，桌面 IPC 与 HTTP 同用例两绑定（w8-handoff §1.2 的既有原则）；origin 的 DTO server 自持（桌面 IPC 类型不跨家） | t1–t4 的请求体逐字段断言 |

---

## 5. 测试计划

新文件 `crates/datazen-server/tests/table_data_live_routes.rs`（夹具沿 `table_data_routes.rs::wired_fixture` / `cm63_lease_continuity.rs` 的既有模式：真 SQLite 管理库 + `WireMockFactory`（`table_data_routes.rs:827-873`）+ `ServerDriverExecBackend` + **真** `ServerRuntimeLeaseSource::new_with_shared_parts(..).with_live_source(..)` + `LeaseConnectionUseCases`；`TableDataState` 带 `lease_source` 与 `schema_cache` 两新字段；`MockDriver` 的 FK/表/查询事实用 `set_table_schema_for_test`（`mock_driver.rs:434`）/ `tables_by_database` / 预录行装配——共享层 fk 测试同款夹具词汇，`services/fk_navigation/tests.rs:23-66`）。

- **t0 基线**：改动前 `cargo test -p datazen-server --features mcp,use-case-contracts` 全量，记录绿名单（含 `table_data_routes` 21 例）。
- **t1 FK 绿腿（全栈）**：开会话（真 `open_session` 用例，`cm63_lease_continuity.rs` 夹具模式）→ `POST /api/v1/table-data/foreign-key-targets`（体含 `constraintName`+`columns`+`values`）→ 200；断言 `target.table/columns/dataTypes/primaryKeys/unique/candidateLimit` 与 `exists/candidates/truncated` 逐字段（MockDriver 预录源表 FK + 目标表 schema + 探针行）。
- **t2 inbound FK 绿腿**：同会话 `POST /api/v1/table-data/inbound-foreign-keys` → 200；断言 `references[]` 配对序 + `scannedTables`/`truncated`；构造 201 张表目录验证早停如实（可选腿）。
- **t3 值搜索绿腿**：`POST /api/v1/table-data/value-searches` → 200；断言 `matches`（MockDriver 预录 COUNT 行）+ `searchedColumns`；`contains` 模式断言语句经绑定形状（MockDriver `query_with_params` 计数，`mock_driver.rs:946-972`）。
- **t4 origin 绿腿（只读裁决也是 200）**：`POST /api/v1/table-data/result-origins`（MockDriver 默认 `describe_result_origin` = `Undeterminable(NotImplemented)`，`traits/result_origin.rs:22-35`）→ 200 `{"writable":false,"reason":"unknownOrigin",…}`；携带 `table`+`database` 时断言 `identityColumns` 来自目录缓存的真实 key 列。
- **t5 fail-closed 新口径**：①未知/越组织 `dbSessionId` → 404 `NOT_VISIBLE_MESSAGE`（防枚举一致）；②不注入 `live_source` 的租约源腿（第二个夹具实例）→ 借出 `NotWired` → 503（`convert::live`，`convert.rs:174-176`）——**未接线的 fail-closed 语义从 501 探针转为 503 诚实失败**；③`dbSessionId` 缺失 → 400（信封预检）。
- **t6 借出期间关闭（组件级）**：resolve 拿 runtime → `borrow_live_connection_by_id` → `close_session` → `lease.connection()` → `LiveConnectionError::Revoked`（`operations.rs:728-742` 的可执行钉）；HTTP 腿上同一会话第二次请求 → `SessionLost` 409。
- **t7 台账不漏登记**：t1 正常路径后断言 `runtime.live_connection_borrows()==0 && rows()==0`（`mod.rs:265-271`）；再构造 FK 解析中途失败（预录探针错误）→ 400/503 后**仍**归零——异常路径与正常路径共用归还例程的结构性证明。
- **t8 SchemaCache 生效**：同 (connection, database, schema, table) 连打两次 origin → `mock.get_columns_calls()` 第二次不增（`mock_driver.rs:426-431` 计数）。
- **t9 既有探针翻新**：`table_data_routes.rs:757-787` 的探针清单从四条缩为 `change-commits` 一条（`value-searches`/`foreign-key-targets`/`result-origins` 三条移出，由 t1–t4 绿腿接管）；断言文案/无副作用声明不变。
- **t10 变更四不回归**：`change-plan-previews`/`change-commits`/`result-cell-commits`/`result-cell-change-previews` 四条仍 501 `capabilityUnsupported`（`REASON_PLAN`/`REASON_COMMIT` 保留；`previewResultCellChange` 的 reason 文案随状态表刷新改写——见 §7 改件 4）。
- **t11 活句柄面单元腿**（`live_source.rs` `#[cfg(test)]`，桌面 `live_source_tests.rs` 同构）：反查 miss → `SessionLost`；epoch 漂移 → `SessionLost`；profile 缺席 → `Refused`；驱动缺席 → `Refused`；**同实例钉**：`registry.get_sql_driver_by_name` 两次取出的 `Arc` 指针相等（桌面「同一驱动实例被重复取出」的 server 端镜像，`live_source.rs:31-33`）。
- **t12 feature 刻痕钉**：`cargo check -p datazen --features services` 修复后独立编过（§2.6；放进实现 PR 的本地验证清单与 CI 备注）。

---

## 6. 明确不做（非目标清单）

| 项 | 理由 |
| --- | --- |
| `previewPendingChanges` / `commitPendingChanges` 点亮 | 刀3 范围。半能力 fail-closed 硬原则（`REASON_PLAN` 文案 `table_data.rs:689-692`）：预览先亮 = 调用方拿到永远无法提交的计划 |
| `commitQueryResultCell` / `previewResultCellChange` 点亮 | 刀4 范围。commit 半边的执行期结果映射探针未移植（`driver_exec.rs:18`）+ 原会话绑定深度依赖；预览被 commit 封条（成对原则） |
| driverType 直取（无会话的值搜索/FK） | W8-b ② 范围（w8-handoff §2.2）。四条请求契约都带 `dbSessionId`（`fk_navigation.rs:127`、`referenced_by.rs:46`、`value_search.rs:49`、origin 请求体），会话先行是契约形状本身 |
| `build_search_statement` 开口（命令面备选路） | 刀2 走活句柄路，共享函数原样跑（`pub(crate)` 不动）——评估档 §6.2 Q5 的原案维持；命令面路还有文本绑定 vs 驱动原生绑定的语义差（评估档 §6.1），不值得为省一个接线而引入「答案会说谎」的候选面 |
| SchemaCache 失效接线（DDL/刷新 → `invalidate`） | 刀3 随写半边一并定（§2.5；评估档 §6.2 Q6） |
| 活句柄借用的限流/配额 | 借出是无新物理资源的引用计数操作；现有认证链 + 会话配额（`SessionQuotaExceeded` 面）已覆盖滥用面。真出现观测需要再加（`live_connection_borrows()` 投影已在） |
| `ServerLiveConnectionSource` 的多组织 scope 化 | v1 沿 backend 的构造注入单组织裁决（`driver_exec.rs:26-29`）；「owner 不携带组织事实」是已登记的端口缺口（同处），不随刀2 重开 |
| 刀0 的 revoke 接线 | 维持刀0 裁决（`lease.rs:24-26`：v1 只暴露 API 不接线）；刀2 的自动重接语义不改变触发点判断 |

---

## 7. 涉及文件与装配点清单（改动面汇总）

| # | 文件 | 改动 | 量 |
| --- | --- | --- | --- |
| 1 | `crates/datazen-server/src/execution/live_source.rs`（新） | `ServerLiveConnectionSource` 四步 + 单元腿 t11 | S |
| 2 | `crates/datazen-server/src/execution/driver_exec.rs` | `Resource` 加 `pub(crate) fn driver_handle()`（§2.1） | XS |
| 3 | `crates/datazen-server/src/execution/lease.rs` | `live_source: Option<…>` 字段 + `with_live_source` 构造器 + `resolve()` 慢路接线 + 模块文档补「活连接端口随实例装配」段 | XS |
| 4 | `crates/datazen-server/src/http/domains/table_data.rs` | `TableDataState` 增 `lease_source`/`schema_cache`；router 四条 handler 换真实现（引用 table_data_live）；模块头 14 方法状态表四行翻 ✅；`REASON_FK_SOURCE`/`REASON_DRIVER_QUERY` 删除；`REASON_ORIGIN` 改写为 `previewResultCellChange` 的真实剩余 blocker（commit 半边未装配 + 结果映射探针未移植——成对原则与刀4 依赖，不再说「无直连句柄面」）；`previewResultCellChange` handler 的 reason 换用改写后常量 | S |
| 5 | `crates/datazen-server/src/http/domains/table_data_live.rs`（新） | 四 handler + `ServerForeignKeyNavigationSource` + 共享借出助手 + origin 编排 + 请求体 DTO | M |
| 6 | `crates/datazen-server/src/bootstrap.rs` | `live_source` 构造 + `lease_source` 链 `with_live_source` + `schema_cache` 实例 + `TableDataState` 两字段传递（`:464-474`） | S |
| 7 | `crates/datazen-server/src/execution/mod.rs` / `http/domains/mod.rs` | 各加一行 `pub mod …` | XS |
| 8 | `crates/datazen/Cargo.toml` + `src/services/{mod,ports}.rs` | services feature 去 `"ai"` + 三处 cfg 门刻痕（§2.6） | XS |
| 9 | `crates/datazen-server/Cargo.toml` | features 追加 `"services"` | XS |
| 10 | `crates/datazen-server/tests/table_data_live_routes.rs`（新）+ `table_data_routes.rs` 探针翻新 | t1–t12 | M |

**共享层行为面零改动**：`services/*` 只动 cfg 门与 feature def，规则文件（fk_navigation/referenced_by/value_search/ports 的规则体）一字不改；`orchestration/*` 一字不改。

---

## 8. 真实 file:line 引用清单（基线 `66d2e11` 逐行核实）

**活句柄端口（共享层，全部不改动）**

1. `crates/datazen/src/execution/application/orchestration/live.rs:167-177` —— `LiveConnectionSource::open_live` 端口契约（`:171-176` 禁止新建物理连接）。
2. `live.rs:390-403` —— `LiveConnections::borrow`（登记与签发同成败；`:394` 的 `NotWired`）；`:369-374` `unwired()`；`:377-382` `wired()`；`:408-418` 门面 revoke。
3. `live.rs:284-333` —— `LiveConnectionLease`（`connection()` `:301-310` Revoked 拒绝；`release`/Drop 同例程 `:312-333`）；`:244-253` ledger revoke 不摘行。
4. `orchestration/mod.rs:225` —— `new_with_parts` 构造 `live: Mutex::new(LiveConnections::unwired())`（server 未接线病灶）；`:237-240` `with_live_connections` 接线入口（覆盖语义 `:236`）。
5. `orchestration/mod.rs:337-355`（`borrow_live_connection`：authorize :343 → view :344 → entry :345 → 借出 :347-354）、`:363-375`（`get_session_by_id` 读本实例 map）、`:382-393`（`borrow_live_connection_by_id`）。
6. `orchestration/mod.rs:584-605`（`session_entry` 三字段过滤 :589-597）、`:606-639`（`session_view`；revision CAS `:629-635`）；`:265-271`（`live_connection_borrows`/`rows` 只读投影）。
7. `orchestration/operations.rs:728-742` —— 关闭路径先作废活连接借用（位置裁决注释 `:730-732`）。
8. `orchestration/convert.rs:168-179` —— `LiveConnectionError → ApiError`（SessionLost 桶 :171-173；NotWired/Refused → ServiceUnavailable :174-176）。
9. `crates/datazen/src/execution/runtime/registry/backend.rs:39-55` —— `OpenResource.db_session_id`（出向端口反向索引的唯一来源）。

**server 现场与缺口**

10. `crates/datazen-server/src/execution/driver_exec.rs:107-108`（`by_session` 字段）、`:325-329`（open 内同帧写入）、`:161-171`（`live_resource_for_session`：反查 + epoch 比对 :170）、`:85-94`（`Resource` 字段私有、无句柄出口——刀2 补 accessor 的位置）、`:16-18`（缺口登记：无池 v1 / 结果映射探针未移植）。
11. `crates/datazen-server/src/execution/lease.rs:66-68`（`current` cell）、`:91-106`（`new_with_shared_parts`）、`:120-125`（`revoke`）、`:134-165`（`resolve` 快慢路——刀2 接线插入点 ：163 前）；`:24-26`（revoke 不接线裁决）。
12. `crates/datazen-server/src/http/domains/table_data.rs:108-133`（TableDataState）、`:158-214`（router，四条路由 :179-194）、`:337-396`（`resolve_read_target` 闸序参照）、`:400-405`（`profile_unavailable`）、`:412-422`（错误映射参照）、`:547-648`（estimate 会话面闸序原型：可见性闸 :562-580、get_session :581-588、梯子 :617-624、绑定 :625-633）、`:650-653`（`not_visible`）、`:666-677`（fail_closed）、`:679-695`（REASON_* 常量）、`:699-741`（八条 501 handler）、`:86-88`（server 不带 services 门的历史登记）。
13. `crates/datazen-server/src/bootstrap.rs:344-351`（backend 装配）、`:354-359`（publisher）、`:367-377`（lease_source——with_live_source 挂点）、`:379-387`（lease_cases + session_directory）、`:400-419`（resource_host + runner）、`:464-474`（TableDataState 传递点）。
14. `crates/datazen-server/src/http/response.rs:72-118` —— ApiErrorCode→状态码表（SessionNotFound→409 :89-96；CapabilityUnsupported 默认 405 :107）。
15. `crates/datazen-server/src/execution/lease_use_cases.rs:33-52` —— 消费者每方法 resolve 转发（四读 handler 的 resolve 错误映射原型 :44-52）。
16. `crates/datazen-server/src/execution/target_runner.rs:78-147` —— runner manual 分支（租约现取 + 三字段校验；对照：四读端点不经 runner）。

**共享服务层（规则单家，零改动）**

17. `crates/datazen/src/services/ports.rs:45-58`（FkSessionBinding；guard 契约 :41-44）、`:66-70`（端口）。
18. `crates/datazen/src/services/fk_navigation.rs:36-39`（候选上限 50/200）、`:123-163`（请求 DTO）、`:196-207`（结果 DTO）、`:264-269`（解析器入口）、`:482-495`（bind_session 助手）、`:499-517`（resolve_database 梯子）、`:288-306,358-406`（三条驱动往返）。
19. `crates/datazen/src/services/referenced_by.rs:33-39`（扫描上限）、`:42-58`（请求 DTO）、`:77-90`（目录 DTO）、`:123-149`（入口）。
20. `crates/datazen/src/services/value_search.rs:45-65`（请求 DTO）、`:68-77`（结果 DTO）、`:213-280`（search_table_values：绑定/内联二形 :250-258、计数 :268-274）。
21. `crates/datazen/src/table_editing/result_edit/mod.rs:40-63`（GridReadOnlyReason）、`:118-138`（ResultEditability）、`:210-215`（evaluate_result_editability）、`:374-388`（read_only_error_code → `grid.result.*`）。
22. `crates/datazen-driver-api/src/traits.rs:46`（`DatabaseDriver: Send + Sync`——guard 可装箱的根据）、`:500-508`（describe_result_origin 默认）；`traits/result_origin.rs:22-35`（默认 `Undeterminable(NotImplemented)`——t4 的依据）。
23. `crates/datazen-driver-api/src/result_origin.rs:186-209`（describe_result_origin_with_catalog 两半）、`:217-254`（resolve_relation_kind 目录确认）。
24. `crates/datazen-driver-api/src/catalog.rs:78,95-129` —— `LegacyCatalogReader`。
25. `crates/datazen/src/schema/cache.rs:29-34`（CachedColumns）、`:43-58`（SchemaCache：TTL 300s :55、max_tables :56）、`:70-75`（scope 键含 schema）、`:92-130`（get_columns 读通路径）、`:319-346`（invalidate——刀3 备忘）。
26. `crates/datazen/src/execution/runtime/driver_registry/registry.rs:339-346`（`get`）、`:399-401`（`get_sql_driver_by_name`）。
27. `crates/datazen-driver-api/src/resource.rs:596-599` —— `ResourceProvider::connection_handle` 契约。
28. `crates/datazen/src/schema/application/scope.rs:34-38,49-57,76-85` —— `embedded_schema_of` / `metadata_schema` / `table_read_database`（值搜索与 estimate 共享梯子）。

**桌面参照（镜像源，不改动）**

29. `crates/datazen-desktop/src/platform/live_source.rs:104-150`（四步）、`:94-99`（权威类型）、`:19-34`（同实例性质）、`:49-56`（桌面绑定晚于 open 的窗口——server 无此窗口，`backend.rs:45-55` + `driver_exec.rs:325-329`）。
30. `crates/datazen-desktop/src/platform/adapter.rs:493-497,519` —— 装配期接线点（server 对应物 = 租约源慢路，§2.2）。
31. `crates/datazen-desktop/src/platform/session_backend.rs:68-72`（`Resource::driver_handle` 镜像源）、`:205-213`（live_resource_for_session 桌面版）。
32. `crates/datazen-desktop/src/commands/fk_navigation.rs:23-62`（HostNavigationSource：guard 装箱 :52-60）、`:69-108`（两命令信封）。
33. `crates/datazen-desktop/src/commands/driver_command/resolve.rs:32-84` —— 借出统一入口（purpose :49、错误映射 :52-58）。
34. `crates/datazen-desktop/src/commands/value_search.rs:27-81` —— 值搜索编排（梯子 :54-66、错误映射 :79）。
35. `crates/datazen-desktop/src/commands/result_origin.rs:37-60`（请求 DTO）、`:66-102`（编排）、`:177-193`（describe 错误映射）、`:199-236`（目录缓存列读）、`:255-271`（sql_target 梯子）、`:312-320`（信封）。
36. `crates/datazen-desktop/src/commands/error.rs:590-606` —— `From<FkNavigationError>` 逐臂映射（server 版的镜像源）。

**feature 图与契约**

37. `crates/datazen/src/lib.rs:20-21`（services 门）、`:44-45`（ai 门）、`:10-11`（backup 门）、`:100-101`（schema 门）。
38. `crates/datazen/Cargo.toml:54-64`（services def 含 `"ai"`——§2.6 裁决对象）、`:192`（schema def）、`:199-207`（table-browsing → schema 传递）。
39. `crates/datazen-server/Cargo.toml:24-30` —— server feature 集（「按域点亮……不整包开启」纪律 + 追加 `"services"` 的落点）。
40. `scripts/check-server-deps.mjs:44-58` —— 闭包门禁规则（只禁宿主/UI crate）。
41. `docs/architecture/platform/http-api-contract.md:133-143` —— §6.7 四行路由契约（请求→响应、`exists:false` 是成功答案）。
42. `crates/datazen-server/tests/table_data_routes.rs:757-787`（fail-closed 探针——t9 翻新对象）、`:827-873`（WireMockFactory）、`:875-890`（FixedLeaseSource）、`:903-1109`（wired_fixture）、`:1114-1144`（open_session 助手）；`crates/datazen-server/tests/cm63_lease_continuity.rs:14-29`（t1–t7 断言清单与夹具模式）。
43. `crates/datazen-driver-api/src/mock_driver.rs:88,110,161-166,434`（`table_schema`/`tables_by_database`/`set_table_schema_for_test`——FK 测试事实注入）、`:426-431`（get_columns 计数——t8）、`:707-723,725-775,777-789`（get_tables/get_table_schema/get_columns）、`:946-972`（query_with_params 绑定形状）。
44. `crates/datazen/src/services/fk_navigation/tests.rs:23-66` —— 共享层 FK 测试宿主（server 集成测试的 MockDriver 装配词汇来源）。
