//! A settings repository bound to an authorized scope by the host.
//! Server hosts must authorize execution-policy changes before binding/writing;
//! the desktop adapter binds this port to the local user's application data.
use crate::{settings::SharedSettings, PortError};
use async_trait::async_trait;
#[async_trait]
pub trait SettingsRepository: Send + Sync + 'static {
    async fn read_settings(&self) -> Result<SharedSettings, PortError>;
    async fn write_settings(&self, settings: SharedSettings) -> Result<SharedSettings, PortError>;
}
