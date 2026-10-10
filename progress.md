# P3 Runtime Wiring — 进度台账

分支 `feature/p3-runtime-wiring`，基准 HEAD `f5a33b7dc2d4cfd1f033686bdd33a9ebd8aad0b5`（
与任务给定的 `f5a33b7dc` 同指一提交）。
工作树 `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p3-runtime-wiring`。

本轨是重派：前任代理零改动、死在读代码阶段、无台账。接手时 `git status --short` 为空，
无残留文件需要报告。

## 状态

- [ ] 任务 1：接上 `with_live_connections`
- [ ] 任务 2：接上 `start_idle_reclaimer`
- [ ] 任务 3：`SessionRegistry` 暴露设置 idle deadline 的公开入口
- [ ] 任务 4：`ExecutionGateway` 是否接线的判断
- [ ] 门禁 A~G
- [ ] 陷阱三问

## 已核实事实（不要盲从侦察结论，逐条以本表为准）

### 端口契约

- `with_live_connections(&self, source: Arc<dyn LiveConnectionSource>) -> &Self`（`application/mod.rs:169`）。
  重复调用是覆盖不是叠加。`LiveConnectionSource::open_live(request)` 唯一责任：
  把**已经活着**的物理引用交出来，`request.handle` 带 `runtime_epoch`。
- `LiveConnection` 携带两个字段：`Arc<dyn DatabaseDriver>`（驱动**注册表项**的共享引用，
  不新建物理连接）+ `ConnectionHandle`（值拷贝路由元数据 `{ id, pool_id }`，不持 OS 资源）。
- `DesktopSessionBackend`（`session_backend.rs:26`）**不具备**实现该端口的能力：它的
  `resources: tokio::sync::RwLock<HashMap<String, Arc<Resource>>>` 的 key 是**不透明
  resource_id**（`uuid`），而端口入参只有 `SessionHandle { db_session_id, runtime_epoch }`。
  从 handle 反查 resource 需要一张 dbSessionId → resource 的映射，`DesktopSessionBackend`
  目前**没有**这张表。因此需要一个新的出向端口适配器，不是「接 DesktopSessionBackend」。
  → 落点 `src-tauri/src/platform/live_source.rs`（新文件）。

### 空闲回收

- `start_idle_reclaimer(self: &Arc<Self>) -> IdleReclaimer`（`application/idle.rs:172`）。
  返回 `IdleReclaimer { stop: Arc<AtomicBool>, task: JoinHandle<()> }`。
  `stop()` 只翻标志，任务在下一轮循环边界退出；`Drop` 翻标志 + `abort()`，两者幂等。
  「进程退出时能停」由 `Drop` 结构保证，宿主只需把句柄**持有一个可确定生命周期的作用域**。
- 宿主侧启动位置：`PlatformAdapter::inject` 只被 `finish_app_state` 调用一次，且
  `runtime: OnceLock` 保证只注入一次。把句柄存进 `OnceLock<IdleReclaimer>` 太脆（OnceLock
  无 `Drop` 前取回手段），所以放进 `AppState`：`finish_app_state` 已有 `adapter.inject(...)`
  的调用点，紧邻即可。

### idle deadline 公开入口

- `registry/actor.rs:696` 的 `fn set_idle_deadline` 私有，唯一入口是
  `ExecCommand::SetIdleDeadline`（`actor.rs:502` 附近的分发），`SessionRegistry` **没有**
  对应公开包装。同理 `ExecCommand::RegisterHandles` 也没有——这确认了「缺包装」是既有的
  系统性缺口，不是 `SetIdleDeadline` 一个。
- 加公开包装需落在 `packages/runtime/src/registry/registry.rs`（`SessionRegistry` 的 impl
  块所在），与 `close_registered` / `submit_execution` 的形状一致。`registry/**` 在
  `mod.rs` 的模块注释里被描述为本 crate 的登记表实现，任务书把「暴露公开入口」列为任务 3，
  因此**在本轨可写范围内**（与「不动 `packages/runtime/src/resource/**`」是两件事）。

### ExecutionGateway

- `src-tauri/src` 内引用数 **0**（grep 实测）。
- 但它是**已被接线的**：`packages/runtime/src/application/mod.rs:133` 构造
  `ExecutionGateway::new(...)`，`application/operations.rs:47/92/107` 三处经门面 accept /
  dispatch / resolve_unknown_outcome。也就是说桌面侧的 `execute_in_session` IPC
  早已走 `RuntimeConnectionUseCases::enqueue → ExecutionGateway::accept`。
  "引用数 0" 只是因为宿主**不需要**直接引用它——门面已经把网关藏在后面。
  → 任务 4 的答案是「已隐式接线，无需也不能在宿主侧新增直接引用」，理由见最终报告。

### 归池（陷阱）

- 15 个路径驱动全部 `ResetForReuse::Unsupported`，`require_reset_for_reuse` 在
  `packages/runtime/src/` 里 0 处引用（grep 实测；命中全在 `packages/driver-api/`）。
- §9.4 归池前检查**不在本轨路径上**：本轨只把 runtime 接到宿主启动装配里，不动归池判定。
  唯一与归池相邻的是 `DesktopSessionBackend::close`（`session_backend.rs:442-445`）里的
  `let _reset = provider.reset_resource(...)` —— 它把 `ResetDisposition` **丢弃**然后直接
  `close_resource`。这是既有形状，不是本轨引入；本轨不改它（另轨在接能力判定进归池），
  但在报告里记账。

## 门禁实测

（跑完再填，逐字抄结论行）

## 实现记录（按提交顺序）

### 提交 2 `5b1ae9154`：任务 3，`SessionRegistry::set_idle_deadline`

- `registry/registry.rs`：加公开包装，形状与 `cancel_execution_bound` 同形。
- `application/idle.rs`：加 `refresh_idle_deadline()`（业务活动触发，非周期任务）；
  模块头的「已知缺口」改为已闭合事实。
- `registry/registry/tests.rs`：5 条新用例，**不靠 sleep**——期限时间轴是注入的三个
  刻度（PAST/SOON/FAR），判据是行为差：设未来则不动手、改回过去则动手、设 None 则
  永不回收、已摘行则 `UnknownSession`、epoch 不匹配则 `UnknownSession`。

### 提交 3：任务 1+2，宿主侧接线

- `packages/runtime/src/registry/backend.rs`：`OpenRequest` 的 `db_session_id` 透传
  进 `OpenRequest`（`backend.rs` 的 `OpenResource`）。**理由**：没有它，宿主的
  `open()` 认不出「这一份资源将来归哪个会话」，出向端口只剩「新建连接」与「永远
  拒绝」两条路。
- `packages/runtime/src/registry/actor.rs:557`：构造 `OpenResource` 时带上同一个 id。
- `src-tauri/src/platform/session_backend.rs`：加 `by_session`（dbSessionId →
  resource_id）反向索引；`open` 里绑、`close` 里摘；`Resource` 只开三个投影
  （epoch / resource_key / provider_id），不开 `provider` 与 `handle` 本体。
- `src-tauri/src/platform/live_source.rs`（新）：`DesktopLiveConnectionSource`
  实现 `LiveConnectionSource`，四步（反查资源 → 解析权威 driver type → 取驱动
  引用 → 从资源投影 handle），每步失败都是拒绝，没有一步会新建物理连接。
- `src-tauri/src/platform/adapter.rs`：`inject` 里 `with_live_connections(...)` +
  `start_idle_reclaimer()`；`stop_idle_reclaimer()` 由 `RunEvent::ExitRequested`
  调用（`bootstrap/run.rs:614`）。
- `src-tauri/src/platform/live_source_tests.rs`（新）：接线生效的三条判据。

### 可观测性（陷阱三问的「建议」落点）

- `adapter.rs` 加 `log_runtime_wiring()`：启动时把 `live_connections` /
  `idle_reclaim_enabled` / `idle_reclaimer` 三项打成一行。前两项为 false 时是
  **配置事实**用 debug，第三项为 false 是**装配缺陷**用 warn（并说清后果：
  会话只建不回收、额度单调下降）。
- 刻意**不**打「pool reachable」这种会骗人的字段：首版空闲池在真实驱动上恒
  不可达是正确行为，不为了让日志变绿而谎报能力。
- `IdleReclaimer` 加 `is_finished()` 投影：没有它，「能被停、不泄漏 task」
  只有 `stop()` 一个 `()` 返回值可断言，等于不可测。

### 新测试（都钉行为，不钉「构造函数被调用过」）

- `registry/registry/tests.rs` +5：期限设未来则不动手、改回过去则动手、
  设 None 则永不回收、已摘行则 `UnknownSession`、epoch 不匹配则
  `UnknownSession`。期限时间轴是注入刻度，不靠 sleep。
- `tests/p3_live_port.rs` +2：起的扫描器停后 `is_finished()` 由 false 翻 true
  （不泄漏 task）；借了还再借，端口**真的被查了两次**（不是回放缓存）。
- `live_source_tests.rs`（新，主机侧）1 条主用例，三条判据同时成立：
  (1) 端口被真的查询一次，入参就是 runtime 带给它的 handle；
  (2) `ConnectionHandle.id` == 后端 open 发回的 `resource_key`，
      `pool_id` == `provider_id`；
  (3) `acquire_resource` 次数借出前后不变（没有新建物理连接），
      且 `reset_resource` 0 次（借出不触发复位）。

## 取舍与待裁决

1. **`ExecutionGateway` 不在宿主侧新增引用**（见最终报告）。
2. **`Resource::epoch()` 等三个投影是 `pub(super)`**：只给 `live_source` 用，
   不给整个 crate。`provider` 与 `handle` 本体不开——交出去等于把关闭权交出。
3. **`Box::leak` 出现在两处测试夹具**（provider 的 `capabilities()` /
   `namespace_shape()` 返回 `&'static`，`register_test_factory` 要 `'static`
   factory）。都是 `#[cfg(test)]`，生产路径没有泄漏。
4. **`unbind` 放在 `close` 而不是应用层 `close_session`**：`close` 是唯一能保证
   「关物理资源」与「摘绑定」同帧的地方。应用层先做会在 `CloseRejected` 时漏摘。
5. **待裁决：`refresh_idle_deadline` 的调用点还没有**。方法、测试、入口都齐了，
   但「哪一次业务活动算真实活动」需要与 P4 consumer 迁移一起定——本轨不迁
   consumer（明确不做），所以先只提供入口，不硬塞一个调用点。
