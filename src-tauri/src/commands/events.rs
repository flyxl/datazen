//! 会话事件流订阅 IPC（当前阶段显式未实现）。
//!
//! 与 `platform/adapter.rs` 口径一致：桌面运行时尚无真实事件流
//! （`stream_id` / epoch / 幂等记录均未接线），因此 `subscribe_events`
//! 必须显式报错，而不是假成功、空流或静默 `Ok(())`——静默的空壳比
//! 显式报错危险得多。`stop_event_subscription` 幂等返回 `Ok(())`，
//! 允许前端在恢复/重置路径上总是安全地停订阅。

use tauri::ipc::Channel;

use super::error::CommandError;

#[tauri::command]
pub async fn subscribe_events(
    _request: serde_json::Value,
    _subscription_id: String,
    _on_event: Channel<serde_json::Value>,
) -> Result<(), CommandError> {
    Err(CommandError::NotConfigured(
        "事件流在当前阶段不可用（未实现）：subscribe_events 没有事件源，\
         与 platform/adapter.rs 的“为什么 ApplicationServices 仍然是空的”口径一致"
            .into(),
    ))
}

#[tauri::command]
pub async fn stop_event_subscription(_subscription_id: String) -> Result<(), CommandError> {
    Ok(())
}
