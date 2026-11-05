# W8 变更端点排刀：刀3（变更二成对端点点亮）实施级设计

> **设计日期**：2026-11-08。**基线**：main `66d2e11`（PR #186 刀1 合并后）。**输入**：[w8-fk-mutation-assessment.md](w8-fk-mutation-assessment.md)（§4 表第 5/6 行 + §6.1 成对原则 + §6.2 Q4/Q6）、[w8-mutation-knife0-1-design.md](w8-mutation-knife0-1-design.md)（§2.3 组合点清单是本文的前提论证）+ 前置阅读 [w8-handoff.md](w8-handoff.md)（§3 执行纪律照守）。
> **范围**：只覆盖**刀3**——`previewPendingChanges`（`POST /api/v1/table-data/change-plan-previews`）与 `commitPendingChanges`（`POST /api/v1/table-data/change-commits`）两条端点从 501 fail-closed 点亮为真实现。刀2（活句柄面四读，`w8-knife2-design` 分支）与刀4（result-cell 二）只在依赖关系上引用，不展开。
> **性质**：纯设计文档，零产品代码。本文每个断言指回真实 `file:line`（在基线 `66d2e11` 上逐行核实，非转录评估档与刀0/刀1 设计档）。
> **验证基线**：server 全量本地验证须 `cargo test -p datazen-server --features mcp,use-case-contracts`（w8-handoff §3 纪律 3）。

---

## 0. 排刀位置与依赖矩阵——核心裁决：会话两路

### 0.1 刀3 在序列中的位置

评估档 §5 的序列：**刀0 CM-63（#185 已合并）→ 刀1 ServerTableColumnSource（#186 已合并）→ 刀2 活句柄面+四读端点（w8-knife2-design 分支）→ 刀3 变更二成对（本文）→ 刀4 result-cell 二**。

| 依赖件 | 状态 | 对刀3 的作用 |
| --- | --- | --- |
| 刀0：租约源 cell 化（跨 resolve 会话/令牌/回执连续） | ✅ 已合并 | commit 写半边的 `open_session` / `issue_submission_token` / `execute_in_session` / `close_session` 落在同一 runtime 实例的私有账上（`lease.rs:134-165` 的 cell 快路） |
| 刀1：`ServerTableColumnSource`（命令面列源） | ✅ 已合并 | preview 与 commit 的重读列闸（`table_editing/mod.rs:147`）的 server 供给件（`table_columns.rs:95-176`） |
| 刀2：`ServerLiveConnectionSource` + 租约源 `with_live_connections` 接线 + `SchemaCache` 挂载 | ⏳ 并行分支 | **不是刀3 的硬前置**（刀3 全程走 target 命令面 + runtime 用例面，无活句柄依赖）；刀2 挂载的 `SchemaCache` 与刀3 的失效裁决关系见 §2.6 |
| W8-b 三件（stream 桥 / driverType 直取 / 命令历史落账） | ⏳ 未开工 | 无直接组合路径：变更二全是同步单应答；审计落账不在本刀（§6） |

**刀3 自己的新件只有 handler 组装**（评估档 §4 表第 5/6 行的裁决——列源就位后「刀3 的宿主侧只剩 handler 组装」，刀0/刀1 设计档 §2.3 原话）：两条 handler + 请求 DTO + 错误映射 fn + `TableDataState` 加一个租约源字段。共享层（`datazen` crate）与刀1 件零改动。

### 0.2 核心裁决：会话两路（manual 路 vs owned-session 路）

`execute_plan`（`crates/datazen/src/table_editing/write_exec.rs:161-352`）内部有两条会话来源路，由 `manual_handle: Option<SessionHandle>` 参数分岔（`write_exec.rs:194-195`）：

| 路 | 进入条件 | 语义 | 现场依据 |
| --- | --- | --- | --- |
| **manual 路** | 调用方持有一个 `TransactionState::Active` 的编辑事务会话句柄 | 语句跑进**调用方的事务**：不开不关事务、不关会话（「table owner retains control」，`write_exec.rs:319-323`）；前置 `get_session` 三字段 + 事务态校验（`:195-210`） | 桌面数据编辑器的显式事务（`tableSession.ts:12-18` 的 `transactionState === 'active'` → `view.handle`） |
| **owned-session 路** | `manual_handle: None` | 宿主自开自关：`open_session`（`OwnerRef::ClientSession{purpose:"tableChangeSet"}`，`write_exec.rs:229-232`）→ `platform.beginTransaction`（`:267-274`）→ 逐条 execute → `platform.commitTransaction`（`:303-310`）→ `close_session(RollbackAndClose)`（`:330-345`，Drop 兜底 `:49-73`） | 无编辑事务的调用方（web v1 的唯一现实形态） |

**裁决：刀3 点亮 = preview 全路径 + commit 的 owned-session 全路径 + manual 路在 HTTP 面显式拒（501 分支）。这不违反「成对 fail-closed」原则。** 理由四条：

1. **成对原则约束的是两条端点，不是 commit 内部的两条会话分支。** 评估档 §6.1 与登记文案（`table_data.rs:689-692`）的原话是「预览先亮 = 调用方拿到永远无法提交的计划」——预览与提交必须同刀点亮。刀3 同 PR 点亮两条端点（§5 测试计划的成对钉 t3），原则的字面与精神都满足。而 manual 路没有自己的「预览半边」：`preview_changes` 的签名根本没有 manual_handle 参数（`table_editing/mod.rs:68-79`）——「两条会话路成对」在结构上无从谈起。
2. **manual 路在 web 面没有调用方。** 它要求客户端持有一个处于 `Active` 事务的会话句柄（`packages/frontend/src/lib/session/tableSession.ts:16-17`），而 web 形态没有任何开事务的 HTTP 面：`begin_session_transaction` / `commit_session_transaction` 是 legacy 轨命令、整轨不上 HTTP（`packages/web/src/platform/web/webUnsupportedMethods.ts:69,79,190` 的 `LEGACY_HTTP_OFF`）；driverGateway 的 `execute_driver_command` 走命令目录编排（`driver_gateway.rs:14` 的 `command_policy`），`platform.beginTransaction` 是宿主命令、不进驱动命令目录（宿主托管命令在 `resource_host.rs:367-381` 逐条显式登记），driverGateway 派发不到它。开着这条面 = 服务一个没有调用方的面。
3. **「能跑」不等于「该跑」。** 刀0 之后 manual 路在 server 侧技术上可行（`get_session` 跨 resolve 连续），但编辑事务的归属/回收语义未定：会话以 `OwnerRef::Editor` 开立（`table_data_routes.rs:1131-1137` 的归属对齐约束）、server 尚未武装 `IdleReclaimer`（刀0 设计档 §1.5 风险 3：无人武装的空闲回收 → 物理连接长期持有）、前端编辑器形态也未定（评估档 §6.2 Q4）。提前点亮等于把语义债提前变现。
4. **明拒，绝不静默忽略。** wire 契约的 `CommitPendingChangesRequest` 带 `manualHandle?: SessionHandle | null`（`packages/backend-client/src/types/tableData.ts:212-217`），且 serde 默认忽略未知字段——若 server DTO 不显式携带该字段，一个带 `manualHandle` 的请求会被**静默按 owned-session 执行**：调用方以为写进了自己的事务，实际落在一个已提交并关闭的陌生会话里。因此 server DTO 必须显式反序列化 `manual_handle`（`#[serde(default)]`，`null` 与缺省都归 `None`）并在 `Some` 时显式拒绝（拒绝码裁决在 §2.5 末行：501 `capabilityUnsupported`，与同域其余 not-served 面同一 wire 词汇，`fail_closed` 的 501 类）。

manual 路的放开条件（登记，不在本刀）：web 面出现编辑事务形态（开事务端点 + 会话回收语义 + 前端编辑器），与评估档 §6.2 Q4 的答案收敛。

### 0.3 独立可合并性（对刀2 的关系）

- 刀3 **不依赖**刀2 的任何件：列源走 target 命令面（`manual_handle: None`，`table_columns.rs:129`），写半边走 runtime 用例面（`write_exec.rs:161` 起无一处活句柄）——评估档 §4 表第 5 行「命令面列源不依赖 W8-b 与 CM-63」+ 第 6 行「owned-session 路全部调用落同一实例」在基线 `66d2e11` 上均已成立。
- 刀3 与刀2 的合并顺序任意。唯一交点：刀2 给 `TableDataState`/装配图引入 `SchemaCache`（其设计档 §2.5），刀3 的失效裁决（§2.6）以「列源与 SchemaCache 零耦合」为结论，两种合并顺序下结论都不变。
- 刀4 的 result-cell 二继续 501（成对原则的剩余封条，§5 t9）。

---

## 1. 问题定义：两条 501 的现状与点亮零件清单

### 1.1 fail-closed 现场（基线 `66d2e11`）

| 端点 | 路由 | handler 现场 | 登记理由 |
| --- | --- | --- | --- |
| `previewPendingChanges` | `POST /api/v1/table-data/change-plan-previews` | `crates/datazen-server/src/http/domains/table_data.rs:721-723` → `fail_closed("previewPendingChanges", REASON_PLAN)`（`:666-677`，501 `capabilityUnsupported` + with_status 覆盖） | `REASON_PLAN`（`:689-692`）：列源未装配 + commit 半边同样未装配，单独点亮预览是半能力 |
| `commitPendingChanges` | `POST /api/v1/table-data/change-commits` | `table_data.rs:726-728` → `fail_closed("commitPendingChanges", REASON_COMMIT)` | `REASON_COMMIT`（`:693-695`）：runtime 写半边（`execute_plan`）未装配 |

两条路由的挂载点在 `table_data.rs:197-204`（模块头状态表第 19-20 行），认证链 L2–L6 与 L4 CSRF（POST unsafe 方法）由 router 的 merge 循环保证（`:195-196` 注释登记「两段式的 CSRF 令牌列随真接线生效」——刀3 点亮即兑现，**本刀对 CSRF 不需要任何新代码**，契约 §6.8 `commitPendingChanges` 行的 ✓ 令牌列（`docs/architecture/platform/http-api-contract.md:150`）由此满足）。

501 探针现状：`crates/datazen-server/tests/table_data_routes.rs:757-787` 的 `driver_bound_table_data_routes_fail_closed_as_not_implemented` 断言四条路由恒 501 且文案携带方法名与「No statement ran」——其中 `/table-data/change-commits` 在刀3 移出探针（§5 t9）。

### 1.2 点亮需要组装的零件（全部已就位，file:line 逐一核实）

| 零件 | 位置 | 消费点 |
| --- | --- | --- |
| 预览半边 `preview_changes(driver, table, changes, columns, config_revision)` | `crates/datazen/src/table_editing/mod.rs:68-79` | preview handler |
| 提交半边 `commit_changes(driver, request, facts, columns, manual_handle, runtime, artifacts, context)` | `mod.rs:104-156`（闸序：validate_table_context :114 → 驱动类型漂移 :115/:86-96 → config_revision CAS :116-121 → read_only 拒写 :122-126 → database/schema 上下文比对 :127-146 → 重读列 :147 → 不可变计划重建 :148-154） | commit handler |
| 写半边 `execute_plan`（transport 中立 runtime 面） | `write_exec.rs:161-352`（语句清单 insert 先行 :169-183；owned-session 开立 :212-239；重验 CAS :258-265；affected != 1 拒绝 :288-294；清理 :325-351） | 经 commit_changes :155 |
| 列源端口与硬规则 | `mod.rs:41-52`（答不出必须显式报错，空答案 ≠ 没有主键） | 两 handler 各铸一次 |
| 列源 server 实现（刀1 件） | `crates/datazen-server/src/execution/table_columns.rs:69-90`（构造）, `:95-176`（六步流） | 两 handler |
| 租约源（刀0 件，cell 单实例直至吊销） | `crates/datazen-server/src/execution/lease.rs:36-39`（端口）, `:57-69`（结构）, `:134-165`（resolve 快慢路） | commit handler 单次 resolve |
| `ServerArtifactChunks`（`pub(crate)` 产物块读取投影） | `crates/datazen-server/src/execution/resource_host.rs:266-282` | commit handler 铸 `Arc<dyn ArtifactChunkReader>` |
| profile/connection 事实（ConnectionFacts 的 server 来源） | `table_data.rs:337-396` 的 `resolve_read_target`（读三端点既有闸序：profile 存在 → 未禁用 → 驱动类型漂移 → 方言驱动可用，返回 `(ConnectionConfig, Counter, driver)`） | 两 handler 共用 |
| 请求真主体 | `crates/datazen-server/src/http/router.rs:285-310`（`SessionIdentity.ctx`） | 两 handler 的 ctx / 列源 / runtime 授权 |
| 目标命令执行面（列源的 runner） | `crates/datazen-server/src/execution/target_runner.rs:78-190`（target 分支 :147-177 = prepare 闸序 → token → execute → 轮询 → 收集） | 列源内部 |
| `ConnectionFacts` 共享形状 | `mod.rs:58-65`（database_type / read_only / database / schema / config_revision） | commit handler 组装 |

**结论**：刀3 是纯装配刀。新代码只出现在 `crates/datazen-server` 的 `table_data.rs`（DTO + 两 handler + 错误映射 fn + 状态表两行）与 `bootstrap.rs`（一个字段接线），共享层一行不改。

---

## 2. 修复方案

### 2.1 请求/响应 DTO（web 前端契约对齐结果：零前端改动）

**契约溯源**（逐处核实）：

- 分发表：`previewPendingChanges → POST /table-data/change-plan-previews`、`commitPendingChanges → POST /table-data/change-commits`，**无 `body` 变换——payload 原样出站**（`packages/web/src/platform/web/httpRoutes.ts:404-405`；`backendTransport.ts:227` 的 `route.body === undefined ? payload : route.body(payload)`）。
- 能力声明面：两条方法**不在** `unsupportedMethods`（`webUnsupportedMethods.ts:32-33` 的 `CAPABILITY_EXCLUDED` 只有 `open_base64_with_dialog`）——web guard 放行，501 是 server 答的、点亮后即真应答。**前端与 `packages/backend-client` 零改动**。
- 错误侧：非 2xx 四字段体经 `deserializeApiError` 归一（`backendTransport.ts:174-191`）——本刀错误映射的 wire 形状自动兼容。
- 契约文档：`docs/architecture/platform/http-api-contract.md:145-154`（§6.8：preview 200 `RowChangePlan` 不执行；commit ✓ 令牌、200 同步回执——「凡是结果在响应体里的用 200」）。

**preview 请求体**（前端 `PreviewPendingChangesRequest`，`types/tableData.ts:200-203`）：

```rust
/// POST /api/v1/table-data/change-plan-previews 的体（契约 PreviewPendingChangesRequest）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewPendingChangesBody {
    context: datazen::table_editing::RowChangeTableContext,   // change_set.rs:18-27，camelCase
    changes: Vec<datazen::table_editing::PendingRowChange>,   // change_set.rs:29-48
}
```

对齐注记：前端 `TableChangeContext` 的 `connectionId`/`driverType` 是 nullable（`types/tableData.ts:79-86`，「前端在连接解析前装配上下文」）；null 打到 Rust `String` 字段 = 反序列化失败 → `invalid_argument` 400——这正是前端 `isCompleteTableChangeContext` 守卫（`types/tableData.ts:74-78` 注释）的存在理由，错误面诚实。前端 `PendingRowChange.rowIndex?`（`:100`，UI-only 提示）不在共享 Rust 类型里，serde 默认忽略未知字段，**不做 `deny_unknown_fields`**——多一个 UI 提示字段不是错误。

**响应**：共享层 `RowChangePlan` 原样序列化（`change_set.rs:71-88`，camelCase；`config_revision` 是 `Counter`，JSON 十进制字符串——CM-01，`crates/datazen/src/shared/id.rs:170-181,206-209`；前端注记 `types/tableData.ts:149-167` 与此逐字对应）→ 200。

**commit 请求体**（前端 `CommitPendingChangesRequest`，`types/tableData.ts:212-217`）：

```rust
/// POST /api/v1/table-data/change-commits 的体（契约 CommitPendingChangesRequest）。
///
/// `manual_handle` 必须显式反序列化并显式拒绝（Some → 501）：serde 默认忽略
/// 未知字段，省掉它会静默把 manual 提交降级为 owned-session 执行——比拒绝更危险。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitPendingChangesBody {
    db_session_id: String,
    #[serde(default)]
    manual_handle: Option<datazen::execution::SessionHandle>,   // {dbSessionId, runtimeEpoch}，session.rs:17-25
    plan: datazen::table_editing::RowChangePlan,                // change_set.rs:71-88
    fingerprint: String,
}
```

`SessionHandle` 的 wire 形状两侧逐字段一致（前端 `types/session.ts:51-55` `{ dbSessionId, runtimeEpoch }`；Rust `crates/datazen/src/execution/domain/session.rs:17-25` camelCase）。前端发 `manualHandle: null`（`packages/frontend/src/commands/database.ts:208,252` 的 `params.manualHandle ?? null`）→ `None` → owned-session，**显式 null 与缺省字段同义**。

**响应**：共享层 `CommitPendingChangesResponse`（`change_set.rs:106-113`：planId / fingerprint / statements[{operation, rowIdentity, affectedRows}] / affectedRows）→ 200。前端 `CommitStatementResult.operation` 的三值 discriminant（`types/tableData.ts:184-190`）由共享层保证（insert/update/delete，`write_exec.rs:295-301`）。

### 2.2 previewPendingChanges handler 组装全序

```text
POST /change-plan-previews（identity: SessionIdentity）
 1. serde 解析体 → PreviewPendingChangesBody；失败 → invalid_argument("preview: …") 400
    （读三端点同款，table_data.rs:434-437）
 2. validate_table_context(&body.context) → 400 InvalidArgument
    （共享层校验，change_set.rs:206-223；读端点先例 table_data.rs:438-443）
 3. resolve_read_target(&state, &identity.ctx, &body.context) → (config, revision, driver)
    （profile 存在 → 未禁用 → 连接目录 → 驱动类型漂移 → 方言驱动；错误序照桌面，
    table_data.rs:337-396。preview 不设 read_only 闸——桌面镜像，data.rs:22-53 无此闸）
 4. ServerTableColumnSource::new(state.runner.clone(), state.registry.clone(),
                                identity.ctx.clone(), revision)
    （刀1 装配先例：tests/table_columns_source.rs:143-150 的 source() 同款调用点铸造）
 5. preview_changes(driver.as_ref(), body.context, &body.changes, &columns, revision.get())
    （mod.rs:68-79：列源现查 → build_row_change_plan → 盖 config_revision 章）
 6. 200 Json(RowChangePlan) / table_editing_error_response(error)（§2.5）
```

语义锚点：列源每调用现查（`table_columns.rs:141-144` 经 runner target 分支跑 `get_table_schema`，prepare 闸序含 revision CAS），无缓存面 → 预览永远答「当下」的列与主键；`plan.config_revision` 是预览时刻的 profile revision，是 commit CAS 的基准（§2.3 步 7）。

### 2.3 commitPendingChanges handler 组装全序

```text
POST /change-commits（identity: SessionIdentity）
 1. serde 解析体 → CommitPendingChangesBody；失败 → invalid_argument("commit: …") 400
 2. body.manual_handle.is_some() → 501 capabilityUnsupported（with_status 501）
    （manual 路明拒，§0.2 裁决 4；文案声明「manual session transactions are not
    served by this host yet; the host opens, drives and closes the plan's own
    session」——与 fail_closed 同族词面，绝不静默降级）
 3. 组装 CommitPendingChangesRequest { db_session_id, plan, fingerprint }
    （共享层 DTO，change_set.rs:90-96；manual_handle 不进共享层请求）
 4. validate_table_context(&request.plan.table) → 400
    （桌面输入校验镜像：data.rs:67「feature layer re-checks the context」双检序）
 5. resolve_read_target(&state, &identity.ctx, &request.plan.table)
    → (config, revision, driver)（闸序同 preview；错误序与读三端点逐字一致）
 6. state.lease.resolve() → Arc<RuntimeConnectionUseCases>（单次 resolve）
    失败 → 503 ServiceUnavailable（lease_use_cases.rs:44-52 同款映射）
    ——单次 resolve 的实例贯穿整个提交序列（§2.4 路形状）
 7. ConnectionFacts { database_type: config.database_type.clone(),
                      read_only: config.read_only,
                      database: config.database.clone(),
                      schema: config.schema.clone(),
                      config_revision: revision.get() }
    （桌面 facts 组装镜像：data.rs:90-96）
 8. columns = ServerTableColumnSource::new(runner, registry, ctx, revision)
    artifacts = Arc::new(ServerArtifactChunks(runtime.clone())) as Arc<dyn ArtifactChunkReader>
    （resource_host.rs:266-282；target_runner.rs:143-144 的同款投影先例）
 9. commit_changes(driver.as_ref(), request, &facts, &columns,
                   None,                    // manual 路已被步 2 拒绝，恒 None
                   runtime as Arc<dyn ConnectionUseCases>,
                   artifacts, identity.ctx.clone())
   （mod.rs:104-156：共享层闸序全重检 → 重读列 → 不可变计划重建 → execute_plan）
10. 200 Json(CommitPendingChangesResponse) / table_editing_error_response(error)
```

**CAS 双闸的语义**：步 5 读出的 `revision` 进入 facts，commit_changes 以它拒绝过期计划（`mod.rs:116-121`「Connection configuration changed; preview the changes again」）；`execute_plan` 在会话开立后**再**对 `opened.config_revision` 复验一次（`write_exec.rs:258-265`）——预览到提交之间、以及提交会话开立瞬间两次漂移都 fail-closed。

**单次 resolve 的裁决**（评估档 §4 表第 6 行的「单次 resolve 的 `Arc<RuntimeConnectionUseCases>` 贯穿提交序列」）：`open_session` 回执、`issue_submission_token` 签发、`execute_in_session` 消费、`get_execution` 轮询、`close_session` 幂等回放必须落同一实例的私有账（`orchestration/mod.rs:208-229`）。刀0 之后每调用 resolve 也恒返同一 `Arc`（cell 快路，`lease.rs:134-142`），但那是对的**事故**（cell 恰好没被吊销）；单次 resolve 是对的**构造**——并且持有 `Arc` 使在途提交对中途 `revoke()` 免疫（吊销只影响后续 resolve，不影响已借出的实例）。桌面同构：`data.rs:87-89` 的「一个实例贯穿整个提交序列」。

**装配点改动**：`TableDataState` 增加 `pub lease: Arc<crate::execution::lease::RuntimeLeaseSource>` 字段（`table_data.rs:109-133` 的零件清单尾部）；`bootstrap.rs:464-474` 的 `table_data_router(...)` 实参补 `lease: lease_source.clone()`（装配点在 mcp 块 move `lease_source`（`bootstrap.rs:518-524`）**之前**，clone 可行）。测试夹具 `wired_fixture` 同步补一行（`table_data_routes.rs:1088-1096`，夹具传 `FixedLeaseSource(runtime.clone())` 或 `real_lease.clone()`——语义等价，建议真租约源以贴近生产）。

### 2.4 两路会话的 TargetBinding / manual_handle 形状（逐条 file:line）

| 面 | 绑定/会话形状 | 现场 |
| --- | --- | --- |
| 列源（preview 与 commit 的重读列共用） | `TargetBinding { target: ExecutionTarget::new(connection_id, namespace).with_object(ObjectTarget::new("table", table)), revision: self.revision, manual_handle: None }`——**object target 是 `get_table_schema` 的硬准入**（requires_object） | `table_columns.rs:117-130`；目标准入规则 `datazen-driver-api/src/traits/command_api.rs:35-38`（钉测试 `command_catalog.rs:132-135`）；runner target 分支 = `host.prepare` 闸序 → token → execute → 轮询 → 收集（`target_runner.rs:147-177,179-189`） |
| commit 写路 · owned-session（HTTP 面**唯一**服务形态） | 不走 `TargetBinding`，直接用例面：`issue_submission_token(OpenSession)` → `open_session(OpenSessionRequest{initial_target: plan 的 connection/database/schema, owner: OwnerRef::ClientSession{purpose:"tableChangeSet"}, idempotency_key})` → `platform.beginTransaction` → 逐条 `execute` → `platform.commitTransaction` → `close_session(RollbackAndClose)` | `write_exec.rs:212-239`（开立）、`:229-232`（owner）、`:267-274`（开事务）、`:276-302`（逐条 execute，含 affected!=1 拒绝 :288-294）、`:303-310`（提交）、`:325-345`（关闭+结果组合）；`OwnerRef::ClientSession` 的策略面放行：`orchestration/convert.rs:106-110` + `check_owner`（`:94`）；`open_session` 的目标授权以真主体 ctx 跑 `AuthorizationAction::Connect`（`orchestration/operations.rs:391-398`） |
| commit 写路 · manual（HTTP 面**明拒**） | `get_session` 三字段 + `TransactionState::Active` 校验 → 语句进调用方事务、不开不关不清理 | `write_exec.rs:195-210`（校验）、`:267,303`（`if !manual` 分支）、`:319-323`（归还控制权）；HTTP 面在 handler 步 2 拒绝，共享层此路从 HTTP 不可达 |

后端对写命令的两道独立守卫（owned-session 路的纵深防御，刀3 不新增）：SQL 守卫按连接目录回读的 `read_only` 拒写（`driver_exec.rs:426-441`，`check_sql(sql, config.read_only, false)`）+ `platform.*` 事务命令的宿主派发（`driver_exec.rs:462-488`）；`execute` 命令的 `boundParameters` 原生绑定 → `{"rowsAffected": N}`（`datazen-driver-api/src/traits/command_api.rs:109-124`）——`write_exec.rs:147-153` 逐产物找 `rowsAffected` 的消费面由此成立。

### 2.5 错误映射面（TableEditingError → HTTP，逐臂）

新 fn `table_editing_error_response(error: TableEditingError) -> Response`（`table_data.rs`，紧邻读半边的 `table_read_error_response` `:412-422`），与读端点先例逐条对齐：

| TableEditingError 臂（`error.rs:10-25`） | wire 码 → 状态 | 对齐依据 |
| --- | --- | --- |
| `Validation(msg)` | `InvalidArgument` → **400** | 读半边 `Validation → 400`（`table_data.rs:415-417`）；共享层守卫文案（stalePlan / readOnly / affectedMismatch / 上下文不完整）都是调用方可修正的输入或状态问题，消息原样上 wire（前端按 `grid.commit.*` 码前缀分支——桌面同源文案，`commands/data/tests.rs:247` 起的 code 断言先例） |
| `NotFound(msg)` | `NotFound` → **404** | 封闭码表的 404 行（`response.rs:85`）；`From<ApiError>` 的 NotFound 桶来源（`error.rs:44-47`） |
| `NotConfigured(msg)` | `ServiceUnavailable` → **503** | `From<ApiError>` 的 ServiceUnavailable 桶回投影（`error.rs:57-61`）；读半边 `Internal → 503` 的「最近诚实类别」裁决（`table_data.rs:408-420`） |
| `Internal(msg)` | `ServiceUnavailable` → **503** | 同上（用例层故障不是调用方的输入错误） |
| `Driver(DriverError)` | `ServiceUnavailable` → **503** | server 家族先例：`workflow_error` 的 `Driver → ServiceUnavailable`（`target_runner.rs:241-249`）；WR-D4 错误映射逐行确认时再细化（`response.rs:108-116` 的临时分组登记同款口径） |
| `Io` / `Json` | `ServiceUnavailable` → **503** | 宿主/序列化故障，非调用方输入错误 |

**已知状态码漂移（如实登记）**：`From<ApiError> for TableEditingError`（`error.rs:33-77`）是共享层既定的兼容桶（「映射与旧 FeatureError 的 From<ApiError> 逐臂一致」），它把 `ConfigRevisionMismatch`（原生 409）、`SessionQuotaExceeded`（原生 429）、`ContextConflict`（原生 409）等折进 `Validation`/`Internal` 桶，经本表投影后以 400/503 出面（wire `code` 同步变为 `invalidArgument`/`serviceUnavailable`）。这与读半边 `TableReadError::Runtime` **保留**原生 `ApiError` 的透传面（`table_data.rs:414`）不同——写半边的错误类型在共享层已定形（`commit_changes` 返回 `Result<_, TableEditingError>`），刀3 不为此改共享层签名；漂移面登记给 WR-D4 错误映射刀统一裁决，消息文本（含 `grid.commit.*` 码前缀与 requestId 尾注，`error.rs:37-41`）不受影响。

manual 路明拒的分支码：`ApiError::new(CapabilityUnsupported, …)` + `with_status(StatusCode::NOT_IMPLEMENTED)` → **501**——`fail_closed` 的同族词面（not-implemented 类，`response.rs:13-17` 的 405/501 区分裁决：此形态不服务的子路属「尚未实现」），与其余六条 fail-closed 端点在 wire 上同词汇。

### 2.6 列缓存失效裁决（评估档 §6.2 Q6 收口：触发点不在变更二）

**裁决：刀3 不接任何失效；`SchemaCache.invalidate` 的触发点属于会跑 DDL 的 SQL 执行面，不属于变更二端点。** 论证三条：

1. **变更二的写半边只产 DML。** `execute_plan` 的语句清单只有 insert / update / delete 三类（`write_exec.rs:169-183`，语句由 `build_insert_statement` / `build_update_statement` / `build_delete_statement` 渲染），INSERT/UPDATE/DELETE 不改变列元数据与主键——成功的 commit **不产生**缓存失效义务。「写操作应触发 invalidate」的评估档原案（§6.2 Q6）对 DML 是过度失效：白白丢缓存，换来零正确性收益。
2. **刀3 的列路径与 `SchemaCache` 零耦合。** 刀1 的 `ServerTableColumnSource` 每调用经命令面现查（`table_columns.rs:141-144`），不读不写任何缓存（刀0/刀1 设计档 §2.5 明确把 `SchemaCache` 挂载划给刀2 的 origin 面）。写路径的主键重验（`mod.rs:147`）因此天然新鲜——DDL 后的下一次 preview/commit 读到的就是新列，不需要任何接线。
3. **真正会制造陈旧的 DDL 发生在别的面。** 任意 SQL（含 DDL）经 `execute`/`query` 命令进入执行面（`driver_exec.rs:408-441` 的 SQL 守卫路径），其 HTTP 入口是 driverGateway `execute_driver_command`（dbSessionId 路）与未来的 web 查询面——失效接线（成功执行后 `SchemaCache::invalidate(connection_id, database, table)`，`crates/datazen/src/schema/cache.rs:319-346`；连接级 `clear_connection` `:366-370`）应挂在**那个**面收口时，登记为该刀的顺手件。刀2 挂载的 `SchemaCache` 读通路径维持 TTL 300s 自愈（TTL 300s + `max_tables=1000`，`cache.rs:55-56`），DDL 后 ≤300s 的陈旧窗口与桌面同权，verdict 安全方向不受影响（刀2 设计档 §4 风险 7 的既有接受裁决）。

对刀2 设计档遗留句（「失效接线随刀3 一并定」）的交待：本文的裁决即「定」——答案是「变更二不触发；DDL 面（driverGateway / 查询面收编刀）触发」，该刀落地时从本节取走结论即可。

### 2.7 装配图增量（bootstrap）

```text
bootstrap.rs
  ├─ lease_source（:367-377，既有）──clone──▶ TableDataState.lease（新字段）
  ├─ table_target_runner（:415-419，既有）──▶ TableDataState.runner（列源与读端点共用）
  ├─ driver_registry / profiles / connections（既有）──▶ 列源构造 + ConnectionFacts
  └─ table_data_router(state)（:464-474，实参补 lease 一行）
```

零新依赖、零 Cargo.toml 改动、零共享层改动。

---

## 3. 语义审查点（落码前必须过的三问）

1. **HTTP 重试会把提交重复应用一次——这是缺陷还是语义？** commit 的幂等键（`open_session` / `issue_submission_token` / `execute_in_session` 的 idempotency_key）在**单次请求内**生成与消费（`write_exec.rs:212-215,276-284`）；响应丢失后的重试是一次新请求、新令牌、新会话，fingerprint 守卫计划内容但不守「恰好一次」。桌面同权（每次 commit 调用同样现铸令牌）——这是共享层的既有语义，不是刀3 引入的缺口。刀3 的处置：不改语义、不加请求级幂等头（`Idempotency-Key` 留给后续刀），把「commit 非恰好一次」写进 handler 文档注释与风险表（§4 第 3 行）。诚实边界：这与读端点的天然幂等不同，前端重试策略（只在网络层失败且未收到响应时重试）是前端形态的义务。
2. **501 明拒 manualHandle 与「同 MethodMap 同契约」是否冲突？** 不冲突：MethodMap 的类型面允许携带 `manualHandle`（桌面 IPC 面真实消费它），web HTTP 面不服务这个子形状——这正是契约 §7「能力差异走显式失败，绝不静默空答」的分支级应用。web 的 `unsupportedMethods` **不需要**登记（它描述的是整个方法不可服务；preview/commit 是服务的，只有 manual 子路 closed）——guard 层零改动。501 而非 400 的理由：删掉 `manualHandle` 字段请求即合法（owned-session 可服务），但「这个子能力此宿主尚未提供」是装配事实而非请求错误；且 501 与同域其余 not-served 面同词汇，前端对 `CapabilityUnsupported` 已有分支（`backendTransport.ts:174-191` 归一路径）。
3. **commit 的执行触碰从哪一行开始？闸序是否先于一切执行面？** 第一处执行域触碰是 handler 步 5 的 `resolve_read_target`（profile 读取），先于 lease resolve、先于列源、先于 `open_session`——未知/禁用 profile 在任何会话资源分配之前 404（防枚举口径与读端点同一条，`table_data.rs:398-405`）。会话开立发生在 `commit_changes` → `execute_plan` 内部（步 9 之后），此前还有共享层五道闸（上下文/漂移/CAS/read_only/schema 比对，`mod.rs:114-146`）。open_session 的目标授权以请求真主体跑（`operations.rs:391-398`），`SessionIdentity.ctx` 是授权语——P3 语义延伸到写面，与读三端点同一口径（`table_data.rs:324-328` 注释）。

---

## 4. 回归风险表

| # | 风险 | 评估 | 缓解 |
| --- | --- | --- | --- |
| 1 | 501 探针破坏：`change-commits` 移出探针列表 | 预期内（点亮即翻新） | §5 t9：探针翻新为六条（其余 fail-closed 面），文案断言不改 |
| 2 | `From<ApiError>` 分桶后的状态码漂移（`ConfigRevisionMismatch` 409→400、`SessionLost` 409→404、`SessionQuotaExceeded` 429→503 等） | 共享层既有桶定义（`error.rs:33-77`）的投影后果，非刀3 新语义；消息文本与 `grid.commit.*` 码前缀无损 | 如实登记（§2.5），WR-D4 错误映射刀统一确认；前端按 message 码前缀分支，不受状态码漂移影响 |
| 3 | HTTP 重试重复应用 DML（commit 非恰好一次） | 共享层既有语义，桌面同权；fingerprint 守内容不守重试 | §3 审查点 1：文档化 + 前端重试纪律；请求级幂等头登记为后续项 |
| 4 | 写面首次上 HTTP 的安全面 | 纵深防御全在既有件：L4 CSRF（认证链内 POST）、sql_guard 只读拒写（`driver_exec.rs:440`）、共享层 read_only 闸（`mod.rs:122-126`）、affected==1 歧义写拒绝（`write_exec.rs:288-294`）、不可变计划重建（`change_set.rs:443-478`：客户端 SQL/摘要仅展示） | t5/t7 钉住只读与歧义写；t4 钉指纹护栏 |
| 5 | owned session 生命周期：每 commit 开+关一会话，quota/回收压力 | 会话必关：显式 `RollbackAndClose`（`write_exec.rs:330-345`）+ Drop 兜底（`:49-73`）；IdleReclaimer 未武装不影响（无长持有面） | t10 钉「提交后会话已关闭」（`get_session` → SessionLost） |
| 6 | 刀2 挂载 `SchemaCache` 后 DDL 陈旧 ≤300s | 与桌面同权；变更二自身无缓存面（§2.6） | DDL 触发点登记给 SQL 执行面收口刀；本刀零接线 |
| 7 | `TableDataState` 加字段破坏既有测试夹具 | 两处夹具（`wired_fixture` / 刀1 的组件级夹具不构造 TableDataState） | 仅 `wired_fixture` 一行；t0 基线全量先行 |

---

## 5. 测试计划

新集成测试文件 `crates/datazen-server/tests/mutation_pair_routes.rs`，夹具复用 `table_data_routes.rs` 的 `wired_fixture` 模式（真 SQLite 管理库 + `WireMockFactory` + `ServerDriverExecBackend` + 真 `ServerRuntimeLeaseSource` + `ServerTargetRunner`，`table_data_routes.rs:903-1109`）与 `table_columns_source.rs` 的 MockDriver 预录模式（`table_columns_source.rs:153-292`）；会话连续性断言参照 `cm63_lease_continuity.rs`（`open_session` 经真用例 `:293-317`）。

- **t0 基线**：改动前 `cargo test -p datazen-server --features mcp,use-case-contracts` 全量记录绿名单（w8-handoff §3 纪律）。
- **t1 preview 绿路径**：MockDriver 预录含主键 schema（`columns_for_database` 腿，`mock_driver.rs:754-764`）→ `POST /change-plan-previews` 体 `{context, changes:[update]}` → 200：`planId` 非空、`fingerprint` 与 `changesFingerprint` 重算一致、`configRevision` = profile revision（十进制字符串）、`updates[0].sqlTemplate` 含 UPDATE、`table` 回显上下文。
- **t2 commit 绿路径（owned-session 全链）**：t1 的 plan 原样提交 `{dbSessionId, manualHandle 缺省, plan, fingerprint}` → 200：`affectedRows == 1`、`statements[0].operation == "update"` 且 `rowIdentity` 逐字段一致；`execute_with_params_calls == 1`（`mock_driver.rs:463`）且 `execute_rows_affected = 1`（`mock_driver.rs:124`）。
- **t3 成对不变量 + manual 明拒**（本刀原则的可执行钉）：(a) preview 与 commit 两条绿腿在同一测试文件同一 PR（成对原则——单腿缺席即测试文件自身失败）；(b) 体带 `"manualHandle": {dbSessionId, runtimeEpoch}` → 501 `capabilityUnsupported`，message 含 manual 与 owned-session 指引词面，且 **MockDriver `execute_with_params_calls == 0`**（拒绝先于一切执行触碰）；(c) `"manualHandle": null` 与缺省字段同义 → 都走 owned-session 绿路径。
- **t4 指纹/不可变计划护栏**：篡改 `plan.updates[0].currentValues` 但保留原 fingerprint → 400「fingerprint is stale or was modified」（`change_set.rs:470-474`）；`plan.config_revision` 加一 → 400「Connection configuration changed; preview the changes again」（`mod.rs:116-121`）——两组都断言 driver 零调用。
- **t5 只读连接**：`config.read_only = true` 夹具 → preview 仍 200（预览不设只读闸，桌面镜像）→ commit 400「grid.cell.readOnly」（`mod.rs:122-126`）。
- **t6 提交期 CAS**：preview 后再 `connections.save`（revision 自增）→ commit 400 stalePlan（步 5 的新 revision ≠ plan.config_revision）；对照腿：同 revision 重提交幂等可行。
- **t7 歧义写拒绝**：`execute_rows_affected = 0` / `= 2` → 400「grid.commit.affectedMismatch」（`write_exec.rs:288-294`）；会话仍被清理（`get_session` → SessionLost）。
- **t8 可见性**：`other_org` cookie 的 preview/commit → 404 固定文案「Connection profile unavailable」（防枚举，`table_data.rs:398-405`）；无 cookie → 401。
- **t9 501 探针翻新**：`table_data_routes.rs:757-787` 探针现状列四条（value-searches / foreign-key-targets / change-commits / result-origins）——移除 `/table-data/change-commits` 后余三条照旧 501 且文案携带方法名与「No statement ran」，断言一字不改；可选加固：把 `inbound-foreign-keys` / `result-cell-commits` / `result-cell-change-previews` 并入探针集合（它们不在现探针里，501 语义由 handler 显式登记维持）。result-cell 二的 501 即刀4 封条（成对原则的剩余面）。
- **t10 会话生命周期**：t2 成功后经 `state.sessions.get_session`（跨 resolve，刀0 语义）→ `SessionLost`（owned session 已 RollbackAndClose，`write_exec.rs:330-345`）。
- **t11 错误映射臂钉**（组件级，参照 `table_columns_source.rs` 的组件直调形）：`TableEditingError` 各臂经 `table_editing_error_response` 的 wire 码/状态逐臂断言（Validation→400 invalidArgument、NotFound→404、NotConfigured/Internal/Driver/Io/Json→503 serviceUnavailable）。

---

## 6. 明确不做（非目标与理由）

| 项 | 理由 |
| --- | --- |
| manual_handle 提交路点亮 | §0.2 裁决：web 无编辑事务面（legacy 轨不上 HTTP、`platform.*` 不进驱动命令目录）、会话回收语义未定、无调用方。501 明拒 + 放开条件登记 |
| result-cell 二（`commitQueryResultCell` / `previewResultCellChange`） | 刀4：结果映射探针未移植（`driver_exec.rs:18` 缺口）+ 原会话绑定，依赖面最深；且 preview 被其 commit 半边封条（成对原则） |
| `SchemaCache` 挂载与失效接线 | §2.6 裁决：变更二只产 DML、列路径零缓存耦合；DDL 触发点属 SQL 执行面（driverGateway / 查询面收编刀） |
| 写操作审计落账 | W8-b ③ 的 `audit_events` 半边是唯一落账面（评估档 §3.5）；本刀不做旁路落账（避免规则双家） |
| 请求级幂等头 / 恰好一次 commit | §3 审查点 1：共享层既有语义，桌面同权；升级属跨传输语义变更，另立项 |
| 前端 / `packages/backend-client` / 契约文档改动 | §2.1 溯源：分发表已有、体原样透传、错误归一兼容——web 契约零改动；`http-api-contract.md` 的 §6.8 行已是真实现口径 |
| 共享层（`crates/datazen`）任何改动 | `preview_changes` / `commit_changes` / `execute_plan` / DTO 全部就位（§1.2）；刀3 的全部新代码在 server crate 的装配面 |

---

## 7. 涉及文件与装配点清单（改动面汇总）

| 文件 | 改动 |
| --- | --- |
| `crates/datazen-server/src/http/domains/table_data.rs` | 模块头状态表两行（⛔→✅ + 说明）；`TableDataState` 加 `lease` 字段（+1）；DTO 两个；`handler_preview_pending_changes` / `handler_commit_pending_changes` 换真实现；`table_editing_error_response` fn；REASON_PLAN/REASON_COMMIT 常量删除（manual 分支文案内联） |
| `crates/datazen-server/src/bootstrap.rs` | `table_data_router` 实参补 `lease: lease_source.clone()`（+1 行） |
| `crates/datazen-server/tests/table_data_routes.rs` | 探针列表移除 `change-commits`；`wired_fixture` 的 `TableDataState` 补 lease 一行 |
| `crates/datazen-server/tests/mutation_pair_routes.rs` | 新文件（t1–t10） |
| `crates/datazen-server/tests/…`（或 `#[cfg(test)]`） | t11 错误映射臂钉（可并入新文件） |

台账：落地 PR 在 `web-ready-plan.md` §11 顶部加条目（w8-handoff §3 纪律 5）；本设计档随分支先行入库。

---

## 8. 真实 file:line 引用清单（基线 `66d2e11` 逐行核实）

**共享层 · 变更两段式**

1. `crates/datazen/src/table_editing/mod.rs:41-52` —— `TableColumnSource` 端口 + 空答案硬规则；`:58-65` `ConnectionFacts`；`:68-79` `preview_changes`（:75 列源、:77 盖章）；`:86-96` `validate_connection_facts`；`:104-156` `commit_changes`（:114 上下文 / :116-121 CAS / :122-126 read_only / :127-146 上下文比对 / :147 重读列 / :148-154 不可变计划 / :155 execute_plan）。
2. `crates/datazen/src/table_editing/write_exec.rs:11-25`（两条 stalePlan 错误文案）、`:43-73`（TableSession + Drop 兜底）、`:75-158`（TableSession::execute：get_session → token → execute_in_session → 轮询 → rowsAffected 读取 :147-153）、`:161-352`（execute_plan 全序：语句清单 :169-183 / 空写早退 :184-191 / manual 分岔 :194-195 / manual 校验 :195-210 / owned 开立 :212-239 / owner :229-232 / 目标复验 :246-257 / CAS :258-265 / beginTransaction :266-274 / 逐条 execute + affected!=1 :276-302（:288-294）/ commitTransaction :303-310 / 回执 :311-316 / manual 归还 :319-323 / 关闭与结果组合 :325-351）。
3. `crates/datazen/src/table_editing/change_set.rs:18-27`（RowChangeTableContext）、`:29-48`（PendingRowChange，insert_marked 语义 :37-47）、`:71-88`（RowChangePlan）、`:90-96`（CommitPendingChangesRequest）、`:106-113`（CommitPendingChangesResponse）、`:206-223`（validate_table_context）、`:225-242`（changes_fingerprint）、`:443-478`（validate_immutable_plan：重按不可变元数据重建 SQL，:466-467 注释 + :470-474 指纹比对）。
4. `crates/datazen/src/table_editing/error.rs:10-25`（七臂变体）、`:33-77`（From<ApiError> 逐臂分桶：NotFound 桶 :44-47 / Validation 桶 :48-56 / NotConfigured 桶 :57-61 / Internal 桶 :62-74）、`:37-41`（message 携带 requestId）。
5. `crates/datazen/src/shared/id.rs:170-181,206-209` —— Counter 十进制字符串 serde（CM-01）。
6. `crates/datazen/src/execution/domain/session.rs:17-25` —— SessionHandle（dbSessionId + runtimeEpoch，camelCase）。
7. `crates/datazen/src/execution/application/orchestration/operations.rs:385-448` —— open_session（:391-398 目标授权真主体、:399 owner 校验、:411-434 幂等回执回放）；`convert.rs:89-115` —— `OwnerRef::ClientSession` 放行（:106-110）与 Job 类拒绝（:111-113）。
8. `crates/datazen/src/schema/cache.rs:27-34`（CachedColumns）、`:43-58`（SchemaCache，TTL 300s :55 / max_tables :56）、`:319-346`（invalidate）、`:366-370`（clear_connection）—— §2.6 的失效裁决依据。

**server 侧 · 执行底座（刀0/刀1 件）**

9. `crates/datazen-server/src/execution/lease.rs:36-39`（RuntimeLeaseSource 端口）、`:57-69`（结构 + current cell :66-68）、`:120-125`（revoke）、`:134-165`（resolve 快慢路）。
10. `crates/datazen-server/src/execution/lease_use_cases.rs:34-52` —— 每方法 resolve 转发 + resolve 失败的 ServiceUnavailable 映射（:44-52）。
11. `crates/datazen-server/src/execution/table_columns.rs:69-90`（构造签名）、`:95-176`（六步流：驱动解析 :100-104 / schema 梯子 :108-112 / 绑定 :117-130 / input :134-140 + run :141-144 / 解析 :148-158 / 空列显式报错 :162-168 + 映射 :169-175）。
12. `crates/datazen-server/src/execution/target_runner.rs:78-88`（结构）、`:92-190`（run：manual 分支 :106-146 / target 分支 :147-177 / 轮询收集 :179-189）、`:241-249`（workflow_error 的 Driver → ServiceUnavailable 裁决）。
13. `crates/datazen-server/src/execution/resource_host.rs:266-282` —— `ServerArtifactChunks`（pub(crate) ArtifactChunkReader 投影）。
14. `crates/datazen-server/src/execution/driver_exec.rs:408-441`（SQL 命令守卫：params 文本绑定 + check_sql(read_only)）、`:462-488`（platform.* 事务命令宿主派发）。
15. `crates/datazen-server/src/bootstrap.rs:342-351`（registry/backend）、`:354-359`（sink/publisher）、`:367-377`（lease_source）、`:379-383`（lease_cases）、`:386-387`（session_directory）、`:415-419`（table_target_runner）、`:464-474`（table_data_router 装配点——lease clone 插入处）、`:518-524`（mcp 块 move lease_source——装配次序依据）。
16. `crates/datazen-server/src/http/router.rs:285-310` —— SessionIdentity（ctx 组装，client_instance_id 恒 "http" :304）。
17. `crates/datazen-server/src/http/response.rs:72-118` —— status_for 状态码表（:77-79 400 类 / :85 404 / :89-96 409 类 / :104 503 / :107 CapabilityUnsupported 默认 405 + 501 类 with_status 裁决 :13-17）。

**server 侧 · tableData 域现场**

18. `crates/datazen-server/src/http/domains/table_data.rs:19-20`（模块头状态表两条 ⛔ 行）、`:109-133`（TableDataState 零件清单）、`:158-213`（router 挂载，变更二路由 :197-204）、`:324-328`（读面授权语注释）、`:337-396`（resolve_read_target 闸序）、`:398-405`（profile_unavailable 404 防枚举）、`:412-422`（table_read_error_response 先例）、`:429-475`（读 handler 形态先例）、`:547-648`（estimate 会话面闸序先例）、`:666-677`（fail_closed）、`:679-695`（REASON_* 常量，REASON_PLAN :689-692 / REASON_COMMIT :693-695）、`:719-728`（两条 501 handler）。
19. `crates/datazen-server/tests/table_data_routes.rs:757-787`（fail-closed 探针——t9 翻新对象）、`:875-890`（FixedLeaseSource 先例）、`:903-1109`（wired_fixture）、`:1088-1096`（TableDataState 实参——lease 补行处）、`:1114-1161`（open_session / read_body 夹具先例）。
20. `crates/datazen-server/tests/table_columns_source.rs:69-292`（WireMockFactory + ColumnSourceFixture + 真 runner 全栈夹具）、`:143-150`（source() 装配先例——刀3 handler 同款）、`:359-364`（commandResult 产物形状）。
21. `crates/datazen-server/tests/cm63_lease_continuity.rs:293-317`（真 open_session 用例先例）、`:347-366`（跨 resolve 会话连续断言先例）。

**桌面宿主 · 组装镜像**

22. `crates/datazen-desktop/src/commands/data.rs:22-53`（preview_pending_changes：闸 → 驱动 → 列源 → preview_changes）、`:55-112`（commit_pending_changes_with_handle：闸 :67-77 → 驱动 :78-82 → runtime 单实例 :83-89 → facts :90-96 → artifacts :98-99 → commit_changes :100-111）、`:114-132`（Tauri 命令面：manual_handle 独立参数）。
23. `crates/datazen-desktop/src/commands/error.rs:477-488` —— From<TableEditingError> for CommandError（桌面错误投影先例）。
24. `crates/datazen-desktop/src/commands/data/tests.rs:247-275` —— 四个带码 commit 拒绝的断言先例（grid.commit.* 码前缀）。

**web 前端契约（零改动证明）**

25. `packages/backend-client/src/tableDataClient.ts:91-94,99-102`（MethodMap 两键）、`:197,204`（方法签名）、`:273-277`（guard 委托）。
26. `packages/backend-client/src/types/tableData.ts:79-86`（TableChangeContext nullable 注记）、`:98-115`（PendingRowChange + rowIndex UI 字段）、`:146-174`（RowChangePlan + configRevision 十进制字符串注记 :149-167）、`:184-197`（CommitStatementResult / CommitPendingChangesResponse）、`:200-203`（PreviewPendingChangesRequest）、`:212-217`（CommitPendingChangesRequest，manualHandle :214）。
27. `packages/backend-client/src/types/session.ts:51-55` —— SessionHandle wire 形状。
28. `packages/web/src/platform/web/httpRoutes.ts:404-405` —— 两条路由（无 body 变换）；`backendTransport.ts:174-191`（非 2xx → deserializeApiError）、`:227`（payload 原样出站）；`webUnsupportedMethods.ts:32-33`（CAPABILITY_EXCLUDED 只有 open_base64_with_dialog——两键不在 unsupportedMethods）。
29. `packages/frontend/src/lib/session/tableSession.ts:12-18` —— tableManualHandle（transactionState active → handle；web 无事务面 → 恒 null 的现实依据）；`packages/frontend/src/commands/database.ts:44-49,208,252`（manualHandle 透传与 null 归一）。
30. `docs/architecture/platform/http-api-contract.md:145-154` —— §6.8 变更两段式（:150 commit 的 ✓ 令牌列 + 200 同步回执裁决 :154）。

**驱动面 · 写命令语义**

31. `crates/datazen-driver-api/src/traits/command_api.rs:35-38`（get_table_schema requires_object）、`:109-124`（execute 的 boundParameters 原生绑定 → rowsAffected）。
32. `crates/datazen-driver-api/src/mock_driver.rs:89,124`（query_rows / execute_rows_affected 选项）、`:463`（execute_with_params_calls 观察面）、`:725-764`（get_table_schema 应答）、`:990-1015`（execute_with_params 返回）。

---

## 9. 交付清单与提交纪律

- **刀3**：`crates/datazen-server/src/http/domains/table_data.rs`（状态表 + DTO + 两 handler + 错误映射 + lease 字段）+ `crates/datazen-server/src/bootstrap.rs`（1 行）+ `crates/datazen-server/tests/table_data_routes.rs`（探针翻新 + 夹具补行）+ `crates/datazen-server/tests/mutation_pair_routes.rs`（t1–t10）。共享层零改动、Cargo.toml 零改动、前端零改动。
- **验证**：`cargo test -p datazen-server --features mcp,use-case-contracts`（t0 基线 → 落地全量）；`corepack pnpm vitest run`（前端零改动回归）。
- **台账**：落地 PR 在 `web-ready-plan.md` §11 顶部加条目（w8-handoff §3 纪律 5）；每刀落地立即 push（w8-handoff §3 纪律 2）。
