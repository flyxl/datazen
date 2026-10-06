//! 隧道共享与引用计数（CM-32）。
//!
//! # 本模块解决什么
//!
//! **两个 session / Job 共用同版本隧道**，关掉其中一个时**不得**把隧道一起关掉；
//! 只有**最后一个引用**释放才真正拆除；隧道失败要传播给**全部**依赖资源。
//!
//! # 边界（重要）
//!
//! * 本模块**不是** tunnel 的发明者。词汇表与端口签名在
//!   `datazen_platform_api::ports::network`（**冻结上游，不得修改**）：
//!   [`TunnelSpec`] = 共享身份，[`TunnelBinding`] = 观测句柄，
//!   `NetworkProvider::{ensure_tunnel, release_tunnel}` = 契约。
//!   本模块是那份契约的一个**实现方**。
//! * 本模块**不认识** SSH / HTTP 代理 / WebSocket 任何一种隧道。宿主枚举
//!   （`driver-api::tunnel_types::TunnelKind` 与 `src-tauri::tunnel::Tunnel`）是
//!   **宿主实现细节**，决定「用哪种开法」，**不参与共享判定**，也不泄漏进来。
//! * 桌面 `NetworkProvider` 实现**不在**本轨范围：隧道生命周期由
//!   `NetworkProvider::ensure_tunnel` 的桌面实现承接，那是「逐 consumer 迁移后
//!   删除旧路径」的过渡实现，本模块不碰。
//! * CM-27 / CM-28 只**部分**覆盖，**不得**标记为已覆盖。`tunnel/` 关掉的只是隧道那几格：
//!   CM-27 的「隧道引用正确」（建隧道失败不落账、隧道开成后回滚释放）与 CM-28 的
//!   「隧道不多减引用」（重复释放幂等、`refs` 不低于 0、`close` 至多一次）。
//!   CM-28 四条断言里，**并发**维度由 `tests/cm28_concurrent_tunnel.rs`（真实
//!   `Barrier` + `tokio::spawn`，20 路并发）钉住，`tests/cm28_concurrent_release.rs`
//!   钉住另外三条里属资源层的两条（driver close 至多一次、预算不负数、
//!   重复响应一致）。
//!
//!   ★ 登记项 F（本轨更正）：上面那两个文件**进 CI**。原先记在这里的
//!   「CI 不会跑本轨新增测试」是**错的**——CI 的 crate 测试脚本不带任何 target
//!   选择器，cargo 因此跑选中 crate 的**全部** target，`packages/runtime` 顶层
//!   全部集成二进制（含上面那两个）再加上 `--lib` 那条产出的库单测二进制都在内；
//!   `--lib` 与 `--release` 两条**保留**，是叠加不是替代。crate 是按 layer 表
//!   （`packages/runtime` → crate `datazen-runtime`）发现并选择的，字面 crate 名
//!   不出现在命令里是设计如此，不是漏跑。
//! * **不在本模块、且此前被误记在这里**的一格：CM-27 的「可确认关闭的资源许可
//!   归零」。该格的主语是**资源许可**，不是隧道引用，已由三处闭合并各自带测试：
//!   `platform-api/src/ports/budget/pool_ledger.rs` 里 `impl Ledger` 的 **`release`**
//!   ——取出 permit、摘行、调 `uncharge` 归还占用、出 `ReleaseDisposition::Closed`，
//!   而该方法内的注释自述「本端口只有『真的 close 了』这一条释放路径」。
//!   （对外的宿主类型是 `InMemoryDriverPoolBudget`，内部账本类型才叫 `Ledger`；
//!   本 crate 里**不存在** `PoolLedger` 这个类型名。）
//!   另两处：`runtime/src/budget/ledger.rs` 的 permit 幂等核销（INV-10，名额按原槽
//!   退回）、`runtime/src/budget/coordinator.rs` 的端口级幂等核销（把 `Unknown`
//!   报成 `NotFound`）。三处都不知道隧道存在，隧道也不该进这一格。
//! * **本轨已闭合**的两格（原登记为「仍未闭合，且是实现缺失」，是**错的**，本轨更正）：
//!   (1) permit / socket / tunnel / handshake / init / register **全阶段失败矩阵**下的
//!   隧道引用回滚。生产接线在 `runtime/src/resource/tunnel_wiring.rs`：
//!   `acquire_tunnel_reference` 负责隧道阶段「失败不落账」（`TunnelLedger::acquire`
//!   在 `open` 失败时既不建条目也不加计数），`roll_back_unpublished` 负责
//!   握手 / 初始化 / 注册三阶段失败后的逆序补偿，引用归零只经 `settle_tunnel_reference`
//!   → `TunnelLedger::return_resource` 这一条路；六个阶段的矩阵由
//!   `tests/tunnel_wiring_contract.rs` 逐阶段钉住。
//!   (3) `CleanupDisposition::Quarantined`（未确认关闭进入隔离）下隧道引用与物理预算的
//!   **归属配对**。判据是 `runtime/src/resource/cleanup.rs` 的
//!   `CleanupDisposition::releases_physical_budget`（只有 `Closed` 为真），执行点是
//!   `tunnel_wiring.rs` 的 `settle_tunnel_reference`（预算不释放则隧道引用原样保留、
//!   连台账都不碰），处置矩阵由 `tests/tunnel_budget_pairing.rs` 钉住。
//!   两格现在都是「有生产接线可断言」，不再是「实现缺失」。
//! * 两格当初缺的那根线早已接上。以下命令可复算，工作目录是 `packages/runtime`：
//!   `grep -rn TunnelLedger src --include=*.rs | grep -v '^src/tunnel/'`
//!   `| grep -vE ':\s*(///|//!|//)'`。`src/tunnel/` 之外的**生产**调用方只分布在
//!   `resource/manager.rs` 与 `resource/tunnel_wiring.rs` 两个文件。基线 `b096ffbb4`
//!   上这些调用方是 **0**（当时 `resource/` 里一个 `tunnel` 都没有；`TunnelLedger`
//!   在 `src/tunnel/` 之外只有一处文档注释提到），即接线确实是本轨新增的。
//!
//! # 「同版本隧道」= `TunnelSpec` 全等值
//!
//! 它**不是** [`PoolKeyGeneration`]，也**不是** `CacheRevision`：
//!
//! | 概念 | 回答什么 | 与隧道的关系 |
//! | --- | --- | --- |
//! | `PoolKeyGeneration` | 哪条物理连接可以被谁复用 | **无隧道字段** |
//! | `CacheRevision` | 慢结果能不能回填 | 无关（缓存代次） |
//! | **`TunnelSpec`** | **哪条隧道被多少持有者共用** | **就是它** |
//!
//! 共享键比 `PoolKeyGeneration` **更细**：同一 `network_route_revision` 内，
//! 两条 `TunnelSpec` 不同的连接**仍各开各的隧道**（上游 `network.rs` 已有测试锁定）。
//! 反向的安全性不必在此重写 —— 路由轮换后 `ResourceManager::current_pool_key`
//! 已用 `PoolKeyRotated` 拒绝旧代申请，跨代隧道本就不会被共享。
//!
//! # 归还接线点：`return_resource`
//!
//! 资源生命周期侧**只有一条**归还入口 [`TunnelLedger::return_resource`]：
//! 给出租约 ⇒ 恰好一次释放，走与 `release` **同一条**归零路径。
//! 它不是「可选接线」—— 台账刻意**不提供**「只摘依赖、不减引用」的口子，
//! 因为那样的口子必然泄漏一个谁都不会再还的引用。
//!
//! # 唯一计数铁律（CM-32-FU1：机械保证）
//!
//! 全系统**只有一份**隧道引用计数：`TunnelLedger` 的 `refs`。
//! `TunnelBinding.ref_count` 是观测快照，**任何**释放判断都不得读它。
//!
//! 这条铁律**不由类型系统保证**：`&self` 方法并不排除 `Mutex` / `Cell` 内部可变性
//! （两个夹具本身就在用 `Mutex`），编译器装得下第二份账。它如今由
//! `tunnel::single_counter_audit` 这道**测试期结构闸门**保证 —— 对台账与全部三份
//! 端口实现做源码结构审计（台账侧「计数只有一个读取面」、端口侧「不许自存一份账、
//! 读数只能是唯一折法的投影」、观测账不得参与判断），每条规则另带一条把原始反例
//! 当字符串喂给自己做断言的 kill test，外加一条「登记表不许被裁小」的自我约束。
//! 该模块是 `#[cfg(test)]`，随 `--lib` 跑，因此**在 CI 作用域内**（CI 现状见上面登记项 F）。
//! 边界：它挡得住自然写出来的第二本账，挡不住蓄意对抗（`unsafe` 指针转义、跨 crate
//! 静态、宏生成），口径见 [`transport`] 模块头末段。
//!
//! 归零路径 `drain()` 涉及的字段确实有**两个**：`refs`（唯一计数来源，按值取自
//! `TunnelEntry::take_reference`）与 `state`（终态守卫，只保证 `Closing` /
//! `Unconfirmed` 不再发第二次 close；它**不是**第二本账，不参与计数、不增减）。
//! 台账另有一份**观测**计数 `teardown_calls`（对外 `close_calls()`），
//! 禁止出现在任何判断条件里。代数不变量见 `journey_single_counter`。

//!
//! [`PoolKeyGeneration`]: crate::resource::PoolKeyGeneration

mod error;
mod ledger;
mod transport;

pub use error::TunnelError;
pub use ledger::{TunnelLease, TunnelLedger, TunnelRelease, TunnelState};
pub use transport::{TunnelFault, TunnelHandle, TunnelTransport};

/// 测试替身：记录式物理端口 + 可注入故障的隧道旅程。与 `resource::harness` 同一形态。
#[cfg(test)]
mod harness;

/// 三个注入点的失败传播旅程（CM-32 第三条断言）。
#[cfg(test)]
mod journey_failure;
/// 资源归还接线：`一次归还 = 恰好一次 release`。
#[cfg(test)]
mod journey_return;
/// CM-32 主旅程：两个 session 共用隧道（第一条、第二条断言）。
#[cfg(test)]
mod journey_sharing;
/// 唯一计数铁律的代数不变量。
#[cfg(test)]
mod journey_single_counter;
/// 唯一计数铁律（CM-32-FU1）的机械闸门 R1–R7：对台账与全部三份隧道端口做源码
/// 结构审计，并把原始反例与同类伪装（含「换名字挂一份镜像账」）作为植入变异喂给
/// 闸门自己做 kill test。住在 `src/tunnel/` 而非顶层 `tests/`，是因为它随 `--lib` 跑、
/// 因而真的进 CI（见上面登记项 F 记的 CI 现状）。
#[cfg(test)]
mod single_counter_audit;
/// 上面那道闸门的通用词法 / 结构基元（抹串、`fn` / `impl` / 字段解析）。
/// 单独成文件只为守单文件规模纪律，不含任何隧道语义。
#[cfg(test)]
mod source_scan;
