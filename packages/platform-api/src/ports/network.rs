//! `NetworkProvider` 迁移垫片（D3，2026-10-26）。
//!
//! 网络能力端口（隧道 ensure/release 契约）与 `TunnelSpec`/`RoutePlan`/
//! `TunnelBinding` 的唯一定义自 D3 起在
//! `datazen-modules::execution::application::network`（§5.1：网络能力接口归
//! 实际消费者的 application；SSH/代理/WebSocket 的物理开法由宿主实现）。
//! 本模块只再导出，旧 import 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::network::{
    endpoint_scheme, NetworkProvider, RoutePlan, TunnelBinding, TunnelSpec,
};
