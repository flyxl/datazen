//! Create-schema dialog, ported from
//! `packages/frontend/src/windows/connection/CreateSchemaDialog.tsx`.
//!
//! The React wrapper differs from the database dialog only in its command,
//! title/name keys, placeholder, field labels and the optional target
//! `database`. This module pins those values in [`SPEC`] and delegates the form
//! to the shared `create_database_dialog::build` state machine; no IPC is faked.
//!
//! ## Local stand-in (removal stage)
//!
//! `AdminSchemaFields` renders the driver-declared optional fields; with no
//! definition port it renders none (React's `definition: undefined` case).
//! The `createSchema.owner` label key is kept here for the eventual port.
//! Remove when `AdminSchemaFields` lands.
//!
//! ## Keys
//!
//! `content-view-create-schema-dialog` with `-body`, `-context`, `-cancel`,
//! `-confirm` (preserving T223's slot keys), plus `-name`, `-name-label`,
//! `-error`.

use rune::El;

use crate::views::create_database_dialog::{
    build, build_snapshot, AdminCreateAction, AdminCreateSpec, AdminCreateState, CreateDialogKind,
};

/// The exact wrapper values from React `CreateSchemaDialog`.
#[allow(dead_code)] // wrapper config; remove when the admin component lands.
pub(crate) const SPEC: AdminCreateSpec = AdminCreateSpec {
    key: "content-view-create-schema-dialog",
    title_key: "common.createSchema",
    name_label_key: "createSchema.name",
    placeholder: "my_schema",
    kind: CreateDialogKind::Schema,
};

/// Render the create-schema dialog. `open` is applied by the caller: React's
/// `Dialog` mounts nothing while closed.
#[allow(dead_code)] // consumer: content_view_dialogs slot (wired by T266).
pub(crate) fn build_schema<S: 'static>(
    state: &AdminCreateState,
    state_edit: impl Fn(&mut S) -> &mut AdminCreateState + 'static,
    on_action: Option<std::rc::Rc<dyn Fn(&mut S, AdminCreateAction) + 'static>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    build::<S>(state, SPEC, state_edit, on_action, dark, layer);
}

/// Snapshot variant for the T223 dialog slot (no host form state there).
#[allow(dead_code)] // consumer: content_view_dialogs slot; remove when the host wires real form state.
pub(crate) fn build_schema_snapshot<S: 'static>(
    state: &AdminCreateState,
    context: Option<&str>,
    on_action: Option<std::rc::Rc<dyn Fn(&mut S, AdminCreateAction) + 'static>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    build_snapshot::<S>(state, SPEC, context, on_action, dark, layer);
}
