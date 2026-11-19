//! Ports of `packages/frontend/src/windows/wapps/*` (T022), one module per
//! React file:
//!
//! - `install_wapp_dialog.rs` — `InstallWappDialog.tsx` (the two-step
//!   install dialog: native pick → validate-only inspect → review →
//!   confirm).
//!
//! `WappManagementPage.tsx` (the page that hosts the dialog and owns its
//! open state) is T073 and is not ported here; `permissionLabels.ts` (the
//! permission badges' tooltip texts) is T484.

pub(crate) mod install_wapp_dialog;

pub(crate) use install_wapp_dialog::*;
