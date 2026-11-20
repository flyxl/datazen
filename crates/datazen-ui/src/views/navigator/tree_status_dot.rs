//! `treeStatusDot`, ported 1:1 from
//! `packages/frontend/src/windows/connection/navigator/treeStatusDot.tsx`.
//!
//! React renders an 8×8 (`h-2 w-2`) rounded pip next to a connection row's
//! name: yellow + `animate-pulse` + `title=conn.connecting` while connecting,
//! green + `title=conn.connected` when connected, red + `title=conn.failed`
//! on error, and **nothing** for `idle` (a never-opened row must not look like
//! it is reporting a status, and no space is reserved for it).
//!
//! The Rune port keeps the same three states and the exact locale keys. Colors
//! come from existing tokens only — kit's `warning.icon` and the palette's
//! `status_ok` / `status_fail` — so no new colour literal is introduced. The
//! CSS `animate-pulse` on the connecting pip has no Rune equivalent in this
//! port; the pip carries its state through colour and tooltip, and the pulse
//! can be layered on when a shared pulse animator lands (removal stage for
//! that gap). No icon is drawn: React's pip is a CSS span, not a lucide glyph.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts the pip yet. The connection-tree host will call
//! [`render_status_dot`] from its row builder with its own active-connection
//! slice; until then every reusable item carries a per-item `#[allow(dead_code)]`
//! with this removal stage.

use rune::{el, Color, El};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::palette;

/// React `ConnectionStatus` (`activeConnectionStore.ts`). `Idle` includes the
/// "no entry in activeConnections" case (`?? 'idle'`).
#[allow(dead_code)] // VM mirror; consumed by the tree host and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ConnectionStatus {
    #[default]
    Idle,
    Connecting,
    Connected,
    Error,
}

impl ConnectionStatus {
    /// React's `activeConnections[connectionId]?.status ?? 'idle'`.
    #[allow(dead_code)] // pure helper; consumed by the renderer and tests.
    pub(crate) fn from_str(value: Option<&str>) -> Self {
        match value {
            Some("connecting") => Self::Connecting,
            Some("connected") => Self::Connected,
            Some("error") => Self::Error,
            _ => Self::Idle,
        }
    }

    /// Exact `title` / aria locale key React attaches to each pip.
    #[allow(dead_code)] // pure mapping; consumed by the renderer and tests.
    pub(crate) fn aria_key(self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Connecting => Some("conn.connecting"),
            Self::Connected => Some("conn.connected"),
            Self::Error => Some("conn.failed"),
        }
    }

    /// React's `bg-yellow-400` / `bg-green-500` / `bg-red-500`, mapped to the
    /// existing kit/palette status tokens. `Idle` draws nothing (`None`).
    #[allow(dead_code)] // pure mapping; consumed by the renderer and tests.
    pub(crate) fn fill(self, dark: bool) -> Option<Color> {
        match self {
            Self::Idle => None,
            Self::Connecting => Some(kit(dark).c.warning.icon),
            Self::Connected => Some(palette::status_ok(dark)),
            Self::Error => Some(palette::status_fail(dark)),
        }
    }

    /// React's `animate-pulse` only applies to the connecting pip.
    #[allow(dead_code)] // parity helper; consumed by the tests until a pulse animator lands.
    pub(crate) fn pulses(self) -> bool {
        self == Self::Connecting
    }
}

/// React `renderStatusDot(connectionId, activeConnections, t)`. The host-owned
/// slice mirrors `Record<string, ConnectionEntry | undefined>`; a missing id
/// resolves to `Idle` and renders no element at all.
#[allow(dead_code)] // consumer: the connection-tree host (not landed).
pub(crate) fn render_status_dot(
    connection_id: &str,
    active: &[(String, ConnectionStatus)],
    dark: bool,
) -> Option<El> {
    let status = active
        .iter()
        .find(|(id, _)| id == connection_id)
        .map_or(ConnectionStatus::Idle, |(_, status)| *status);
    let key = status.aria_key()?;
    let color = status.fill(dark)?;
    let title = t(key);
    Some(
        el()
            .key(format!("tree-status-dot-{connection_id}"))
            .size(8.0, 8.0)
            .radius(4.0)
            .shrink()
            .fill(color)
            .label(title)
            .help(title),
    )
}
