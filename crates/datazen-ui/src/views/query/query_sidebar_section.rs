//! Port of `windows/connection/query/QuerySidebarSection.tsx`.
//!
//! React renders up to two 256pt sidebars beside the query editor: a file-first
//! favorites panel (refresh, root hint, open/delete, native context menu) and a
//! history panel (search, current/all scope, grouped entries, clear history).
//! The exported `useQueryContextPath` hook owns namespace/session effects; this
//! port keeps only its pure "select a context level" helper and routes every
//! command through [`QuerySidebarAction`] on an injected host port — no fake
//! IPC, clipboard write, timer or thread.
//!
//! ## Keys
//!
//! React `data-testid`s are kept verbatim: `query-favorites-panel`,
//! `favorites-refresh`, `favorites-root`, `query-history-panel`,
//! `history-scope-current`, `history-scope-all`, `history-scope-fallback-hint`,
//! `history-group-label` and `query-history-item` (the last two as hidden
//! markers because Rune keys must be unique). Row-level keys carry an index.
//!
//! ## Local stand-ins (removal stage)
//!
//! The VM mirrors the store slices the component reads; the grouping/filtering
//! helpers are pure ports of `historyGroups.ts`. No production host mounts this
//! section yet, so every reusable item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the query-panel host wires the sidebars".

use std::rc::Rc;

use rune::{col, icon, row, text, El, Icon, PointerPress, TextEdit};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};

use crate::i18n::t;
use crate::icons::{ICON_REFRESH, ICON_TRASH};
use crate::ui::kit::kit;
use crate::ui::menu::{context_menu, Item};

mod favorites;
pub(crate) mod history;

pub use history::{find_group, group_history, HistoryGroup};

pub const FAVORITES_PANEL_KEY: &str = "query-favorites-panel";
pub const FAVORITES_REFRESH_KEY: &str = "favorites-refresh";
pub const FAVORITES_ROOT_KEY: &str = "favorites-root";
pub const HISTORY_PANEL_KEY: &str = "query-history-panel";
pub const HISTORY_SEARCH_KEY: &str = "query-history-search";
pub const SCOPE_CURRENT_KEY: &str = "history-scope-current";
pub const SCOPE_ALL_KEY: &str = "history-scope-all";
pub const FALLBACK_KEY: &str = "history-scope-fallback-hint";
pub const GROUP_LABEL_KEY: &str = "history-group-label";
pub const HISTORY_ITEM_KEY: &str = "query-history-item";
pub const FAVORITE_MENU_KEY: &str = "favorite-context-menu";
pub const HISTORY_MENU_KEY: &str = "history-context-menu";
pub const HISTORY_HEADER_MENU_KEY: &str = "history-header-context-menu";

/// React's `FavoriteQuery` fields the panel reads.
#[allow(dead_code)] // VM mirror; remove when the host owns the store slice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FavoriteVm {
    pub id: String,
    pub title: String,
    pub sql: String,
}

/// React's `QueryHistoryEntry` fields the panel reads.
#[allow(dead_code)] // VM mirror; remove when the host owns the store slice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryEntryVm {
    pub id: String,
    pub sql: String,
    pub database: Option<String>,
    pub success: bool,
    pub execution_time_ms: u64,
    pub rows_affected: Option<u64>,
    /// RFC3339 timestamp, mirrored verbatim for the host's formatter.
    pub executed_at: String,
}

/// React `historyScopeMode`.
#[allow(dead_code)] // VM mirror; remove when the host owns the scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryScope {
    #[default]
    Current,
    All,
}

/// A grouped section after search filtering (React's `scopedSections` entry).
#[allow(dead_code)] // VM helper; remove when the panel host computes sections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistorySection {
    pub key: String,
    /// `None` for the single current-database section (no group header).
    pub label: Option<String>,
    pub items: Vec<HistoryEntryVm>,
}

/// React props plus the two local `useState` values.
#[allow(dead_code)] // VM; owned by the host until the panel view lands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SidebarSectionVm {
    pub favorites_visible: bool,
    pub history_visible: bool,
    pub favorites_root: Option<String>,
    pub favorites: Vec<FavoriteVm>,
    pub history: Vec<HistoryEntryVm>,
    pub selected_database: Option<String>,
    pub history_search: TextEdit,
    pub history_scope: HistoryScope,
    /// Context-menu anchors: which row the menu came from, and where.
    pub favorite_menu: Option<(FavoriteVm, PointerPress)>,
    pub history_menu: Option<(String, PointerPress)>,
    pub header_menu: Option<PointerPress>,
}

#[allow(dead_code)] // pure helpers; remove when the shared libs are ported.
impl SidebarSectionVm {
    /// React `historyScopeFallback`: current scope but no matching group.
    pub fn scope_fallback(&self, groups: &[HistoryGroup]) -> bool {
        self.history_scope == HistoryScope::Current
            && history::find_group(groups, self.selected_database.as_deref()).is_none()
    }

    /// React's `scopedSections` + `filteredCount`, including case-insensitive
    /// SQL search within the active scope.
    pub fn sections(&self, groups: &[HistoryGroup]) -> Vec<HistorySection> {
        let query = self.history_search.text.trim().to_ascii_lowercase();
        let filter = |items: &[HistoryEntryVm]| -> Vec<HistoryEntryVm> {
            if query.is_empty() {
                items.to_vec()
            } else {
                items
                    .iter()
                    .filter(|entry| entry.sql.to_ascii_lowercase().contains(&query))
                    .cloned()
                    .collect()
            }
        };
        if self.history_scope == HistoryScope::Current {
            if let Some(group) = history::find_group(groups, self.selected_database.as_deref()) {
                return vec![HistorySection {
                    key: group.key.clone(),
                    label: None,
                    items: filter(&group.entries),
                }];
            }
        }
        groups
            .iter()
            .map(|group| HistorySection {
                key: group.key.clone(),
                label: Some(group.label.clone()),
                items: filter(&group.entries),
            })
            .collect()
    }

    /// The pure part of `handleSelectContextLevel`: a non-empty level extends
    /// the path up to `index`; an empty value is ignored.
    pub fn select_context_level(path: &[String], index: usize, value: &str) -> Option<Vec<String>> {
        if value.is_empty() {
            return None;
        }
        let mut next: Vec<String> = path.iter().take(index).cloned().collect();
        next.push(value.to_owned());
        Some(next)
    }

    /// Reference host reducer for the view-owned state; commands stay with the
    /// host. Consumers: the query-panel host (not landed) and the tests.
    pub fn reduce(&mut self, action: &QuerySidebarAction) {
        match action {
            QuerySidebarAction::SetHistoryScope(scope) => self.history_scope = *scope,
            QuerySidebarAction::SetHistorySearch(value) => self.history_search = TextEdit::new(value.clone()),
            QuerySidebarAction::FavoriteContextMenu { favorite, press } => {
                self.favorite_menu = Some((favorite.clone(), *press));
            }
            QuerySidebarAction::HistoryContextMenu { sql, press } => {
                self.history_menu = Some((sql.clone(), *press));
            }
            QuerySidebarAction::HistoryHeaderContextMenu(press) => self.header_menu = Some(*press),
            QuerySidebarAction::CloseMenu => {
                self.favorite_menu = None;
                self.history_menu = None;
                self.header_menu = None;
            }
            QuerySidebarAction::RefreshFavorites
            | QuerySidebarAction::OpenFavorite(_)
            | QuerySidebarAction::DeleteFavorite(_)
            | QuerySidebarAction::ApplySql(_)
            | QuerySidebarAction::CopySql(_)
            | QuerySidebarAction::SelectHistory(_)
            | QuerySidebarAction::ClearHistory
            | QuerySidebarAction::SelectContextPath(_) => {}
        }
    }
}

/// Every interaction the two sidebars ask the host to perform.
#[allow(dead_code)] // host port; remove when the panel wires the sidebars.
#[derive(Clone, Debug, PartialEq)]
pub enum QuerySidebarAction {
    RefreshFavorites,
    OpenFavorite(FavoriteVm),
    DeleteFavorite(String),
    FavoriteContextMenu { favorite: FavoriteVm, press: PointerPress },
    ApplySql(String),
    CopySql(String),
    SetHistorySearch(String),
    SetHistoryScope(HistoryScope),
    SelectHistory(String),
    ClearHistory,
    HistoryContextMenu { sql: String, press: PointerPress },
    HistoryHeaderContextMenu(PointerPress),
    CloseMenu,
    /// The namespace-level choice from `useQueryContextPath`.
    SelectContextPath(Vec<String>),
}

#[allow(dead_code)] // host port; remove when the panel wires the sidebars.
pub type QuerySidebarActionFn<S> = Rc<dyn Fn(&mut S, QuerySidebarAction) + 'static>;
#[allow(dead_code)] // text accessor; remove when the panel owns the search field.
pub type SearchEditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// A 24pt icon-only control on a panel header.
pub fn icon_button<S: 'static>(
    key: &str,
    label: &str,
    glyph: &'static Icon,
    press: Option<Rc<dyn Fn(&mut S) + 'static>>,
    dark: bool,
) -> El {
    let mut button = IconButton::new(glyph)
        .variant(IconButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .label(label.to_owned())
        .build(kit(dark))
        .key(key.to_owned());
    if let Some(press) = press {
        button = button.on_press(move |s: &mut S| press(s));
    }
    button
}

/// A trailing separator line, matching React's `border-b border-edge`.
pub fn bottom_border(dark: bool) -> El {
    row()
        .w_full()
        .h(1.0)
        .fill(kit(dark).c.border_base)
        .a11y_hidden()
}

/// Render the two optional panels into `layer`.
#[allow(dead_code)] // view entry; remove when the panel wires the sidebars.
pub fn build<S: 'static>(
    vm: &SidebarSectionVm,
    on_action: &Option<QuerySidebarActionFn<S>>,
    search_edit: Option<SearchEditFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    if vm.favorites_visible {
        favorites::build(vm, on_action, dark, layer);
    }
    if vm.history_visible {
        history::build(vm, on_action, search_edit, dark, layer);
    }
    if let Some((favorite, press)) = vm.favorite_menu.as_ref() {
        build_favorite_menu(favorite, *press, on_action, dark, layer);
    }
    if let Some((sql, press)) = vm.history_menu.as_ref() {
        build_history_menu(sql, *press, on_action, dark, layer);
    }
    if let Some(press) = vm.header_menu {
        build_header_menu(press, on_action, dark, layer);
    }
}

fn menu_trigger(key: &str) -> El {
    rune::el().key(format!("{key}-trigger")).size(0.0, 0.0).a11y_hidden()
}

fn menu_emit<S: 'static>(
    on_action: &Option<QuerySidebarActionFn<S>>,
    action: QuerySidebarAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

fn build_favorite_menu<S: 'static>(
    favorite: &FavoriteVm,
    press: PointerPress,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let menu = context_menu::<S>(
        FAVORITE_MENU_KEY,
        menu_trigger(FAVORITE_MENU_KEY),
        Some(press),
    )
    .min_width(180.0)
    .item(Item::new(t("query.openInNewTab")).on_select(menu_emit(
        on_action,
        QuerySidebarAction::OpenFavorite(favorite.clone()),
    )))
    .item(Item::new(t("query.applySql")).on_select(menu_emit(
        on_action,
        QuerySidebarAction::ApplySql(favorite.sql.clone()),
    )))
    .item(Item::new(t("common.copySql")).on_select(menu_emit(
        on_action,
        QuerySidebarAction::CopySql(favorite.sql.clone()),
    )))
    .item(Item::new(t("common.delete")).on_select(menu_emit(
        on_action,
        QuerySidebarAction::DeleteFavorite(favorite.id.clone()),
    )))
    .on_close(menu_emit(on_action, QuerySidebarAction::CloseMenu));
    let _ = menu.build(kit(dark), layer);
}

fn build_history_menu<S: 'static>(
    sql: &str,
    press: PointerPress,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let menu = context_menu::<S>(HISTORY_MENU_KEY, menu_trigger(HISTORY_MENU_KEY), Some(press))
        .min_width(180.0)
        .item(Item::new(t("query.applySql")).on_select(menu_emit(
            on_action,
            QuerySidebarAction::ApplySql(sql.to_owned()),
        )))
        .item(Item::new(t("common.copySql")).on_select(menu_emit(
            on_action,
            QuerySidebarAction::CopySql(sql.to_owned()),
        )))
        .on_close(menu_emit(on_action, QuerySidebarAction::CloseMenu));
    let _ = menu.build(kit(dark), layer);
}

fn build_header_menu<S: 'static>(
    press: PointerPress,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let menu = context_menu::<S>(
        HISTORY_HEADER_MENU_KEY,
        menu_trigger(HISTORY_HEADER_MENU_KEY),
        Some(press),
    )
    .min_width(180.0)
    .item(Item::new(t("query.clearHistory")).on_select(menu_emit(
        on_action,
        QuerySidebarAction::ClearHistory,
    )))
    .on_close(menu_emit(on_action, QuerySidebarAction::CloseMenu));
    let _ = menu.build(kit(dark), layer);
}
