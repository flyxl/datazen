//! Rune ports of `packages/frontend/src/components/admin/**`.
//!
//! | React source | Rune module |
//! |---|---|
//! | `AdminCreateDialog.tsx` (T013) | `admin_create_dialog` (state) + `admin_create_dialog_view` (view) |
//! | `AdminSchemaFields.tsx` (T064) | `admin_schema_fields` |
//! | `PrivilegeSelector.tsx` (T064) | `privilege_selector` |
//!
//! The production consumers are the `CreateDatabaseDialog` /
//! `CreateSchemaDialog` wiring task (not yet done): it owns an
//! `AdminCreateDialogState`, feeds the command definition in, and drains
//! the execute-request / created-name outboxes. The grant dialogs
//! (`CreateUserDialog` / `PrivilegeView` wiring) consume
//! `PrivilegeSelector` the same way.

#[allow(unused_imports)]
// the CreateDatabaseDialog / CreateSchemaDialog wiring is the production consumer
pub use admin_create_dialog::{
    AdminCreateDialogConfig, AdminCreateDialogState, AdminCreateExecuteRequest, AdminSchemaField,
};
#[allow(unused_imports)]
// the CreateDatabaseDialog / CreateSchemaDialog wiring is the production consumer
pub use admin_create_dialog_view::AdminCreateDialog;
#[allow(unused_imports)]
// the CreateDatabaseDialog / CreateSchemaDialog wiring is the production consumer
pub use admin_schema_fields::{
    AdminSchemaFields, SchemaFieldEditFn, admin_schema_field, admin_schema_has_field,
    schema_field_label, schema_properties, visible_schema_fields,
};
#[allow(unused_imports)]
// the CreateUserDialog / PrivilegeView wiring is the production consumer
pub use privilege_selector::{
    IsPrivilegeSelectedFn, PrivilegeGroup, PrivilegeSelector, TogglePrivilegeFn, all_privileges,
    privilege_groups, visible_privilege_groups,
};

pub mod admin_create_dialog;
pub mod admin_create_dialog_view;
pub mod admin_schema_fields;
pub mod privilege_selector;
