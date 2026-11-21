use serde_json::Value;

use super::protocol::ConnectionConfigLike;
use crate::ThemeSnapshot;

pub trait WappBridgeHost {
    fn now_ms(&mut self) -> i64;

    fn new_nonce(&mut self) -> String;

    fn theme_snapshot(&self) -> ThemeSnapshot;

    fn load_connections(&self) -> Vec<ConnectionConfigLike>;

    fn active_connection_id(&self) -> Option<String>;

    fn storage_get(&self, wapp_id: &str, key: &str) -> Option<Value>;

    fn storage_set(&mut self, wapp_id: &str, key: &str, value: Value);

    fn storage_remove(&mut self, wapp_id: &str, key: &str);

    fn execute_driver_command(
        &self,
        wapp_id: &str,
        connection_id: &str,
        command: &str,
        args: Value,
    ) -> Result<Value, String>;

    fn audit_log(&self, wapp_id: &str, api: &str, detail: &str);

    fn show_notification(&self, title: &str, body: Option<&str>) -> Result<(), String>;

    fn resolve_string(&self, wapp_id: &str, key: &str, locale: &str) -> Option<String>;

    fn session_register(&mut self, wapp_id: &str) -> Result<String, String>;

    fn session_invoke(&self, request: &Value) -> Result<Value, String>;

    fn session_end(&self, instance_id: &str);
}
