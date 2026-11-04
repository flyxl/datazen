//! The storage port for named views.
//!
//! The desktop host stores the views in `{appData}/named_views.json`; a
//! browser build has no such place yet. The port is what both would
//! implement, so the rules above it do not have to know which one they run
//! in (08 §3.7, T0-12).

use super::dto::NamedView;
use super::error::NamedViewError;

/// Reads and writes the collection of named views.
///
/// The port is deliberately **whole-collection**: the desktop store writes one
/// file atomically, so a per-record port would either have to invent a
/// second storage model or lose that guarantee. A form factor that stores the
/// views elsewhere implements the same two calls.
#[async_trait::async_trait]
pub trait NamedViewRepository: Send + Sync {
    /// Every stored view, across every scope.
    ///
    /// A record that cannot be read is skipped rather than fatal: one damaged
    /// entry must not hide the views around it.
    async fn load_views(&self) -> Result<Vec<NamedView>, NamedViewError>;

    /// Replace the stored collection with `views`.
    async fn save_views(&self, views: &[NamedView]) -> Result<(), NamedViewError>;
}

/// Mints the identity of a new view.
///
/// The id is the host's business — the desktop mints a ULID, so the ids sort
/// by creation time — and the shared layer only asks for one.
pub trait NamedViewIdMinter: Send + Sync {
    /// Mint an id that no other view carries.
    fn mint_id(&self) -> String;
}
