//! 桌面形态的驱动事件出口实现。
//!
//! ## 这一层解决什么
//!
//! 驱动可能产生「给人看的事件」——Redis 驱动的 Pub/Sub 消息是现成例子。
//! 事件怎么送达用户**不是驱动能决定的事**：桌面形态用 Tauri 事件发给前端，
//! server 形态会用它自己的传输通道。驱动侧只认
//! [`datazen_driver_api::DriverEventSink`] 这一个形态中立契约，因此同一份
//! 驱动 crate 在两种形态里都能用，驱动代码一行都不用改。
//!
//! 改造前这条通道是 Tauri 插件（`packages/drivers/redis/src/plugin.rs`），
//! 于是 `tauri` 进了 redis 驱动的可选依赖闭包 —— server 形态要支持
//! Redis Pub/Sub 就必须启用 `tauri-plugin` feature，直接撞上
//! `docs/architecture/platform/shared-boundaries-and-ports.md` §2.2 对 server
//! 的禁令。**Pub/Sub 曾经是「桌面独占」能力，根因就是注入通道的形态。
//!
//! ## 语义边界
//!
//! 事件名与 payload 都由驱动给出，这里逐字转发：前端看到的字段与驱动产出
//! 的字段一致，sink 不做重命名、不做包装、不做裁剪。
//!
//! ## 为什么装在启动期最前处
//!
//! sink 的安装是**运行时**决定（宿主在启动时选一个实现装进去），不是链接期
//! 常量。装得越早，「事件先到、sink 后装」的窗口越短；而对 Redis Pub/Sub
//! 来说这个窗口里消息的处理方式与改造前一致 —— sink 未装时消息被**静默
//! 丢弃**，不缓存、不补发（见 `packages/drivers/redis/src/ops/pubsub.rs`）。

use std::sync::Arc;

use datazen_driver_api::{DriverEvent, DriverEventSink};
use tauri::{AppHandle, Emitter};

/// 把驱动事件转成 Tauri 事件发给前端的 sink。
///
/// 这是**桌面形态**的实现：事件名即 Tauri 事件名，payload 原样作为事件体。
pub struct TauriEventSink {
    handle: AppHandle,
}

impl DriverEventSink for TauriEventSink {
    fn emit(&self, event: &DriverEvent) {
        if let Err(error) = self.handle.emit(event.name, &event.payload) {
            tracing::warn!(event = event.name, error = %error, "驱动事件发送失败");
        }
    }
}

/// 把桌面形态的 sink 装进本次构建链接进来的每一个驱动事件出口。
///
/// 宿主在启动期**不知道**本次构建链接了哪些驱动 —— 驱动是编译期选型（像
/// Caddy 2 的编译期插件），宿主也不可能静态引用某个具体驱动的安装函数。
/// 所以收集靠 `inventory` 遍历：链接进来的驱动自己声明出口，宿主遍历装载；
/// 没链接的驱动自然不在其中，装了也不会有副作用。
pub fn install_driver_event_sinks(handle: &AppHandle) {
    let sink: Arc<dyn DriverEventSink> = Arc::new(TauriEventSink {
        handle: handle.clone(),
    });
    let mut installed = 0usize;
    for outlet in datazen_driver_api::iter_event_outlets() {
        tracing::info!(outlet = outlet.outlet_id(), "安装驱动事件出口 sink");
        outlet.install(sink.clone());
        installed += 1;
    }
    tracing::info!(installed, "驱动事件出口 sink 安装完成");
}
