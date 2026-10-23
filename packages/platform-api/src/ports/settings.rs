//! A settings repository bound to an authorized scope by the host.
//! Server hosts must authorize execution-policy changes before binding/writing;
//! the desktop adapter binds this port to the local user's application data.
//!
//! `read_settings` / `write_settings` carry the typed, shape-neutral projection
//! used by the shared backend. The native desktop command also needs the full
//! local settings document (paths, onboarding and other host-owned fields). That
//! document stays opaque here: the host owns its wire type and the application
//! layer only moves it through this port, so no desktop DTO leaks into the
//! platform contract or gets duplicated in a second projection.
use crate::{settings::SharedSettings, PortError};
use async_trait::async_trait;
#[async_trait]
pub trait SettingsRepository: Send + Sync + 'static {
    async fn read_settings(&self) -> Result<SharedSettings, PortError>;
    async fn write_settings(&self, settings: SharedSettings) -> Result<SharedSettings, PortError>;

    /// Read the host-owned full settings document without exposing its concrete
    /// desktop type to the shape-neutral crates.
    async fn read_app_settings(&self) -> Result<serde_json::Value, PortError>;

    /// Persist the host-owned full settings document and return the stored
    /// snapshot. The host implementation validates/deserializes its concrete
    /// document before writing it.
    async fn write_app_settings(
        &self,
        settings: serde_json::Value,
    ) -> Result<serde_json::Value, PortError>;
}
