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

## 取舍与待裁决

（边做边记）
