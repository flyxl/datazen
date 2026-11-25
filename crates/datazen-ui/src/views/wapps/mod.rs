//! Ports of `packages/frontend/src/windows/wapps/*`, one module per React
//! file (plus the one split the 421-line management page takes):
//!
//! - `install_wapp_dialog.rs` — `InstallWappDialog.tsx` (T022; the two-step
//!   install dialog: native pick → validate-only inspect → review →
//!   confirm).
//! - `wapp_management_page.rs` — `WappManagementPage.tsx` (T072; the page
//!   that hosts the dialog, owns the search/filter state, renders the
//!   installed cards and asks the uninstall confirmation).
//! - `wapp_card.rs` — the management page's `renderCard` + `WappCardIcon`,
//!   split out with the page's own module.
//! - `wapp_summary.rs` — the view models the page's module scope owns: the
//!   `WappSummary` projection, `WAPP_API_VERSION`, the
//!   workspace/theme predicates and the filter/search/counts/groups
//!   derivations.
//!
//! `permissionLabels.ts` (the permission badges' tooltip texts) landed as
//! T484's `permission_labels`.

pub mod install_wapp_dialog;
// T484: the permission badges' tooltip texts.
pub mod permission_labels;
pub mod wapp_card;
pub mod wapp_management_page;
pub mod wapp_summary;

pub use install_wapp_dialog::*;
pub use wapp_management_page::*;
pub use wapp_summary::*;
