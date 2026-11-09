# W8 变更端点排刀：刀0（CM-63）+ 刀1（ServerTableColumnSource）实施级设计

> **设计日期**：2026-11-07。**基线**：main `9531b5f`。**输入**：[w8-fk-mutation-assessment.md](w8-fk-mutation-assessment.md)（W8-b 评估档，**尚未合入 main**，随本 PR 链路落账）+ 前置阅读 [w8-handoff.md](w8-handoff.md)（§3 执行纪律照守）。
> **范围**：只覆盖排刀序列的前两刀——**刀0（跨 resolve 会话连续性 CM-63）**与**刀1（`ServerTableColumnSource` 备料件）**。刀2（活句柄面 + FK/值搜索/origin 四读端点）、刀3（变更二成对）、刀4（result-cell 二）只在排刀关系上引用，不展开。
> **性质**：纯设计文档，零产品代码。本文每个断言指回真实 `file:line`（在基线 `9531b5f` 上逐行核实，非转录评估档）。
> **验证基线**：server 全量本地验证须 `cargo test -p datazen-server --features mcp,use-case-contracts`（w8-handoff §3 纪律 3）。

---

## 0. 排刀位置与两刀的关系

评估档 §5 给出的序列是：**刀0 CM-63 → 刀1 ServerTableColumnSource → 刀2 活句柄面+四读端点 → 刀3 变更二成对 → 刀4 result-cell 二**。本文两刀：

| 刀 | 性质 | 量 | 依赖 | 可独立合并 |
| --- | --- | --- | --- | --- |
| 刀0 | 语义修复（租约源共享语义），改动集中在 `crates/datazen-server/src/execution/lease.rs` | S/M | 无（不依赖刀1/2/3/4） | ✅ |
| 刀1 | 纯增量备料件（server crate 新增一个适配器 + 测试），无行为面变化 | S | 无（不依赖 CM-63、不依赖刀2 活句柄面） | ✅ |

两刀互不阻塞、合并顺序任意（可拆两 PR，也可一 PR 两段提交）。详见 §3。

---

## 1. 刀0：CM-63 跨 resolve 会话连续性

### 1.1 问题定义

mainline 租约源 `ServerRuntimeLeaseSource::resolve()` **每次调用现构造**一个新的 `RuntimeConnectionUseCases` 实例（`crates/datazen-server/src/execution/lease.rs:89-104`，模块文档「每调用现取，禁止缓存」`lease.rs:6,29-32`）。而 `RuntimeConnectionUseCases::new_with_parts`（`crates/datazen/src/execution/application/orchestration/mod.rs:179-231`）每构造一次就新建一整套**实例私有账**（`mod.rs:208-229`）：

| 私有账 | 位置 | 跨 resolve 失联的后果 |
| --- | --- | --- |
| `sessions: Mutex<HashMap<DbSessionId, SessionEntry>>` | `mod.rs:218` | 请求 A 开的会话，请求 B 的 `get_session` / `get_session_by_id` / `borrow_live_connection` 直接 `SessionNotFound`（`mod.rs:584-605` 的 `session_entry` 读**本实例** map，miss → `SessionNotFound` @ `:597`；`get_session_by_id` @ `mod.rs:363-375` 同样读本实例 map） |
| `tokens: Mutex<HashMap<String, IssuedToken>>` | `mod.rs:219`；签发/记账 `tokens.rs:62-103` | 签发令牌（resolve N）与消费令牌（resolve M）失联 |
| `keyring: TokenKeyring::single(secret)`，secret 每实例随机铸 | `mod.rs:203-207,220` | **双层失联**：即使 tokens map 共享，密钥不同令牌也无法验签 |
| `receipts: tokio::sync::Mutex<HashMap<…>>`（open_session 幂等回执） | `mod.rs:228`；写入 `operations.rs:438-447` | 幂等重放跨 resolve 失效，重试可能开出第二个会话 |
| `registry: Arc<SessionRegistry>` 与 `gateway: Arc<ExecutionGateway>`（含 `InMemoryIdempotencyRepository::new()`） | `mod.rs:188-195,197-202` | `session_view` 的 `registry.session_view`（`mod.rs:623-628`）在另一实例上必然失联——**意味着修法不能是「只共享 sessions map」，必须共享整个实例** |
| `idle_activity` / `transaction_gates` / `target_admissions` | `mod.rs:222-227` | 事务闸门与目标准入的互斥跨 resolve 失效 |

**不受影响的面**（对照，证明「读共享件」的路径本来就好）：`sink` 与 `directory` 是租约源跨 resolve 共享的（`lease.rs:37-42`、bootstrap 装配 `crates/datazen-server/src/bootstrap.rs:354-387`），`get_execution` 读 sink（`orchestration/operations.rs:777-784`）、`read_artifact_chunk` 读 sink（`orchestration/mod.rs:481-489`）、目录可见性闸读 directory（`crates/datazen-server/src/http/domains/table_data.rs:564-580`）都跨 resolve 可见。

**与 8 条变更/FK 端点及 estimate 的关系**（评估档 §3.4 结论，本文核实一致）：

- `estimateTableRows` 已接线为真用例（`table_data.rs:547-648`），其 `state.sessions.get_session`（`:581-588`）走 `LeaseConnectionUseCases`（`lease_use_cases.rs:33-52`，每方法 resolve 再转发）——**生产口径上每次请求 resolve 出的是新实例，get_session 必然 `SessionNotFound`**。测试绿是因为夹具 `FixedLeaseSource` 把租约源定为单例（`crates/datazen-server/tests/table_data_routes.rs:875-890`，注释明写「issue/consume 跨 resolve 必然失联（g1 CM-63 登记）」）。**刀0 = 把测试夹具已经采用的语义落进生产租约源**。
- `POST /api/v1/sessions` 开的会话（`crates/datazen-server/src/http/domains/sessions.rs:72,183-198`）跨请求 get/close（`sessions.rs:73-80,206-215,314-333`）同样撞墙。
- 变更四条与 FK 四条中凡请求携带 `dbSessionId` 的路径（manual 提交、原会话绑定、活句柄借出）全部以「会话可跨 resolve 解析」为前置。

### 1.2 涉及文件与接口（真实 file:line）

| 件 | 位置 | 角色 |
| --- | --- | --- |
| `RuntimeLeaseSource` trait | `crates/datazen-server/src/execution/lease.rs:14-18` | 端口：`async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String>` |
| `ServerRuntimeLeaseSource`（结构体 + 构造） | `lease.rs:33-42`（字段）, `48-60`（`new`）, `64-78`（`new_with_shared_parts`）, `82-87`（`session_directory` 出口） | **刀0 唯一产品代码改动点** |
| `resolve()` 实现 | `lease.rs:89-104` | 现：每调用 `RuntimeConnectionUseCases::new_with_parts(profiles, policy, backend, SystemRuntimeClock(Instant::now()), sink, directory)`（`:92-103`） |
| 消费者 A：`LeaseConnectionUseCases` | `crates/datazen-server/src/execution/lease_use_cases.rs:34-52` | 每方法 resolve → sessions 域 / tableData estimate 的用例面（`bootstrap.rs:379-383`） |
| 消费者 B：`ServerTargetRunner` | `crates/datazen-server/src/execution/target_runner.rs:78-87`（字段）, `106-146`（manual 分支 resolve） | 读三端点执行面 |
| 消费者 C：`ServerResourceExecutionHost` | `crates/datazen-server/src/execution/resource_host.rs:83,246-250` | prepare 内 resolve |
| 消费者 D：MCP 后端 | `crates/datazen-server/src/mcp/runtime.rs:60-130` | 同端口消费 |
| bootstrap 装配 | `crates/datazen-server/src/bootstrap.rs:354-387`（sink/publisher、lease_source、lease_cases、session_directory） | 装配不变；共享件清单在此 |

**关键结构事实**：所有消费者都只经 `Arc<dyn RuntimeLeaseSource>` 调 `resolve()`——**刀0 的改动可以完全封在 `ServerRuntimeLeaseSource` 内部，四个消费者一行不改**。

### 1.3 修复方案：租约源持「当前 runtime 单元，直至吊销」

即评估档 §3.4 的 R1（推荐评估起点）。语义重塑为：

> `RuntimeLeaseSource` 的「每调用现取，禁止缓存」禁令约束的是**消费者**（不得把用例面缓存跨请求持有，必须每次向租约源索要，从而尊重「可能被吊销」）；租约源**自身**成为「当前实例」的唯一归属者。吊销发生时换新实例，旧会话按既有 `SessionLost` 语义失效。

**实现形状**（`lease.rs` 单文件改动）：

```rust
pub struct ServerRuntimeLeaseSource {
    // …既有四件：profiles / policy / backend / sink / directory 不变…
    /// 当前 runtime 单元：Some 即「未吊销的当前实例」；None 触发下次 resolve 重建。
    /// Mutex 而非 tokio 锁：构建过程全同步（Arc 克隆 + 小 map + 随机 secret），
    /// 无 await 点，std::sync::Mutex 即可且不跨 await 持锁。
    current: std::sync::Mutex<Option<Arc<RuntimeConnectionUseCases>>>,
}

async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
    // 1) 快路：lock → Some(arc) → clone Arc 返回（O(1)，无重建）。
    // 2) 慢路：None → 在锁内 new_with_parts(...)（与 lease.rs:92-103 逐参一致，
    //    SystemRuntimeClock::now() 语义不变）→ 存入 cell → 返回。
    // 3) pub fn revoke(&self)：lock → *cell = None。v1 仅暴露 API，bootstrap 不调用（见 1.4）。
}
```

设计要点：

1. **共享粒度 = 整个 `RuntimeConnectionUseCases` 实例**，不是逐 map 迁移。理由：`session_view` 依赖每实例 `SessionRegistry`（`mod.rs:623-628`）、令牌验签依赖每实例 keyring（`tokens.rs:62-77` + `mod.rs:203-207,220`）、幂等重放依赖每实例 `InMemoryIdempotencyRepository`（`mod.rs:197-202`）——逐 map 打补丁要动共享层四处语义，违反「规则单家」；共享实例则共享层零改动。
2. **消费者纪律不变**：`LeaseConnectionUseCases` / `ServerTargetRunner` / `ServerResourceExecutionHost` / MCP 后端仍每调用 resolve。禁令对象从「防吊销」改写为「防跨请求持有」——模块文档（`lease.rs:1-32`）随刀同步改写，明确 cell 是被认可的归属者。
3. **既有共享件清单不变**：sink（`lease.rs:38` + `bootstrap.rs:354-359`）、directory（`lease.rs:41,82-87` + `bootstrap.rs:386-387`）、backend（`bootstrap.rs:344-351`）照旧同实例装配。
4. **内存护栏已在共享层、单例化后开始真正生效**：tokens 容量 4096（`tokens.rs:88-93`）、会话墓碑回收 86400s / 4095 上限（`mod.rs:272-288`）、open_session 回执回收（`tokens.rs:105` 的 `reclaim_receipts`）。无需新增护栏。

### 1.4 语义审查点（落码前必须过一遍的三问）

1. **吊销触发点**：v1 刀0 暴露 `revoke()` 但 bootstrap 不接线。理由：server v1 的 profiles/policy/backend 都是长生命周期 `Arc`，profile 事实每次请求现读（`resource_host.rs:153-172`、`orchestration/mod.rs:571-583`），配置漂移由每请求 CAS 拒绝（`session_view` 的 revision 闸 `orchestration/mod.rs:629-635` → `ConfigRevisionMismatch`；prepare 的 revision 闸 `resource_host.rs:210-214`）——不存在「实例整体过期」的现实触发条件。后续若有驱动注册表热替换 / 后端重建需求，再接线 revoke（登记为开放问题，评估档 §6.2 Q2 的答案收敛为：**触发点 = 未定义即不触发；触发语义 = 旧实例整体作废，旧会话按 SessionLost 走**）。
2. **重建窗口**：resolve 的重建发生在锁内、构建全同步且无 I/O（backend 已在外部装配），窗口内无在途请求持「半成品」实例；重建后旧会话在旧实例里，外部不可达——等价于「从未存在」，`SessionNotFound` 而非迁移窗口。与评估档「吊销语义按既有 SessionLost 走」一致。
3. **私有账生命周期变化**：单例化后 tokens/receipts/sessions 从「随 resolve 生灭」变为「随进程生灭」——这是修复本身，不是风险；容量护栏（§1.3 要点 4）与 keyring 单密钥（进程内一致，桌面长生命周期 runtime 同款形状）随之成立。

**明确不做**（评估档 §3.4 R2 的边界）：宿主级 ephemeral 会话账（W8-b ②）不是本刀范围，且它不覆盖编辑会话（openSession 开立的会话在主线账上），与刀0 是并行不互相替代关系。

### 1.5 回归风险

| # | 风险 | 评估 | 缓解 |
| --- | --- | --- | --- |
| 1 | 依赖「每 resolve 新实例」的既有测试被破坏 | 低。检索表明 server 内 `RuntimeLeaseSource` 的实现只有主线件与测试夹具 `FixedLeaseSource`（`table_data_routes.rs:881-890`）/ `ErrLeaseSource`（`mcp/stateless_exec.rs:388`），后者只回报错误 | 测试计划 t0（§1.6）：先全量跑基线，再逐个核对断言 |
| 2 | 令牌/幂等容量从「每 resolve 一份」变为进程级一份（4096），`ResourceBusy` 理论上可达 | 语义收紧而非放宽；web 单组织部署难以触顶 | 接受；护栏复用共享层既有实现 |
| 3 | 会话持久化后无人武装空闲回收 → 物理连接长期持有 | **严格优于现状**（现状：会话跨 resolve 既不可查也不可关，物理资源一样被持有且无法清理）。回收器已存在：`IdlePolicy::default()` 30 分钟（`orchestration/idle.rs:63-71`）、`start_idle_reclaimer`（`idle.rs:235-252`，桌面有、server 未武装） | 刀0 不武装回收器；「server 武装 IdleReclaimer + TTL 策略」登记为后续刀（运维可见性先行：会话现在可查可关了） |
| 4 | `SystemRuntimeClock` 从每 resolve 一基点变为一进程一基点 | 单调语义不变（`mod.rs:87-101`），令牌 TTL 相对测量不受影响 | 无需动作 |
| 5 | estimate 生产口径从「必然 SessionNotFound」变为可用 | 预期内的行为面变化，正是已登记债务的偿还（web-ready-plan §11 2026-11-05 条目） | 测试计划 t6 钉住 |

### 1.6 测试计划

新增集成测试 `crates/datazen-server/tests/cm63_lease_continuity.rs`，夹具复用 `table_data_routes.rs` 的装配模式（真 SQLite 管理库 + `WireMockFactory`（`table_data_routes.rs:827-873`）+ `ServerDriverExecBackend` + **真** `ServerRuntimeLeaseSource` + `LeaseConnectionUseCases`），断言全部跨「两个独立的 `LeaseConnectionUseCases` 实例」（等价于两个独立 HTTP 请求的 resolve）：

- **t0 基线**：改动前先跑 `--features mcp,use-case-contracts` 全量，记录绿名单。
- **t1 会话连续**：A 开会话（`open_session`）→ B `get_session` → `Ok`，connection_id / config_revision / namespace 一致。
- **t2 会话关闭幂等连续**：A 开 → B `close_session` → C 再 `close_session` 回放同一回执（receipts 跨 resolve 可见，`operations.rs:426-447` 语义）。
- **t3 令牌签发/消费连续**：A `issue_submission_token(ExecuteInSession)` → B `execute_in_session` 消费成功（tokens map + keyring 同实例的双层验证）。
- **t4 执行视图跨 resolve 不回归**：A 提交 target 执行 → B `get_execution` / `read_artifact_chunk`（旧行为已好，钉住）。
- **t5 吊销语义**：`revoke()` 后 resolve 返回新实例，旧 dbSessionId 在新实例上 `get_session_by_id` → `SessionNotFound`（评估档「旧会话按 SessionLost 语义失效」的可执行钉）。
- **t6 estimate 端到端**：HTTP 面走真 lease source（不再是 FixedLeaseSource 的一条腿）：POST sessions 开会话 → POST `/api/v1/table-data/row-count-estimates`（`table_data.rs:547-648` 的闸序）→ 200。
- **t7 既有探针不变**：`table_data_routes.rs:757-787` 的 fail-closed 探针与读三端点绿腿不改一字仍绿（8 条 501 状态在刀0/刀1 都不动，`table_data.rs:666-677,699-741`）。

---

## 2. 刀1：`ServerTableColumnSource`（备料件）

### 2.1 目标与边界

评估档 §4 表第 5/6 行的唯一缺口件：变更二端点（`previewPendingChanges` / `commitPendingChanges`）需要的 server 侧目录源。共享层端口已定形：

```rust
// crates/datazen/src/table_editing/mod.rs:46-52（端口，不改动）
#[async_trait]
pub trait TableColumnSource: Send + Sync {
    async fn table_columns(
        &self,
        table: &RowChangeTableContext,
    ) -> Result<CachedColumns, FeatureError>;
}
```

端口硬规则（`mod.rs:41-45`）：**答不出必须显式报错，空答案不等于「没有主键」**。

刀1 边界：

- **交付**：server crate 新增 `crates/datazen-server/src/execution/table_columns.rs`（实现 + 单元/集成测试）。
- **不交付**：`previewPendingChanges` / `commitPendingChanges` handler 仍 501（`table_data.rs:721-728`，`REASON_PLAN`/`REASON_COMMIT` 文案 `:689-695` 不动）。理由是硬原则「半能力 fail-closed 成对」（评估档 §6.1；预览先亮 = 调用方拿到永远无法提交的计划）。
- **不依赖**：CM-63（走 target 面，`manual_handle: None`，无会话解析）；刀2 活句柄面（见 §2.3）；W8-b 三件。

### 2.2 接口设计

```rust
// crates/datazen-server/src/execution/table_columns.rs（新件，镜像桌面 HostTableColumns 的 server 形态）
pub struct ServerTableColumnSource {
    runner: Arc<dyn TargetCommandRunner>,                       // TableDataState.runner 同实例（table_data.rs:117）
    registry: Arc<DriverRegistry>,                              // TableDataState.registry 同实例（table_data.rs:113）
    ctx: RequestContext,                                        // 每请求构造（SessionIdentity.ctx 真主体）
    revision: Counter,                                          // 调用方已读 config_revision（与读三端点同 CAS 语义）
}

impl ServerTableColumnSource {
    pub fn new(
        runner: Arc<dyn TargetCommandRunner>,
        registry: Arc<DriverRegistry>,
        ctx: RequestContext,
        revision: Counter,
    ) -> Self;
}

#[async_trait]
impl TableColumnSource for ServerTableColumnSource {
    async fn table_columns(
        &self,
        table: &RowChangeTableContext,
    ) -> Result<CachedColumns, TableEditingError> { … }
}
```

**执行流**（`table_columns` 内部六步）：

1. **方言驱动解析**：`registry.get_sql_driver_by_name(&table.driver_type)` → `None` → `TableEditingError::NotConfigured("Driver unavailable")`（桌面镜像 `crates/datazen-desktop/src/commands/data/table_columns.rs:81-85`）。
2. **schema 梯子**：`table.schema` 经 `driver.has_schema_level()` 过滤——无 schema 级的驱动拒绝任何 schema 实参（`public.users` 在 MySQL 不是东西，桌面镜像 `table_columns.rs:86-90`）。database 必填：`validate_table_context`（`crates/datazen/src/table_editing/change_set.rs:206-218`）已保证 `table.database` 非空，本件不再兜底。
3. **铸造 target 绑定**：`TargetBinding { target: ExecutionTarget::new(connection_id, namespace).with_object(ObjectTarget::new("table", &table.table)), revision: self.revision, manual_handle: None }`。**object target 是硬要求**：`get_table_schema` 的目标准入规则 `allows_object && requires_object`（`crates/datazen-driver-api/src/traits/command_api.rs:35-38`，规则钉测试 `crates/datazen-driver-api/src/command_catalog.rs:132-135`）；server 已有先例把该命令映射为 object target（`crates/datazen/src/workflow/application/resource_execution.rs:245-252`）。绑定铸造与 estimate 的 handler 同款（`table_data.rs:625-633`）。
4. **经 runner 跑命令**：`runner.run(&self.ctx, &binding, "get_table_schema", json!({"table": …, "database": …, "schema": …}))`。input 的 `table`+`database` 必填（`crates/datazen-driver-api/src/schema_catalog_commands.rs:92-109`，required @ `:108`）。runner target 分支全序：`host.prepare`（闸序 = profile 存在 → 未禁用 → revision CAS → 连接配置 → 驱动可用 → 目标规范化，`resource_host.rs:192-257`）→ token → `execute_at_target`（`target_runner.rs:147-177`）→ 25ms 轮询（`:194-208`）→ 产物收集（`:212-234`）。
5. **解析产物**：`outputs` 里 `find(|output| output["kind"] == "commandResult")`（共享层既有谓词先例 `crates/datazen/src/table_browsing/api.rs:411`）→ 对 `&output["data"]` 用 `parse_table_schema_from_command`（`schema_catalog_commands.rs:203-206`）反序列化为 `TableSchema`。形状依据：非 query 命令产物是 `{"kind":"commandResult","data":<CommandResult.data>}`（`crates/datazen-driver-api/src/resource_adapter.rs:502`），`get_table_schema` 的 `data = {"schema": <TableSchema>}`（`schema_catalog_commands.rs:183`）。解析失败 → `TableEditingError::Validation("Table schema unavailable for '<table>': …")`（桌面文案同族，`table_columns.rs:117-121`）。
6. **映射 `TableSchema` → `CachedColumns`**：`columns: schema.columns`；`primary_keys` = 有效主键（`primary_keys` 字段优先，空则回退 `is_primary_key` 列标记——`crates/datazen-driver-api/src/types.rs:453-455` 的既有规则）；`table_name` / `cached_at: Instant::now()`（`CachedColumns` 形状 `crates/datazen/src/schema/cache.rs:29-34`）。**`columns.is_empty()` → 显式 `Validation` 报错**（端口硬规则 + 桌面镜像 `table_columns.rs:122-127`）。

**错误面零新码**：runner 失败的 `ApiError` 直接走共享层现成的 `From<ApiError> for TableEditingError`（`crates/datazen/src/table_editing/error.rs:33-77`，逐臂映射已定：`SessionNotFound/NotFound → NotFound`、`InvalidArgument/ConfigRevisionMismatch → Validation`、`ServiceUnavailable → NotConfigured` 等）。

### 2.3 与既有件的组合点（可组合性论证）

- **命令面 `get_table_schema` 可组合性**：该命令是驱动目录读的标准命令（`schema_catalog_commands.rs:88-123`，定义随 `LegacyCommandCatalog` 进驱动目录；workflow 面 `resource_execution.rs:192-201` 已把它链入定义查找），执行走 `driver.get_table_schema(handle, table, database, schema)`（`:180-182`）。server 执行面经 `ServerTargetRunner` 的 target 分支派发（`target_runner.rs:147-177`），**全程不需要活句柄、不需要会话**——这就是「命令面列源不依赖 CM-63 与活句柄面」（评估档 §4 表第 5 行）的机制根据。
- **为刀3 备料的组装清单**（刀1 落地后全部就位）：`commit_changes`（`table_editing/mod.rs:104-156`）吃 driver + `ConnectionFacts`（server 有 profiles/connections，`table_data.rs:122-126`）+ `&dyn TableColumnSource`（**刀1 本件**）+ `Arc<dyn ConnectionUseCases>`（租约 resolve，刀0 之后语义健康）+ `Arc<dyn ArtifactChunkReader>`（`ServerArtifactChunks`，`resource_host.rs:266-282`）+ `RequestContext`（`SessionIdentity.ctx`）。写半边 `execute_plan`（`table_editing/write_exec.rs:161-352`）是 transport 中立 runtime 面——刀1 一行不改它，但列源就位意味着刀3 的宿主侧只剩 handler 组装。
- **列重读闸的天然配合**：commit 半边在闸序里**重读列**（`table_editing/mod.rs:147` → `validate_immutable_plan` `:148-154`）——`ServerTableColumnSource` 每调用现查的 v1 形状与此闸正交（每次预览/提交各自现查，无陈旧缓存面）。

### 2.4 装配点（bootstrap）

**v1 刀1 零 bootstrap 改动**：`TableDataState` 已持有全部零件（`runner` / `registry` / `profiles` / `connections`，`table_data.rs:109-133`、装配 `bootstrap.rs:464-474`），组件按请求构造（与桌面 `HostTableColumns<'a>` 持 `&AppState` 的每请求形状同构，`table_columns.rs:15-23`），无需进入全局装配图。刀3 的 handler 在调用点构造：

```
ServerTableColumnSource::new(state.runner.clone(), state.registry.clone(),
                             identity.ctx.clone(), revision)
```

若后续出现多 handler 共用需求，再考虑挂 `TableDataState`（挂的是**工厂**而非实例，因 `ctx`/`revision` 是每请求事实）——登记为刀3 的装配细节，不在刀1 预支。

### 2.5 非目标（明确不做与理由）

| 项 | 理由 |
| --- | --- |
| `SchemaCache` 挂载 | `SchemaCache`（`crates/datazen/src/schema/cache.rs:43-58`，TTL 300s）的读通路径 `get_columns`（`:92-120`）需要活句柄 + `CatalogReader`——那是刀2 活句柄面的事；写通路径（`store_columns_if_current`）不公开。刀1 走命令面现查，刀2 点亮 origin 时再按评估档 §3.1 的口径挂缓存（S） |
| 8 条 handler 状态变化 | 成对 fail-closed 原则（`REASON_PLAN` 文案 `table_data.rs:689-692` 明写 commit 半边未装配） |
| `build_search_statement` 开口 | 仅当值搜索走命令面备选路才需要；刀2 活句柄路覆盖后维持 `pub(crate)`（评估档 §6.2 Q5） |
| 列缓存失效接线（DDL/刷新 → `invalidate`，`cache.rs:319`） | 随刀3 变更端点点亮一并定（评估档 §6.2 Q6） |

### 2.6 测试计划

集成测试（与 `table_data_routes.rs` 同目录或 `#[cfg(test)]` 模块），夹具复用 `WireMockFactory` 模式（`table_data_routes.rs:827-873`）+ MockDriver 预录 `get_table_schema` 应答：

- **t1 命中路径**：MockDriver 预录含主键的 schema 应答 → 全栈（prepare → token → execute → poll → collect → 解析 → 映射）返回 `CachedColumns`，列序 / data_type / nullable / primary_keys 逐字段断言；`table_name` 与请求表名一致。
- **t2 空列显式报错**：驱动应答 0 列 → `Validation`，文案含表名（端口硬规则的可执行钉）。
- **t3 schema 级过滤**：`has_schema_level() == false` 的驱动 + 上下文带 schema → 命令 input 不携带 `schema` 键（钉「不把 schema 传给不认 schema 的驱动」）。
- **t4 未知驱动**：`registry` 无该 `driver_type` → `NotConfigured("Driver unavailable")`。
- **t5 revision CAS**：binding.revision 与 profile 现值漂移 → `prepare` 的 CAS（`resource_host.rs:210-214`）拒绝 → 映射为 `Validation`（`error.rs:48-56` 的 `ConfigRevisionMismatch` 桶）。
- **t6 产物形状防御**：outputs 无 `commandResult` 块 / `data.schema` 缺失 → `Validation("Table schema unavailable for …")`，绝不出「看似成功」的空列答案。
- **t7 8 条 fail-closed 不回归**：`table_data_routes.rs:757-787` 探针照旧绿。

---

## 3. 独立可合并性声明：刀0/刀1 不依赖刀2 的活句柄面

**刀0 的独立性**：改动封在 `ServerRuntimeLeaseSource`（`lease.rs`）内部；四个消费者经端口 `resolve()` 交互（§1.2），端口签名不变；共享层（`datazen` crate）零改动。它修的是「会话/令牌/回执跨请求可见」这一执行底座语义，与「活句柄借出」（`LiveConnectionSource::open_live`，`orchestration/live.rs:167-177`；server 未接线，`lease.rs` resolve 不调 `with_live_connections`，`LiveConnections::unwired()` @ `orchestration/mod.rs:225`）是两件正交的事——刀0 之后 server 的活连接端口**仍然未接线**，刀2 才接。刀0 立即兑现的价值：estimate 生产可用（§1.6 t6）、sessions 域 open/get/close 跨请求真实可用。

**刀1 的独立性**：走 target 执行面（`manual_handle: None`，`prepare` 闸序），不携带 `dbSessionId`、不解析会话、不借句柄——CM-63 是否已修不影响其任何行为；刀2 的 `ServerLiveConnectionSource` / 租约源 `with_live_connections` 接线（评估档 §3.3 缺口链）落地与否也不影响。刀1 对共享层的唯一「依赖」是既有的端口与命令面（全部已存在，零改动）。

**合并矩阵**：

| 合并顺序 | 结果 |
| --- | --- |
| 刀0 先 | 底座语义修复即兑现；刀1 合并前后均不受影响 |
| 刀1 先 | 备料件入库（无行为面变化，8 条仍 501）；刀0 随后合并不需要 rebase 语义 |
| 同 PR 两段 | 允许；提交粒度保持两段（CM-63 修复一段、列源一段），便于回溯 |

---

## 4. 真实 file:line 引用清单（基线 `9531b5f` 逐行核实）

**刀0 核心**

1. `crates/datazen-server/src/execution/lease.rs:14-18` —— `RuntimeLeaseSource` 端口签名。
2. `crates/datazen-server/src/execution/lease.rs:89-104` —— `resolve()` 每调用现构造 runtime（CM-63 病灶）。
3. `crates/datazen/src/execution/application/orchestration/mod.rs:208-229` —— `new_with_parts` 的实例私有账（sessions 218 / tokens 219 / receipts 228 / live unwired 225）；keyring 随机 secret `203-207,220`；每实例 registry/gateway `188-202`。
4. `crates/datazen/src/execution/application/orchestration/mod.rs:584-605` —— `session_entry` 读本实例 map，miss → `SessionNotFound`（597）。
5. `crates/datazen/src/execution/application/orchestration/operations.rs:449-456`（get_session）、`:438-447`（回执落本实例 receipts）、`:777-784`（get_execution 读共享 sink，对照面）。
6. `crates/datazen/src/execution/application/orchestration/tokens.rs:62-103` —— 令牌签发走本实例 keyring + tokens map。
7. `crates/datazen-server/src/execution/lease_use_cases.rs:33-52` —— 消费者每方法 resolve 转发。
8. `crates/datazen-server/tests/table_data_routes.rs:875-890` —— `FixedLeaseSource` 夹具注释（g1 CM-63 登记 + 单例测试形先例）；`:757-787` fail-closed 探针。
9. `crates/datazen-server/src/bootstrap.rs:354-387` —— sink/publisher、lease_source、lease_cases、session_directory 装配。
10. `crates/datazen-server/src/http/domains/table_data.rs:547-648` —— estimate 会话面闸序（生产口径撞 CM-63 的既登记面）；`:564-588` directory 闸 + `get_session`。

**刀1 核心**

11. `crates/datazen/src/table_editing/mod.rs:46-52` —— `TableColumnSource` 端口；`:41-45` 空答案≠无主键；`:67-79` `preview_changes`；`:104-156` `commit_changes`（147 重读列 / 148-154 不可变计划）。
12. `crates/datazen/src/table_editing/change_set.rs:20-27` —— `RowChangeTableContext` 字段面；`:206-218` `validate_table_context`。
13. `crates/datazen/src/table_editing/error.rs:10-25,33-77` —— `TableEditingError` 变体与现成 `From<ApiError>` 映射。
14. `crates/datazen-server/src/execution/target_runner.rs:78-191` —— `ServerTargetRunner::run`（target 分支 147-177 / 轮询 194-208 / 收集 212-234）。
15. `crates/datazen-driver-api/src/schema_catalog_commands.rs:88-123`（get_table_schema 定义）、`:172-184`（执行分支）、`:203-206`（`parse_table_schema_from_command`）。
16. `crates/datazen-driver-api/src/traits/command_api.rs:35-38` —— get_table_schema `requires_object`；`crates/datazen-driver-api/src/command_catalog.rs:132-135` —— 钉测试。
17. `crates/datazen-driver-api/src/resource_adapter.rs:502` —— 非 query 命令产物 `{"kind":"commandResult","data":…}`；`crates/datazen/src/table_browsing/api.rs:411` —— `commandResult` 谓词先例。
18. `crates/datazen-driver-api/src/types.rs:441-451,453-455,470-478` —— `TableSchema` / 有效主键规则 / `ColumnSchema`。
19. `crates/datazen/src/schema/cache.rs:29-34,43-58,92-120,319` —— `CachedColumns` 形状、`SchemaCache`（TTL 300s）、活句柄读通路径、invalidate（刀3 备忘）。
20. `crates/datazen-desktop/src/commands/data/table_columns.rs:15-50,63-129` —— 桌面镜像参照（快路径 91-97 / miss 路 98-121 / 空列显式报错 122-127）。
21. `crates/datazen/src/workflow/application/resource_execution.rs:245-252` —— get_table_schema → object target 既有映射。
22. `crates/datazen/src/table_editing/write_exec.rs:161-352` —— `execute_plan` transport 中立写半边（刀3 组合点论证；manual 路 195-210 / owned-session 开立 212-239 / affected!=1 拒绝 288-294 / RollbackAndClose 清理 325-351）。
23. `crates/datazen-server/src/http/domains/table_data.rs:109-133`（TableDataState 零件清单）、`:666-677`（fail_closed）、`:679-695`（逐端口原因常量）。
24. `crates/datazen/src/execution/application/orchestration/idle.rs:63-71,235-252` —— IdlePolicy 默认值与 `start_idle_reclaimer`（刀0 回归风险 3 的后续刀依据）。
25. `crates/datazen/src/execution/application/orchestration/live.rs:167-177,356-403` —— 活句柄端口契约与 `LiveConnections::borrow`（刀2 的边界参照，本文只用于声明刀0/刀1 不触碰）。

---

## 5. 交付清单与提交纪律

- **刀0**：`crates/datazen-server/src/execution/lease.rs`（cell 化 + `revoke()` + 模块文档改写）+ `crates/datazen-server/tests/cm63_lease_continuity.rs`（t1–t7）。共享层零改动、Cargo.toml 零改动。
- **刀1**：`crates/datazen-server/src/execution/table_columns.rs`（新件）+ 测试；`crates/datazen-server/src/execution/mod.rs` 加一行 `pub mod table_columns;`（或 `mod table_columns;`，按 crate 内可见性定）。共享层零改动、handler 零改动、Cargo.toml 零改动。
- **台账**：落地 PR 在 `web-ready-plan.md` §11 顶部加条目（w8-handoff §3 纪律 5）；本设计档随分支先行入库。
- **验证**：`cargo test -p datazen-server --features mcp,use-case-contracts`；两刀各自独立可验证（刀0：t1–t7；刀1：t1–t7 各自）。
