//! Panel and cell editability — the `resolvePanel*` half of the port.
//!
//! The React source deliberately keeps two different questions apart:
//! `resolvePanelReadOnlyReason` explains why a whole grid is read-only, while
//! `resolveCellEditable` (called by `resolvePanelCellEditable` with the row
//! facts the store owns) explains why one cell is. Both orders are copied
//! verbatim: the first matching check is the sentence the user is told.

use std::collections::BTreeMap;

use crate::support::cell_types::{
    is_integer_cell_type, is_read_only_cell_type, resolve_cell_type, CellType, CellTypeHints,
};

/// React `CELL_EDIT_MAX_BYTES` from `lib/cellEditorKind.ts`.
#[allow(dead_code)] // pure constant; drop with the cell editability host.
pub const CELL_EDIT_MAX_BYTES: usize = 64 * 1024;

/// React `CellReadOnlyReason`.
#[allow(dead_code)] // VM mirror; drop when the host supplies editability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditingReadOnlyReason {
    ConnectionReadOnly,
    DriverCapability,
    View,
    NoPrimaryKey,
    AmbiguousRowIdentity,
    AutoIncrement,
    Generated,
    BinaryColumn,
    LargeValue,
    TypeNotEditable,
}

impl EditingReadOnlyReason {
    /// React `readOnlyReasonMessageKey` — the one reason→key mapping.
    #[allow(dead_code)] // pure helper; drop with the read-only tip.
    pub fn message_key(self) -> &'static str {
        match self {
            Self::ConnectionReadOnly => "cellEditor.readOnly.connection",
            Self::DriverCapability => "cellEditor.readOnly.driverCapability",
            Self::View => "cellEditor.readOnly.view",
            Self::NoPrimaryKey => "cellEditor.readOnly.noPrimaryKey",
            Self::AmbiguousRowIdentity => "cellEditor.readOnly.ambiguousRowIdentity",
            Self::AutoIncrement => "cellEditor.readOnly.autoIncrement",
            Self::Generated => "cellEditor.readOnly.generated",
            Self::BinaryColumn => "cellEditor.readOnly.binaryColumn",
            Self::LargeValue => "cellEditor.readOnly.largeValue",
            Self::TypeNotEditable => "cellEditor.readOnly.typeNotEditable",
        }
    }
}

/// React `CellValueErrorReason` — why an open editor refused its draft.
#[allow(dead_code)] // VM mirror; drop with the navigation guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellValueErrorReason {
    EmptyNotNull,
    InvalidNumber,
    InvalidInteger,
    InvalidBoolean,
    InvalidDate,
    InvalidTime,
    InvalidDateTime,
    InvalidJson,
    InvalidUuid,
    TooLarge,
    NotEditable,
}

impl CellValueErrorReason {
    /// React `cellValueErrorMessageKey`.
    #[allow(dead_code)] // pure helper; drop with the navigation guard.
    pub fn message_key(self) -> &'static str {
        match self {
            Self::EmptyNotNull => "cellEditor.notNull",
            Self::InvalidNumber => "cellEditor.invalidNumber",
            Self::InvalidInteger => "cellEditor.invalidInteger",
            Self::InvalidBoolean => "cellEditor.invalidBoolean",
            Self::InvalidDate => "cellEditor.invalidDate",
            Self::InvalidTime => "cellEditor.invalidTime",
            Self::InvalidDateTime => "cellEditor.invalidDateTime",
            Self::InvalidJson => "cellEditor.invalidJson",
            Self::InvalidUuid => "cellEditor.invalidUuid",
            Self::TooLarge => "cellEditor.valueTooLarge",
            Self::NotEditable => "cellEditor.binaryNotEditable",
        }
    }
}

/// React `TableType`: every non-`Table` relation is refused as a view.
#[allow(dead_code)] // VM mirror; drop when the host supplies the relation kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationKind {
    Table,
    View,
}

/// React `rowKind`: a draft row is not yet in the database.
#[allow(dead_code)] // VM mirror; drop with the cell editability host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditingRowKind {
    Update,
    Insert,
}

/// The columns facts `buildColumnEditorInfo` needs, supplied by the host.
#[allow(dead_code)] // VM mirror; drop when the host owns column metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditingColumnVm {
    pub name: String,
    pub data_type: Option<String>,
    pub nullable: bool,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

/// The driver facts `useTableEditing` reads from `DB_REGISTRY` (host-supplied).
#[allow(dead_code)] // VM mirror; drop when the host supplies driver metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DriverEditingMetaVm {
    /// `DatabaseTypeMeta.dataGrid?.editable`; absent means editable.
    pub editable: Option<bool>,
    /// `dataGrid?.currentDatetimeWrite` (DBR-05).
    pub current_datetime_write: Option<bool>,
    pub hints: Option<CellTypeHints>,
}

/// The panel facts `useTableEditing` folds into its `TableEditingContext`.
#[allow(dead_code)] // VM mirror; drop when the host owns the panel context.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditingPanelFactsVm {
    pub panel_id: String,
    pub connection_read_only: bool,
    pub mount_reason: Option<EditingReadOnlyReason>,
    pub columns: Vec<EditingColumnVm>,
}

/// React `TableEditingContext` after the hook's `useMemo` has run.
#[allow(dead_code)] // VM mirror; drop when the host owns the panel context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditingContextVm {
    pub panel_id: String,
    pub connection_read_only: bool,
    pub mount_reason: Option<EditingReadOnlyReason>,
    pub relation_kind: RelationKind,
    pub capability_editable: Option<bool>,
    pub current_datetime_write: Option<bool>,
    pub has_primary_key: bool,
    pub has_columns: bool,
    pub columns: Vec<EditingColumnVm>,
    pub hints: Option<CellTypeHints>,
}

/// The hook's `useMemo`: primary-key/column presence come from the columns,
/// driver capability and type hints come from the host's driver metadata.
#[allow(dead_code)] // pure derivation; drop when the host owns the context.
pub fn derive_editing_context(
    facts: &EditingPanelFactsVm,
    meta: Option<&DriverEditingMetaVm>,
) -> EditingContextVm {
    EditingContextVm {
        panel_id: facts.panel_id.clone(),
        connection_read_only: facts.connection_read_only,
        mount_reason: facts.mount_reason,
        // The panel hook mounts its grid over a table, always.
        relation_kind: RelationKind::Table,
        capability_editable: meta.and_then(|meta| meta.editable),
        current_datetime_write: meta.and_then(|meta| meta.current_datetime_write),
        has_primary_key: facts.columns.iter().any(|column| column.is_primary_key),
        has_columns: !facts.columns.is_empty(),
        columns: facts.columns.clone(),
        hints: meta.and_then(|meta| meta.hints.clone()),
    }
}

/// React `ColumnEditorInfo`.
#[allow(dead_code)] // VM mirror; drop when the grid owns column info.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnEditorInfoVm {
    pub cell_type: CellType,
    pub nullable: bool,
    pub integer_only: bool,
    pub read_only_reason: Option<EditingReadOnlyReason>,
}

/// React `buildPanelColumnEditorInfo`: once per column, never per cell.
#[allow(dead_code)] // pure helper; drop when the grid owns column info.
pub fn build_panel_column_editor_info(
    context: &EditingContextVm,
) -> BTreeMap<String, ColumnEditorInfoVm> {
    let mut info = BTreeMap::new();
    for column in &context.columns {
        let data_type = column.data_type.as_deref();
        let cell_type = resolve_cell_type(data_type, context.hints.as_ref());
        let reason = if is_read_only_cell_type(data_type, context.hints.as_ref()) {
            Some(EditingReadOnlyReason::TypeNotEditable)
        } else if cell_type == CellType::Bytes {
            Some(EditingReadOnlyReason::BinaryColumn)
        } else {
            None
        };
        info.insert(
            column.name.clone(),
            ColumnEditorInfoVm {
                cell_type,
                nullable: column.nullable,
                integer_only: is_integer_cell_type(data_type, context.hints.as_ref()),
                read_only_reason: reason,
            },
        );
    }
    info
}

/// React `resolvePanelReadOnlyReason`. The empty-panel exception is deliberate:
/// no columns means "not loaded yet", which is not a refusal.
#[allow(dead_code)] // pure gate; drop when the host owns the panel context.
pub fn resolve_panel_read_only_reason(
    context: &EditingContextVm,
) -> Option<EditingReadOnlyReason> {
    if let Some(reason) = context.mount_reason {
        return Some(reason);
    }
    if context.connection_read_only {
        return Some(EditingReadOnlyReason::ConnectionReadOnly);
    }
    if context.capability_editable == Some(false) {
        return Some(EditingReadOnlyReason::DriverCapability);
    }
    if context.relation_kind != RelationKind::Table {
        return Some(EditingReadOnlyReason::View);
    }
    if context.has_columns && !context.has_primary_key {
        return Some(EditingReadOnlyReason::NoPrimaryKey);
    }
    None
}

/// React `resolvePanelCellReadOnlyReason`: the panel reason wins; only then the
/// column's own reason (a driver-declared unwritable type, or binary).
#[allow(dead_code)] // pure gate; drop when the host owns the context.
pub fn resolve_panel_cell_read_only_reason(
    context: &EditingContextVm,
    column_name: &str,
) -> Option<EditingReadOnlyReason> {
    if let Some(reason) = resolve_panel_read_only_reason(context) {
        return Some(reason);
    }
    build_panel_column_editor_info(context)
        .get(column_name)
        .and_then(|info| info.read_only_reason)
}

/// React `CellEditability`.
#[allow(dead_code)] // VM mirror; drop with the editor host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEditabilityVm {
    pub editable: bool,
    pub reason: Option<EditingReadOnlyReason>,
    pub code: Option<&'static str>,
    pub message_key: &'static str,
}

fn editable_vm() -> CellEditabilityVm {
    CellEditabilityVm {
        editable: true,
        reason: None,
        code: None,
        message_key: "",
    }
}

fn refused_vm(reason: EditingReadOnlyReason) -> CellEditabilityVm {
    CellEditabilityVm {
        editable: false,
        reason: Some(reason),
        code: Some("grid.cell.readOnly"),
        message_key: reason.message_key(),
    }
}

/// The row facts `resolvePanelCellEditable` passes to `resolveCellEditable`.
#[allow(dead_code)] // VM mirror; drop when the store owns row identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellEditabilityInputVm {
    pub mount_reason: Option<EditingReadOnlyReason>,
    pub connection_read_only: bool,
    pub capability_editable: Option<bool>,
    pub relation_kind: RelationKind,
    pub has_primary_key: bool,
    pub row_identity_unique: bool,
    pub column_is_auto_increment: bool,
    pub cell_type: CellType,
    pub in_memory_byte_size: usize,
    pub type_read_only: bool,
    pub row_kind: EditingRowKind,
}

/// React `readOnlyReasonOf`: the first match decides what the user is told.
#[allow(dead_code)] // pure gate; drop with the cell editability host.
pub fn read_only_reason_of(input: &CellEditabilityInputVm) -> Option<EditingReadOnlyReason> {
    use EditingReadOnlyReason::*;
    if let Some(reason) = input.mount_reason {
        return Some(reason);
    }
    if input.connection_read_only {
        return Some(ConnectionReadOnly);
    }
    if input.capability_editable == Some(false) {
        return Some(DriverCapability);
    }
    if input.relation_kind != RelationKind::Table {
        return Some(View);
    }
    if !input.has_primary_key {
        return Some(NoPrimaryKey);
    }
    if !input.row_identity_unique {
        return Some(AmbiguousRowIdentity);
    }
    if input.column_is_auto_increment && input.row_kind == EditingRowKind::Update {
        return Some(AutoIncrement);
    }
    if input.type_read_only {
        return Some(TypeNotEditable);
    }
    if input.cell_type == CellType::Bytes {
        return Some(BinaryColumn);
    }
    if input.in_memory_byte_size > CELL_EDIT_MAX_BYTES {
        return Some(LargeValue);
    }
    None
}

/// React `resolveCellEditable`.
#[allow(dead_code)] // pure gate; drop with the cell editability host.
pub fn resolve_cell_editable(input: &CellEditabilityInputVm) -> CellEditabilityVm {
    match read_only_reason_of(input) {
        None => editable_vm(),
        Some(reason) => refused_vm(reason),
    }
}

/// React `resolvePanelCellEditable`: the registry's missing-context branch is
/// pinned by passing `None` (the hook answers "connection read-only"), while a
/// registered panel fills in `rowIdentityUnique: true`, size `0`, `update`.
#[allow(dead_code)] // pure gate; drop when the host owns the panel registry.
pub fn resolve_panel_cell_editability(
    context: Option<&EditingContextVm>,
    column_name: &str,
) -> CellEditabilityVm {
    let Some(context) = context else {
        return refused_vm(EditingReadOnlyReason::ConnectionReadOnly);
    };
    let column = context
        .columns
        .iter()
        .find(|entry| entry.name == column_name);
    let info = build_panel_column_editor_info(context).get(column_name).copied();
    let data_type = column.and_then(|column| column.data_type.as_deref());
    let input = CellEditabilityInputVm {
        mount_reason: context.mount_reason,
        connection_read_only: context.connection_read_only,
        capability_editable: context.capability_editable,
        relation_kind: context.relation_kind,
        has_primary_key: context.has_primary_key,
        row_identity_unique: true,
        column_is_auto_increment: column.is_some_and(|column| column.is_auto_increment),
        cell_type: info.map(|info| info.cell_type).unwrap_or(CellType::Unknown),
        in_memory_byte_size: 0,
        type_read_only: is_read_only_cell_type(data_type, context.hints.as_ref()),
        row_kind: EditingRowKind::Update,
    };
    resolve_cell_editable(&input)
}

/// React `CellCoord` for the one editor-open entry point.
#[allow(dead_code)] // VM mirror; drop when the host owns grid selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditingCoordVm {
    pub row_index: usize,
    pub column_name: String,
}

/// The one effect `requestEdit` emits on success: `startEdit(panel, row, col)`.
#[allow(dead_code)] // host intent; drop when the host owns the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartEditEffectVm {
    pub row: usize,
    pub column: String,
}

/// React `requestEdit`: ask first, open the editor only when allowed. The
/// refusal is returned (never thrown, never silent), so the caller can show it.
#[allow(dead_code)] // pure gate; drop when the host owns the editor entry point.
pub fn request_edit_plan(
    context: Option<&EditingContextVm>,
    coord: &EditingCoordVm,
) -> (CellEditabilityVm, Option<StartEditEffectVm>) {
    let editability = resolve_panel_cell_editability(context, &coord.column_name);
    if !editability.editable {
        return (editability, None);
    }
    (
        editability,
        Some(StartEditEffectVm {
            row: coord.row_index,
            column: coord.column_name.clone(),
        }),
    )
}
