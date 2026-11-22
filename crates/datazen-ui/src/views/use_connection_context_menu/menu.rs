//! Pure port of `lib/schemaTreeContextMenu.ts::buildSchemaTreeContextMenuItems`.
//!
//! The host supplies which handlers are wired (a set of action ids) and the
//! caller-supplied labels; this function decides the exact item set, order and
//! separators for each schema-tree node kind. No IPC, menu host or React lives
//! here — just an ordered list of [`MenuEntry`]s.

use std::collections::BTreeSet;

/// React `SchemaTreeNodeKind`.
#[allow(dead_code)] // VM mirror; supplied by the tree host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MenuKind {
    Table,
    View,
    Database,
    Schema,
    Category,
    Blank,
    /// `function` / `procedure` / `trigger` / `sequence` / `type` — the default branch.
    #[default]
    Other,
}

/// One rendered menu entry (React `NativeMenuItemDef`, reduced to observable data).
#[allow(dead_code)] // VM mirror; consumed by the host menu renderer and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MenuEntry {
    Item { id: &'static str, label: String },
    Separator,
    Submenu { id: &'static str, label: String, items: Vec<MenuEntry> },
}

/// Product feature flags gating the migration-tool items.
#[allow(dead_code)] // VM mirror; supplied by the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FeatureFlags {
    pub data_transfer: bool,
    pub schema_diff: bool,
    pub data_sync: bool,
}

/// React `BuildSchemaTreeContextMenuArgs`, reduced to pure inputs.
#[allow(dead_code)] // VM surface; supplied by the hook.
#[derive(Clone, Debug, Default)]
pub(crate) struct MenuArgs {
    pub kind: MenuKind,
    pub labels: std::collections::BTreeMap<&'static str, String>,
    /// Handler ids that are actually wired; an unwired handler omits its item.
    pub wired: BTreeSet<&'static str>,
    pub read_only: bool,
    pub safe_mode: bool,
    pub show_open_structure: bool,
    pub show_er_focus: bool,
    /// `None` means React's `undefined` (default handling).
    pub show_export: Option<bool>,
    pub show_batch_export: Option<bool>,
    pub show_new_table: bool,
    pub category_id: Option<String>,
    pub features: FeatureFlags,
    /// Extra labels for the generate-sql submenu (optional in React).
    pub generate_labels: std::collections::BTreeMap<&'static str, String>,
}

impl MenuArgs {
    fn label(&self, id: &'static str) -> String {
        self.labels.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }

    fn item(&self, id: &'static str) -> Option<MenuEntry> {
        self.wired.contains(id).then(|| MenuEntry::Item { id, label: self.label(id) })
    }

    /// React `batchExportShown(kind)`: default follows `showExport` (true only
    /// for views when `showExport === true`), else `showExport !== false`.
    fn batch_export_shown(&self) -> bool {
        // React forces `showBatchExport = false` for tables, whatever the
        // driver/caller says.
        if self.kind == MenuKind::Table {
            return false;
        }
        match self.show_batch_export {
            Some(value) => value,
            None if self.kind == MenuKind::View => self.show_export == Some(true),
            None => self.show_export != Some(false),
        }
    }
}

fn push(defs: &[Option<MenuEntry>]) -> Vec<MenuEntry> {
    defs.iter().flatten().cloned().collect()
}

/// React `generateSqlSubmenu`: present only when at least one generate handler
/// is wired; the submenu's own items follow SELECT/INSERT/UPDATE/DELETE/DDL.
fn generate_sql_submenu(args: &MenuArgs) -> Option<MenuEntry> {
    let ids = [
        ("generate-select", "SELECT"),
        ("generate-insert", "INSERT"),
        ("generate-update", "UPDATE"),
        ("generate-delete", "DELETE"),
        ("generate-ddl", "DDL"),
    ];
    let items = push(
        &ids.iter()
            .map(|(id, fallback)| {
                args.wired.contains(id).then(|| MenuEntry::Item {
                    id,
                    label: args.generate_labels.get(id).cloned().unwrap_or_else(|| fallback.to_string()),
                })
            })
            .collect::<Vec<_>>(),
    );
    if items.is_empty() {
        return None;
    }
    Some(MenuEntry::Submenu {
        id: "generate-sql",
        label: args
            .generate_labels
            .get("generate-sql")
            .cloned()
            .unwrap_or_else(|| "Generate SQL".to_owned()),
        items,
    })
}

/// React `migrationToolItems`: each item additionally gated by a product flag.
fn migration_items(args: &MenuArgs) -> Vec<MenuEntry> {
    let mut items = Vec::new();
    if args.features.data_transfer {
        if let Some(item) = args.item("data-transfer") {
            items.push(item);
        }
    }
    if args.features.schema_diff {
        if let Some(item) = args.item("compare-schema") {
            items.push(item);
        }
    }
    if args.features.data_sync {
        if let Some(item) = args.item("compare-data") {
            items.push(item);
        }
    }
    items
}

/// React `buildSchemaTreeContextMenuItems` — the full decision.
#[allow(dead_code)] // pure rule; remove with the host menu wiring.
pub(crate) fn build_menu(args: &MenuArgs) -> Vec<MenuEntry> {
    match args.kind {
        MenuKind::Table => {
            let danger = if args.read_only {
                Vec::new()
            } else {
                push(&[args.item("truncate"), args.item("drop")])
            };
            let mut main = push(&[
                args.item("open"),
                if args.show_open_structure { args.item("open-structure") } else { None },
                if args.show_er_focus { args.item("focus-er") } else { None },
                generate_sql_submenu(args),
                args.item("new-query"),
                args.item("copy-name"),
                args.item("copy-ddl"),
                if args.show_export != Some(false) { args.item("export") } else { None },
                if args.batch_export_shown() { args.item("batch-export") } else { None },
                if !args.read_only { args.item("import") } else { None },
            ]);
            if danger.is_empty() {
                return main;
            }
            main.push(MenuEntry::Separator);
            main.extend(danger);
            main
        }
        MenuKind::View => {
            let mut main = push(&[
                args.item("open"),
                args.item("copy-name"),
                args.item("copy-ddl"),
                if args.show_export == Some(true) { args.item("export") } else { None },
                if args.batch_export_shown() { args.item("batch-export") } else { None },
            ]);
            if args.read_only {
                return main;
            }
            if let Some(drop) = args.item("drop-view") {
                main.push(MenuEntry::Separator);
                main.push(drop);
            }
            main
        }
        MenuKind::Database => {
            let mut main = push(&[
                args.item("refresh"),
                args.item("new-query"),
                args.item("query-history"),
                if !args.read_only && !args.safe_mode { args.item("execute-sql-file") } else { None },
                args.item("copy-database-name"),
                args.item("view-er-diagram"),
                if args.batch_export_shown() { args.item("batch-export") } else { None },
                if !args.read_only { args.item("import") } else { None },
                if !args.read_only && args.show_new_table { args.item("new-table") } else { None },
                args.item("create-schema"),
            ]);
            let sync = migration_items(args);
            let backup = push(&[args.item("backup"), args.item("restore")]);
            let close_db = args.item("close-database-connection");
            let drop_db = if args.read_only { None } else { args.item("drop-database") };
            if !sync.is_empty() {
                main.push(MenuEntry::Separator);
                main.extend(sync);
            }
            if !backup.is_empty() {
                main.push(MenuEntry::Separator);
                main.extend(backup);
            }
            if let Some(close) = close_db {
                main.push(MenuEntry::Separator);
                main.push(close);
            }
            if let Some(drop) = drop_db {
                main.push(MenuEntry::Separator);
                main.push(drop);
            }
            main
        }
        MenuKind::Schema => {
            let mut main = push(&[
                args.item("refresh"),
                args.item("new-query"),
                args.item("query-history"),
                if !args.read_only && !args.safe_mode { args.item("execute-sql-file") } else { None },
                args.item("copy-schema-name"),
                args.item("view-er-diagram"),
                if args.batch_export_shown() { args.item("batch-export") } else { None },
                if !args.read_only { args.item("import") } else { None },
                if !args.read_only { args.item("new-table") } else { None },
            ]);
            let sync = migration_items(args);
            if !sync.is_empty() {
                main.push(MenuEntry::Separator);
                main.extend(sync);
            }
            if !args.read_only {
                if let Some(drop) = args.item("drop-schema") {
                    main.push(MenuEntry::Separator);
                    main.push(drop);
                }
            }
            main
        }
        MenuKind::Category => push(&[
            args.item("refresh"),
            if !args.read_only && args.category_id.as_deref() == Some("tables") {
                args.item("new-table")
            } else {
                None
            },
            if !args.read_only { args.item("import") } else { None },
        ]),
        MenuKind::Blank => push(&[
            args.item("refresh"),
            args.item("new-query"),
            if args.batch_export_shown() { args.item("batch-export") } else { None },
            if !args.read_only { args.item("import") } else { None },
            if !args.read_only && args.show_new_table { args.item("new-table") } else { None },
        ]),
        MenuKind::Other => push(&[args.item("copy-name")]),
    }
}

/// Project the rendered menu to its observable id sequence (a `separator` for
/// each [`MenuEntry::Separator`]) — the shape the React tests assert.
#[allow(dead_code)] // pure helper; consumed by tests.
pub(crate) fn entry_ids(entries: &[MenuEntry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item { id, .. } | MenuEntry::Submenu { id, .. } => *id,
            MenuEntry::Separator => "separator",
        })
        .collect()
}
