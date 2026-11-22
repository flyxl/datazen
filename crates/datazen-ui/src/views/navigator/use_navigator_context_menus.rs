//! `windows/connection/navigator/useNavigatorContextMenus.ts` — the navigator
//! context menus, ported as a pure decision module for T578.
//!
//! The React hook is a wiring shell: it computes capability flags from the
//! active connection / driver registry / view actions, then calls the two menu
//! builders (`buildMainConnectionContextMenuItems` + `buildMainGroupContextMenuItems`
//! in `mainWindowContextMenu.ts`, `buildSchemaTreeContextMenuItems` in
//! `schemaTreeContextMenu.ts`) and shows the resulting `NativeMenuItemDef[]`.
//! Every item *set*, its order, its separators, its exact i18n label and the
//! action it fires are the decision logic; the popup, the driver command
//! lookups, the clipboard, the timers and the store writes are host concerns.
//!
//! This module keeps the decision half only:
//!
//! - [`MenuEntry`] is a host-agnostic mirror of `NativeMenuItemDef` (item /
//!   separator / submenu + stable `id` + i18n `label_key` + resolved `label`).
//! - [`NavAction`] is the intent an item emits; the navigator host performs it.
//!   No fake IPC, clipboard, timer, promise or store call lives here.
//! - [`DriverCaps`], [`ViewActionCaps`], [`ProductFeatures`] are the capability
//!   inputs the React hook derives before calling the builders. The host fills
//!   them from its stores; a missing capability means the item is *omitted*,
//!   exactly as an absent React handler makes `item()` return `null`.
//!
//! One submodule per menu family (group, connection, database, schema, table,
//! category). "Favorite" is the pin/unpin entry in the connection organize
//! submenu — [`connection::favorite_entry`] is its single spelling.
//!
//! ## Consumer / removal stage
//!
//! No production navigator host mounts these menus yet. Every reusable item
//! carries a per-item `#[allow(dead_code)]` with this stage as its removal
//! note; the attributes are dropped together when the host lands.
//!
//! ## Keys / icons / colour
//!
//! Label keys are the exact keys the React hook passes to `t()`; none are
//! invented. Icons are a host render concern and come only from the registry in
//! `crate::icons` — this module emits ids, never inline icon bodies. No colour
//! literal (oklch or otherwise) is introduced here.

use crate::i18n::t;

pub(crate) mod category;
pub(crate) mod connection;
pub(crate) mod database;
pub(crate) mod group;
pub(crate) mod schema;
pub(crate) mod table;

// ── Item descriptor ────────────────────────────────────────────────────────

/// The three `NativeMenuItemDef` kinds the navigator builders emit. The
/// driver-sdk's `predefined` kind is for native OS edit items and is not used
/// by these menus.
#[allow(dead_code)] // descriptor mirror; consumed by the host render layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Item,
    Separator,
    Submenu,
}

/// One host-agnostic context-menu entry. Mirrors `NativeMenuItemDef`: an item
/// has an action, a separator has none, a submenu carries children. `enabled`
/// mirrors the optional `enabled?: boolean`; the builders emit only actionable
/// items (React's enablement-by-omission), so it is `true` for every emitted
/// item and is exposed for the host renderer, not to hide items here.
#[allow(dead_code)] // descriptor mirror; consumed by the host render layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuEntry {
    pub kind: EntryKind,
    pub id: String,
    /// The exact i18n key behind `label`; `None` for caller-formatted text
    /// (group labels / move targets, React's `formatGroupLabel`).
    pub label_key: Option<&'static str>,
    /// Resolved label text (`t(label_key)` or the caller's formatted label).
    pub label: String,
    pub enabled: bool,
    /// Destructive-action hint for the host renderer (drop / truncate).
    pub danger: bool,
    pub action: Option<NavAction>,
    pub children: Vec<MenuEntry>,
}

#[allow(dead_code)] // descriptor constructor; remove with the host.
fn item(id: &str, key: &'static str, action: NavAction) -> MenuEntry {
    MenuEntry {
        kind: EntryKind::Item,
        id: id.to_owned(),
        label_key: Some(key),
        label: t(key).to_owned(),
        enabled: true,
        danger: false,
        action: Some(action),
        children: Vec::new(),
    }
}

#[allow(dead_code)] // descriptor constructor; remove with the host.
fn danger_item(id: &str, key: &'static str, action: NavAction) -> MenuEntry {
    let mut entry = item(id, key, action);
    entry.danger = true;
    entry
}

#[allow(dead_code)] // descriptor constructor; remove with the host.
fn dynamic_item(id: String, label: String, action: NavAction) -> MenuEntry {
    MenuEntry {
        kind: EntryKind::Item,
        id,
        label_key: None,
        label,
        enabled: true,
        danger: false,
        action: Some(action),
        children: Vec::new(),
    }
}

#[allow(dead_code)] // descriptor constructor; remove with the host.
fn separator() -> MenuEntry {
    MenuEntry {
        kind: EntryKind::Separator,
        id: "separator".to_owned(),
        label_key: None,
        label: String::new(),
        enabled: true,
        danger: false,
        action: None,
        children: Vec::new(),
    }
}

#[allow(dead_code)] // descriptor constructor; remove with the host.
fn submenu(id: &str, key: &'static str, children: Vec<MenuEntry>) -> MenuEntry {
    MenuEntry {
        kind: EntryKind::Submenu,
        id: id.to_owned(),
        label_key: Some(key),
        label: t(key).to_owned(),
        enabled: true,
        danger: false,
        action: None,
        children,
    }
}

// ── Emitted actions ────────────────────────────────────────────────────────

/// React's five `GeneratedSqlType` values for the table generate-sql submenu.
#[allow(dead_code)] // payload mirror; consumed by the host SQL generator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SqlKind {
    #[default]
    Select,
    Insert,
    Update,
    Delete,
    Ddl,
}

/// The discriminants of every intent a navigator menu can fire. One variant
/// per stable menu id; payloads live in [`NavTarget`] so the enum stays small.
#[allow(dead_code)] // host port; remove when the navigator host wires it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavActionKind {
    NewGroup,
    NewConnection,
    RenameGroup,
    DeleteGroup,
    OpenOrDisconnect,
    CopyName,
    CopyConnectionUrl,
    NewQuery,
    OpenQueryHistory,
    PendingQueryHistory,
    CreateDatabase,
    CreateUser,
    OpenServerStatus,
    ToggleFavorite,
    ObjectFilter,
    OpenProcessList,
    Backup,
    Restore,
    RefreshConnection,
    EditConnection,
    DuplicateConnection,
    MoveToGroup,
    RemoveFromGroup,
    DeleteConnection,
    CloseDatabase,
    RefreshDatabase,
    CopyDatabaseName,
    OpenErDiagram,
    ExecuteSqlFile,
    CreateTable,
    CreateSchema,
    DropDatabase,
    DataTransfer,
    CompareSchema,
    CompareData,
    RefreshSchema,
    DropSchema,
    OpenTable,
    GenerateSql,
    OpenTableStructure,
    CopyDdl,
    FocusEr,
    TruncateTable,
    DropRelation,
    RefreshCategory,
}

/// All payloads a menu action may need. Empty fields mean "not relevant to
/// this action", mirroring the narrow closures the React hook creates.
#[allow(dead_code)] // payload mirror; consumed by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NavTarget {
    pub id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database: String,
    pub schema: String,
    pub name: String,
    pub group: String,
    pub section_group: String,
    pub database_type: String,
    pub sql_kind: SqlKind,
    pub is_view: bool,
    pub cat_id: String,
}

#[allow(dead_code)] // host port; remove when the navigator host wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NavAction {
    pub kind: NavActionKind,
    pub target: NavTarget,
}

#[allow(dead_code)] // action constructor; remove with the host.
fn act(kind: NavActionKind) -> NavAction {
    NavAction {
        kind,
        target: NavTarget::default(),
    }
}

#[allow(dead_code)] // action constructor; remove with the host.
fn at(kind: NavActionKind, target: NavTarget) -> NavAction {
    NavAction { kind, target }
}

// ── Capability inputs ──────────────────────────────────────────────────────

/// The React `DB_REGISTRY[type]` flags the menus read, plus the tree-shape
/// fields behind `shouldUseMultiDatabaseTree`. The host supplies them; this
/// module never imports a driver or hardcodes a dialect.
#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct DriverCaps {
    pub supports_backup: bool,
    pub supports_create_database: bool,
    pub supports_create_user: bool,
    pub supports_create_schema: bool,
    pub read_only: bool,
    /// React `dbMeta?.supportsErDiagram !== false` — callers set the default.
    pub supports_er_diagram: bool,
    pub has_multi_database: bool,
    pub database_field_type: String,
}

/// The optional `viewActions` methods the hook calls. A `false` means the
/// matching item is never created (React's optional-chaining guard).
#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Debug, Default)]
pub(crate) struct ViewActionCaps {
    pub new_query: bool,
    pub query_history: bool,
    pub open_sql_file: bool,
    pub create_table: bool,
    pub open_server_status: bool,
    pub open_process_list: bool,
    pub open_er_diagram: bool,
    pub open_table_structure: bool,
}

/// `isProductFeatureEnabled(...)` results the migration-tools items read.
#[allow(dead_code)] // VM mirror; supplied by the navigator host.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProductFeatures {
    pub data_transfer: bool,
    pub schema_diff: bool,
    pub data_sync: bool,
}

/// React `connectionGroups.ts` `formatGroupLabel(key, t)`: the three preset
/// storage keys get their localized `newConn.group*` labels; every other key —
/// including the empty / ungrouped one — is passed through verbatim. The
/// caller decides how to render the ungrouped bucket, not this helper.
#[allow(dead_code)] // deps mirror; remove when the groups lib is ported.
pub(crate) fn format_group_label(name: &str) -> String {
    let preset_key = match name {
        "preset:production" => Some("newConn.groupProd"),
        "preset:development" => Some("newConn.groupDev"),
        "preset:testing" => Some("newConn.groupTest"),
        _ => None,
    };
    match preset_key {
        Some(key) => t(key).to_owned(),
        None => name.to_owned(),
    }
}

// ── Test helpers ───────────────────────────────────────────────────────────

/// Flatten a menu into its ordered ids, writing `"separator"` for separators
/// (matching the React tests' `ids()` helper).
#[allow(dead_code)] // test helper; remove with the host.
pub(crate) fn entry_ids(entries: &[MenuEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry.kind {
            EntryKind::Separator => "separator".to_owned(),
            _ => entry.id.clone(),
        })
        .collect()
}

/// Depth-first lookup by stable id, including submenu children.
#[allow(dead_code)] // test helper; remove with the host.
pub(crate) fn find_entry<'a>(entries: &'a [MenuEntry], id: &str) -> Option<&'a MenuEntry> {
    for entry in entries {
        if entry.id == id {
            return Some(entry);
        }
        if let Some(found) = find_entry(&entry.children, id) {
            return Some(found);
        }
    }
    None
}
