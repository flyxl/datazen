//! Port of `packages/frontend/src/lib/sqlEditorContextMenu.ts` and the
//! driver-sdk menu contract it consumes (`types/menu.ts`).
//!
//! ## What maps where
//!
//! - `toggleSqlLineComments` moves here from `lib_helpers` — the landing
//!   spot `SqlEditor.tsx`'s port promised when it carried the function.
//! - `NativeMenuItemDef` / `NativeMenuPredefined` / `nativeEditMenuItems`
//!   are driver-sdk's web context-menu contract (the host's
//! `lib/nativeContextMenu.ts` re-export shim and the show/hide bridge are
//! segment 2's T567; this module owns only the item model the SQL editor
//! builds).
//! - `Platform` moved home to `hooks/usePlatform.ts`'s port (T384;
//!   re-exported below at the pre-T384 path) and `formatModKey` stays
//!   carried (lib/keymap.ts, segment 2, T541). The React builder reads the platform
//!   through `getPlatformSync()`; the engine takes it as an input (the
//!   host knows its OS — there is no user agent to sniff).
//! - The optional-`enabled` union field stays `Option<bool>` (absent =
//!   enabled); the builder always decides it explicitly.
//!
//! No production consumer yet; the workspace view wiring will call
//! `build_sql_editor_context_menu_items` the way SqlEditor.tsx's
//! contextmenu handler does.

use std::rc::Rc;

/// React `Platform` — its definition home since T384
/// (`hooks/usePlatform.ts`); re-exported at the pre-T384 carry path.
pub use crate::hooks::use_platform::Platform;

/// `formatModKey` from `lib/keymap.ts` (segment 2, T541) — carried 1:1:
/// ⌘ on macOS, Ctrl everywhere else.
pub fn format_mod_key(platform: Platform) -> &'static str {
    if platform == Platform::Macos {
        "⌘"
    } else {
        "Ctrl"
    }
}

/// driver-sdk `NativeMenuPredefined` — the edit/separator kinds shared by
/// web context menus (legacy name kept for item defs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMenuPredefined {
    Separator,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

/// driver-sdk `NativeMenuItemDef` — the item model. `action` is
/// `() => void | Promise<void>` in React; a synchronous `Rc<dyn Fn()>` here
/// (no driver action in this port is async).
pub enum NativeMenuItemDef {
    Item {
        id: String,
        label: String,
        shortcut: Option<String>,
        /// React's optional `enabled` (absent = enabled).
        enabled: Option<bool>,
        action: Rc<dyn Fn()>,
    },
    Separator,
    Predefined {
        item: NativeMenuPredefined,
        text: Option<String>,
    },
    Submenu {
        id: Option<String>,
        label: String,
        items: Vec<NativeMenuItemDef>,
    },
}

impl NativeMenuItemDef {
    /// The `kind: 'item' && id === ...` predicate React's tests and
    /// callers use.
    pub fn is_item_with_id(&self, id: &str) -> bool {
        matches!(self, Self::Item { id: i, .. } if i == id)
    }

    /// A kind/name token for order assertions: `'Cut'`-style predefined
    /// names for edit items, the id for real items, the kind otherwise —
    /// exactly the `kinds()` helper in sqlEditorContextMenu.test.ts.
    pub fn kind_token(&self) -> &str {
        match self {
            Self::Item { id, .. } => id,
            Self::Separator => "separator",
            Self::Predefined { item, .. } => match item {
                NativeMenuPredefined::Separator => "Separator",
                NativeMenuPredefined::Cut => "Cut",
                NativeMenuPredefined::Copy => "Copy",
                NativeMenuPredefined::Paste => "Paste",
                NativeMenuPredefined::SelectAll => "SelectAll",
                NativeMenuPredefined::Undo => "Undo",
                NativeMenuPredefined::Redo => "Redo",
            },
            Self::Submenu { .. } => "submenu",
        }
    }
}

/// driver-sdk `nativeEditMenuItems()` — the standard edit block for
/// text-focused surfaces, in declaration order.
pub fn native_edit_menu_items() -> Vec<NativeMenuItemDef> {
    vec![
        NativeMenuItemDef::Predefined { item: NativeMenuPredefined::Cut, text: None },
        NativeMenuItemDef::Predefined { item: NativeMenuPredefined::Copy, text: None },
        NativeMenuItemDef::Predefined { item: NativeMenuPredefined::Paste, text: None },
        NativeMenuItemDef::Predefined { item: NativeMenuPredefined::SelectAll, text: None },
    ]
}

/// React `SqlEditorContextMenuLabels`.
pub struct SqlEditorContextMenuLabels {
    pub run: String,
    pub run_selection: String,
    pub format: String,
    pub comment: String,
    pub add_favorite: Option<String>,
}

/// React `SqlEditorContextMenuHandlers` — every handler optional; the
/// builder omits the item when the handler (or, for the favorite, the
/// label too) is absent.
#[derive(Default)]
pub struct SqlEditorContextMenuHandlers {
    pub on_run: Option<Rc<dyn Fn()>>,
    pub on_run_selection: Option<Rc<dyn Fn()>>,
    pub on_format: Option<Rc<dyn Fn()>>,
    pub on_comment: Option<Rc<dyn Fn()>>,
    pub on_add_favorite: Option<Rc<dyn Fn(&str)>>,
}

/// React `BuildSqlEditorContextMenuArgs` — plus the `platform` input the
/// React version reads through `getPlatformSync()` (T386 seam; React's
/// `hasSelection` defaults to false and is decided by the caller here).
pub struct BuildSqlEditorContextMenuArgs<'a> {
    pub labels: &'a SqlEditorContextMenuLabels,
    pub handlers: &'a SqlEditorContextMenuHandlers,
    /// Full editor SQL (or trimmed selection fallback from the editor).
    pub sql_text: &'a str,
    /// True when the editor has a non-empty selection.
    pub has_selection: bool,
    pub platform: Platform,
}

/// `toggleSqlLineComments` — toggle `-- ` line comments on each non-empty
/// line. If every non-empty line is already commented, strips the comment
/// prefix instead.
pub fn toggle_sql_line_comments(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let non_empty: Vec<&str> = lines.iter().copied().filter(|l| !l.trim().is_empty()).collect();
    let all_commented =
        !non_empty.is_empty() && non_empty.iter().all(|l| l.trim_start().starts_with("--"));
    if all_commented {
        return lines
            .iter()
            .map(|l| {
                // `l.replace(/^(\s*)-- ?/, '$1')`: keep the indent, drop
                // the marker and at most one following space.
                let indent_end = l.len() - l.trim_start().len();
                let rest = l[indent_end..].strip_prefix("--").unwrap_or(&l[indent_end..]);
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                format!("{}{}", &l[..indent_end], rest)
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::from(*l)
            } else {
                format!("-- {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `buildSqlEditorContextMenuItems` — build the native context-menu items
/// for the SQL editor. OS edit actions first; then Run / Run Selection /
/// Format / Comment; then Add Favorite (which needs both the handler and
/// the label, and receives the trimmed SQL only when it is non-empty).
pub fn build_sql_editor_context_menu_items(
    args: &BuildSqlEditorContextMenuArgs<'_>,
) -> Vec<NativeMenuItemDef> {
    let mod_key = format_mod_key(args.platform);
    let trimmed = args.sql_text.trim();
    let mut items = native_edit_menu_items();
    items.push(NativeMenuItemDef::Separator);

    if let Some(on_run) = args.handlers.on_run.clone() {
        items.push(NativeMenuItemDef::Item {
            id: "run".into(),
            label: args.labels.run.clone(),
            shortcut: Some(format!("{mod_key}+Enter")),
            enabled: Some(!trimmed.is_empty()),
            action: on_run,
        });
    }
    if let Some(on_run_selection) = args.handlers.on_run_selection.clone() {
        items.push(NativeMenuItemDef::Item {
            id: "run-selection".into(),
            label: args.labels.run_selection.clone(),
            shortcut: Some(format!("{mod_key}+Enter")),
            enabled: Some(args.has_selection),
            action: on_run_selection,
        });
    }
    if let Some(on_format) = args.handlers.on_format.clone() {
        items.push(NativeMenuItemDef::Item {
            id: "format".into(),
            label: args.labels.format.clone(),
            shortcut: None,
            enabled: Some(!trimmed.is_empty()),
            action: on_format,
        });
    }
    if let Some(on_comment) = args.handlers.on_comment.clone() {
        items.push(NativeMenuItemDef::Item {
            id: "comment".into(),
            label: args.labels.comment.clone(),
            shortcut: Some(format!("{mod_key}+/")),
            enabled: Some(!trimmed.is_empty() || args.has_selection),
            action: on_comment,
        });
    }

    // `if (handlers.onAddFavorite && labels.addFavorite)` — both required.
    if let (Some(on_add_favorite), Some(label)) =
        (args.handlers.on_add_favorite.clone(), args.labels.add_favorite.clone())
    {
        items.push(NativeMenuItemDef::Separator);
        let sql = trimmed.to_string();
        items.push(NativeMenuItemDef::Item {
            id: "add-favorite".into(),
            label,
            shortcut: Some(format!("{mod_key}+S")),
            enabled: Some(!sql.is_empty()),
            // `() => { if (trimmed) handlers.onAddFavorite?.(trimmed); }`
            action: Rc::new(move || {
                if !sql.is_empty() {
                    on_add_favorite(&sql);
                }
            }),
        });
    }

    items
}
