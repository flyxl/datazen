//! Redis Tauri plugin setup.
//!
//! Redis operations execute through the generic Driver Command API
//! (`execute_driver_command`). This plugin only installs a Pub/Sub event sink
//! so subscription messages can reach the frontend.

use std::sync::Arc;

use tauri::Emitter;

use crate::ops_pubsub::{self, RedisPubSubMessageEvent};

/// Register the Redis plugin. No `plugin:redis|*` commands are exposed.
pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("redis")
        .setup(|app, _api| {
            let handle = app.clone();
            ops_pubsub::set_pubsub_emitter(Arc::new(move |event: RedisPubSubMessageEvent| {
                let _ = handle.emit(ops_pubsub::EVENT_NAME, &event);
            }));
            Ok(())
        })
        .build()
}
