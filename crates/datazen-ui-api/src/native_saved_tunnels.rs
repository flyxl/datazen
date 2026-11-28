//! Native Settings' supported saved-tunnel slice. No connection/probe capability
//! is implied by a persisted definition.
use crate::HttpProxyTunnelConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTunnelStoreError {
    Unavailable,
    Unreadable,
    Unwritable,
    InvalidDefinition,
    MissingOrUnsupported,
    Credential,
}

/// Only public fields ever reach the editor. A saved password is not returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeHttpProxyDefinition {
    pub id: String,
    pub name: String,
    pub proxy: HttpProxyTunnelConfig,
    pub password_configured: bool,
}

/// An explicit edit intent. In particular, an empty/redacted field does not
/// erase a saved password. Do not derive Debug or serialize this type.
pub enum NativeTunnelPassword {
    Keep,
    Clear,
    Replace(String),
}
