//! Rune ports of `packages/frontend/src/components/export/**`.
//!
//! | React source | Rune module |
//! |---|
//! | `ExportSuccessPanel.tsx` (T014) | `export_success_panel` |
//!
//! The production consumers are the DataTable / connection export-dialog
//! wiring tasks (not yet done): they own an `ExportSuccessPanel` and pass
//! `t("export.success")` as its message.

#[allow(unused_imports)] // the export-dialog wiring is the production consumer
pub(crate) use export_success_panel::ExportSuccessPanel;

pub(crate) mod export_success_panel;
