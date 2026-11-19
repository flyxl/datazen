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
//! `permissionLabels.ts` (the permission badges' tooltip texts) is T484 and
//! still ahead.

pub(crate) mod install_wapp_dialog;
pub(crate) mod wapp_card;
pub(crate) mod wapp_management_page;
pub(crate) mod wapp_summary;

pub(crate) use install_wapp_dialog::*;
pub(crate) use wapp_management_page::*;
pub(crate) use wapp_summary::*;
