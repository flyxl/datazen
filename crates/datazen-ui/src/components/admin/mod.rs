//! Rune ports of `packages/frontend/src/components/admin/**`.
//!
//! | React source | Rune module |
//! |---|---|
//! | `AdminCreateDialog.tsx` (T013) | `admin_create_dialog` (state) + `admin_create_dialog_view` (view) |
//!
//! The production consumers are the `CreateDatabaseDialog` /
//! `CreateSchemaDialog` wiring task (not yet done): it owns an
//! `AdminCreateDialogState`, feeds the command definition in, and drains
//! the execute-request / created-name outboxes.

#[allow(unused_imports)]
// the CreateDatabaseDialog / CreateSchemaDialog wiring is the production consumer
pub(crate) use admin_create_dialog::{
    AdminCreateDialogConfig, AdminCreateDialogState, AdminCreateExecuteRequest, AdminSchemaField,
};
#[allow(unused_imports)]
// the CreateDatabaseDialog / CreateSchemaDialog wiring is the production consumer
pub(crate) use admin_create_dialog_view::AdminCreateDialog;

pub(crate) mod admin_create_dialog;
pub(crate) mod admin_create_dialog_view;
