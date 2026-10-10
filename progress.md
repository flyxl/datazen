# progress.md — P3 reset capability gate

分支 `feature/p3-reset-capability`，base `8dae098cc`。
本文件是并行开发期进度台账（AGENTS.md 允许，验收合并时删除）。

## 状态

| 项 | 状态 |
| --- | --- |
| progress.md 首提交 | 完成 |
| 依赖方向论证 | 完成 |
| 设计定稿 | 完成（见下） |
| 实现 | 进行中 |
| 门禁 A–G | 未开始 |

## 依赖方向论证（任务 1）

**结论：不新增依赖，也不使用 `datazen-driver-api::capabilities::ResetForReuse`。**

事实：
- `packages/runtime` **已经**依赖 `datazen-driver-api`（`packages/runtime/Cargo.toml` `[dependencies]`），
  方向 `runtime → driver-api`，单向，`driver-api` 不反向依赖 runtime（其 `[dependencies]` 无任何 datazen 成员）。所以「不依赖」这个前提不成立，没有强行加依赖的问题。
- 但**真正的落点是 runtime 自己那一份**：`packages/runtime/src/connection/capability.rs:65` 定义
  `ResetForReuse { Verified, Unsupported }`，:175 `reuse_path_verified()` 的 doc 写的就是
  「归池复用是否被驱动验证过。未验证时**一切复用路径必须走关闭**」。
  `packages/runtime/src/connection/types.rs:91-95` 明确记录了同名物**刻意并存、不合并**的判据
  （driver 侧 vs 宿主观测侧是两个维度）。资源层用宿主侧那份，不越界去读插件契约那份。
- 两份 enum 的 **Default 相反**：driver-api 侧 `Default = Unsupported`（失败安全）；
  runtime 侧 `CapabilitySnapshot::defaults()` 是 `Verified`（:145）。这正说明它们是两份不同的东西。

生产供给侧现状（已定点 grep 确认）：
- `connection/capability.rs` 的 `CapabilitySnapshot` 通过 `connection/port.rs:63`
  `ResourceDescriptor::capabilities` 进入资源层 DTO，**但从未进入归池判定**。
- `resource/cleanup.rs` 的 `DriverCleanVerdict` 只有一个私有 bool（:27），宿主看不到判定依据。
- `require_reset_for_reuse`（`packages/driver-api/src/capabilities.rs:594`）在 `packages/runtime/src/` 里 0 处引用。

方案：**复用既有通道**（`crate::connection::capability::ResetForReuse` + `HostConditionSnapshot`），
不引入窄能力端口 trait。理由：宿主侧已经有一条现成的、同 crate 内的通道，且
`FixedSessionNotPoolable` 已经示范了「非阻断项挂在 `HostConditionSnapshot` 上、由
`CleanupPlan::of` 强制置位」的先例（`cleanup.rs:322-324`），再加一个端口 trait 是平行造第二套机制。

## 设计定稿

1. `resource/cleanup.rs` 新增 `ReturnReason::DriverResetUnsupported`（命名反映**能力拒绝**，
   不是运行时故障）。`reason_code() = "driverResetUnsupported"`。
2. `HostConditionSnapshot` 新增第 6 个字段 `reset_for_reuse: ResetForReuse`。它**不是**宿主四项之一，
   与 `fixed_session` 同属「用途/能力」维度的非故障项，doc 里写明这一点。
   `impl Default for ResetForReuse = Verified`，与 `CapabilitySnapshot::defaults()` 对齐：
   这样 27 处既有 `HostConditionSnapshot::default()` 调用点行为不变（否则全部翻成 Closed，基线必崩）；
   且「忘记供给」的失败模式 = 今天的失败模式（跑一次注定失败的复位再关），不引入新风险。
3. `ReturnDecision::decide(driver, host, ...)` 与 `CleanupPlan::of(record, host, driver)` 的签名
   **不变**（能力走 `host` 通道，与 `fixed_session` 同构）。能力闸放在 `decide` 的**最前**：
   能力是声明阶段就确定的事实，比运行时 `Clean` 更根本；`Unsupported` 时直接
   `CleanupRequired(DriverResetUnsupported)`，归池候选当场作废。
4. `CleanupPlan::of` 里 `admits_to_idle_pool()` 为假时落点仍是 `Closed` → `release` 里
   `plan.disposition == ReturnedToPool` 那个分支不成立 ⇒ **不调用 `transport.reset`**（任务 2 的「省掉往返」）。
   `release` 的三态 match 与 `needs_reset` 那段一律不碰。
5. `Verified` 路径：能力闸直接放过，后面仍是 `driver.is_clean()` + `host.first_blocker()` 的完整双闸，
   且 `release` 的复位闸照旧（任务 4）。

## 待裁定 / 取舍

- **`needs_reset` 边界**：`Unsupported` + 仍有未释放句柄时，`release` 里那段「先在原资源上回滚再关」
  仍会调一次 `reset`（它失败 ⇒ 隔离）。这是**刻意保留**的保守行为：不能为了省一次往返就在事务还开着时静默关闭。
  改动只落在 `CleanupPlan::of` / `ReturnDecision::decide` 一侧，符合任务书「别碰」的边界。
- `ReturnReason` 变体新增 ⇒ 需检查 `reason_code()` 全量 match 是否还有别处（`src-tauri` 侧有无序列化依赖）。

## 门禁

基线：在自己 HEAD `8dae098cc` 上实测（另一轨未合流，数字以本轨实测为准）。
