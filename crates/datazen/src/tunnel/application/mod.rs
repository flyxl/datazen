//! 隧道应用层：隧道管理 UI 背后的形态中立用例。
//!
//! 用例消费 [`domain`](super::domain) 的端口（[`TunnelRepository`] /
//! [`KnownHostsRepository`]）与机制层（SSH / HTTP 代理 / WebSocket 的本地
//! 腿），不碰宿主状态；桌面命令壳与未来的 web 服务端 handler 是同一用例的
//! 两个薄绑定（WR-A2：编排归 application，宿主只留装配与错误映射）。
//!
//! * [`probe`]——「测试隧道」按钮背后的探测编排；
//! * [`full_definitions`]——全量名单的两档组合。
//!
//! 投影规则（[`summarize`] / [`usage_from_connections`]）刻意**不在**这里：
//! 它们的消费者是**仓储适配器**（桌面 `store/tunnels.rs` 与测试替身的
//! `list` / `usage` 实现），不是用例——端口承诺形状，映射贴着端口住在
//! domain 层。结构裁决（用例归 application、投影留 domain）记录于
//! [web-ready-plan §11](../../../../docs/development/web-ready-plan.md)。
//!
//! [`TunnelRepository`]: super::domain::TunnelRepository
//! [`KnownHostsRepository`]: super::domain::KnownHostsRepository
//! [`summarize`]: super::domain::summarize
//! [`usage_from_connections`]: super::domain::usage_from_connections

pub mod full_definitions;
pub mod probe;

pub use full_definitions::list_full_definitions;
pub use probe::test_saved_tunnel;
