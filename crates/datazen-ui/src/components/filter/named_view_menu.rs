//! Port of `packages/frontend/src/components/filter/NamedViewMenu.tsx`.
//!
//! The saved-views menu is a six-state machine — `idle`, `loading`, `list`,
//! `naming`, `importing`, `error` — with an exit from every state. React keeps
//! the machine in component state and awaits three injected commands plus two
//! file commands. Rune is immediate mode, so:
//!
//! * the whole machine lives in [`NamedViewVm`], which the host embeds in `S`;
//! * [`step`] is the pure transition function: a user [`NamedViewAction`]
//!   mutates the VM and returns the [`NamedViewEffect`]s the host must perform;
//! * the host feeds command answers back with [`on_list`], [`on_save`],
//!   [`on_remove`] and [`on_error`]. No fake async or thread is created.
//!
//! The pure helpers mirror `lib/namedViews.ts` (name normalization, scope key,
//! sort, import-name dedupe, filter readout) so the menu and a future host port
//! reach the same answers. They are local stand-ins: removal stage is when the
//! Rust `namedViews` library and the `NamedView` backend-client DTO land.
//!
//! ## Keys
//!
//! React `data-testid`s are kept as the prefix of Rune keys. Where React repeats
//! a testid on every row (`named-view-item`, `named-view-apply`, …) the Rune key
//! carries an id/index suffix to stay unique among siblings, and the bare
//! testid is present once as an inert marker.
//!
//! `ui::menu::Menu` is not used for the panel: React's panel is a state-machine
//! surface (label/inputs/buttons), not a menu listbox. Kit `Button`/`TextField`
//! and the registry icons render it; removal stage: if a menu-style popup is
//! ever required, wrap this panel with `ui::menu::Menu`.

use rune::TextEdit;

use crate::i18n::t;

use super::filter_operators::filter_operator_meta;

/// React `MenuState`.
#[allow(dead_code)] // VM surface; consumer: the filter table view (not landed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuState {
    Idle,
    Loading,
    List,
    Naming,
    Importing,
    Error,
}

#[allow(dead_code)] // matches React's `data-state`; delete with the host view.
impl MenuState {
    pub fn as_str(self) -> &'static str {
        match self {
            MenuState::Idle => "idle",
            MenuState::Loading => "loading",
            MenuState::List => "list",
            MenuState::Naming => "naming",
            MenuState::Importing => "importing",
            MenuState::Error => "error",
        }
    }

    pub fn is_open(self) -> bool {
        self != MenuState::Idle
    }
}

/// React `NamedViewScope`.
#[allow(dead_code)] // DTO stand-in; delete when the backend-client DTO lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedViewScope {
    pub connection_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

/// A filter carried by a view (operator kept as a string: the port must not
/// know the host's operator union, per `namedViews.ts`).
#[allow(dead_code)] // DTO stand-in; delete when the backend-client DTO lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewFilter {
    pub column: String,
    pub operator: String,
}

#[allow(dead_code)] // DTO stand-in; delete when the backend-client DTO lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewSort {
    pub column: String,
    pub descending: bool,
}

#[allow(dead_code)] // DTO stand-in; delete when the backend-client DTO lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterLogic {
    And,
    Or,
}

#[allow(dead_code)] // matches React's logic strings; delete with the DTO.
impl FilterLogic {
    pub fn as_str(self) -> &'static str {
        match self {
            FilterLogic::And => "and",
            FilterLogic::Or => "or",
        }
    }
}

/// React `NamedView` (the fields the menu reads/writes).
#[allow(dead_code)] // DTO stand-in; delete when the backend-client DTO lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedView {
    pub id: String,
    pub name: String,
    pub scope: NamedViewScope,
    pub filters: Vec<ViewFilter>,
    pub filter_logic: FilterLogic,
    pub sorts: Vec<ViewSort>,
    pub visible_columns: Option<Vec<String>>,
    pub is_default: bool,
}

/// The applied (and optionally draft) table state a view is taken from.
#[allow(dead_code)] // VM surface; delete with the host view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewState {
    pub filters: Vec<ViewFilter>,
    pub filter_logic: FilterLogic,
    pub sorts: Vec<ViewSort>,
    pub visible_columns: Option<Vec<String>>,
}

/// The complete observable state of the menu.
#[allow(dead_code)] // VM surface; delete with the host view.
#[derive(Clone, Debug)]
pub struct NamedViewVm {
    pub state: MenuState,
    pub scope: NamedViewScope,
    pub views: Vec<NamedView>,
    pub error: String,
    pub notice: String,
    pub name: TextEdit,
    pub renaming_id: Option<String>,
    pub rename_value: TextEdit,
    pub columns: Vec<String>,
    pub dirty: bool,
    pub applied: ViewState,
    pub draft: Option<ViewState>,
}

// ---- Pure helpers (stand-ins for `lib/namedViews.ts`) ------------------------------------------

/// Trim a name and collapse every run of whitespace into one space.
#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn normalize_view_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn normalized_name_key(name: &str) -> String {
    normalize_view_name(name).to_lowercase()
}

#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn view_names_equal(left: &str, right: &str) -> bool {
    normalized_name_key(left) == normalized_name_key(right)
}

/// The tagged, collision-free identity of a scope.
#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn scope_key(scope: &NamedViewScope) -> String {
    let part = |value: &Option<String>| match value {
        None => "n:".to_owned(),
        Some(text) => format!("s:{text}"),
    };
    format!(
        "c:{}|d:{}|s:{}|t:{}",
        scope.connection_id,
        part(&scope.database),
        part(&scope.schema),
        scope.table
    )
}

/// Default view first, then by normalized name, raw name and id.
#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn sort_named_views(views: &[NamedView]) -> Vec<NamedView> {
    let mut sorted = views.to_vec();
    sorted.sort_by(|a, b| {
        if a.is_default != b.is_default {
            return if a.is_default {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        normalized_name_key(&a.name)
            .cmp(&normalized_name_key(&b.name))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    sorted
}

fn contains_name(taken: &[String], name: &str) -> bool {
    let key = normalized_name_key(name);
    taken.iter().any(|existing| normalized_name_key(existing) == key)
}

/// `"Users"`, `"Users (2)"`, … for a name an import must not duplicate.
#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn resolve_import_name(name: &str, taken: &[String]) -> String {
    let base = normalize_view_name(name);
    if base.is_empty() || !contains_name(taken, &base) {
        return base;
    }
    for suffix in 2..100_000u32 {
        let candidate = format!("{base} ({suffix})");
        if !contains_name(taken, &candidate) {
            return candidate;
        }
    }
    format!("{base} (imported)")
}

/// What reading a view's filters found.
#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub struct FilterReadout {
    pub unknown_operators: Vec<String>,
}

#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn read_view_filters(filters: &[ViewFilter]) -> FilterReadout {
    let mut unknown = Vec::new();
    for condition in filters {
        if filter_operator_meta(&condition.operator).is_none() && !unknown.contains(&condition.operator) {
            unknown.push(condition.operator.clone());
        }
    }
    FilterReadout {
        unknown_operators: unknown,
    }
}

/// A view of this state — a new view carries an empty id for the host to mint.
#[allow(dead_code)] // helper; delete when namedViews.ts is ported.
pub fn view_from_state(
    scope: &NamedViewScope,
    state: &ViewState,
    name: &str,
) -> NamedView {
    NamedView {
        id: String::new(),
        name: name.to_owned(),
        scope: scope.clone(),
        filters: state.filters.clone(),
        filter_logic: state.filter_logic,
        sorts: state.sorts.clone(),
        visible_columns: state.visible_columns.clone(),
        is_default: false,
    }
}

fn replace_in_list(views: &mut Vec<NamedView>, saved: &NamedView) {
    // React `handleSetDefault`: a default is exclusive within its scope.
    // Clearing here covers every save path (new, rename, default, import).
    if saved.is_default {
        let key = scope_key(&saved.scope);
        for entry in views.iter_mut() {
            if scope_key(&entry.scope) == key && entry.id != saved.id {
                entry.is_default = false;
            }
        }
    }
    views.retain(|entry| entry.id != saved.id);
    views.push(saved.clone());
    let sorted = sort_named_views(views);
    *views = sorted;
}

// ---- The state machine -------------------------------------------------------------------------

/// What the menu asks its host to do. No command is performed by the widget.
#[allow(dead_code)] // VM surface; consumer: the host toolbar (not landed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamedViewEffect {
    List(NamedViewScope),
    Save(NamedView),
    Remove(String),
    Apply(NamedView),
    ApplyDraft,
    ConfirmDelete(String),
    ConfirmOverwrite {
        view: NamedView,
        new_name: String,
    },
    WriteExport {
        file_name: String,
        content: String,
    },
    ReadImport,
}

/// A user intent. The view binds these to kit presses and field keys.
#[allow(dead_code)] // VM surface; consumer: the host toolbar (not landed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamedViewAction {
    Trigger,
    Retry,
    SaveAs,
    CancelNaming,
    Save,
    ApplyFirstAndSave,
    StartRename(String),
    CommitRename(String),
    CancelRename,
    Apply(String),
    SetDefault(String),
    Delete(String),
    Confirm(String),
    CancelConfirm,
    Export,
    Import,
}

fn find_view<'a>(views: &'a [NamedView], id: &str) -> Option<&'a NamedView> {
    views.iter().find(|view| view.id == id)
}

fn scoped(vm: &NamedViewVm) -> Vec<NamedView> {
    let key = scope_key(&vm.scope);
    sort_named_views(
        &vm.views
            .iter()
            .filter(|view| scope_key(&view.scope) == key)
            .cloned()
            .collect::<Vec<_>>(),
    )
}

fn is_name_taken(vm: &NamedViewVm, name: &str) -> bool {
    scoped(vm).iter().any(|view| view_names_equal(&view.name, name))
}

/// Apply one action. Returns the effects the host must perform; the VM already
/// holds every synchronous transition so the next frame paints the new state.
#[allow(dead_code)] // state machine; consumer: the host toolbar (not landed).
pub fn step(vm: &mut NamedViewVm, action: NamedViewAction) -> Vec<NamedViewEffect> {
    match action {
        NamedViewAction::Trigger => {
            if vm.state.is_open() {
                close(vm);
                Vec::new()
            } else {
                vm.state = MenuState::Loading;
                vm.error.clear();
                vec![NamedViewEffect::List(vm.scope.clone())]
            }
        }
        NamedViewAction::Retry => {
            vm.state = MenuState::Loading;
            vm.error.clear();
            vec![NamedViewEffect::List(vm.scope.clone())]
        }
        NamedViewAction::SaveAs => {
            vm.name.replace_text(String::new());
            vm.state = MenuState::Naming;
            Vec::new()
        }
        NamedViewAction::CancelNaming => {
            vm.name.replace_text(String::new());
            vm.state = MenuState::List;
            Vec::new()
        }
        NamedViewAction::Save => {
            let name = normalize_view_name(&vm.name.text);
            if name.is_empty() || is_name_taken(vm, &name) {
                return Vec::new();
            }
            let record = view_from_state(&vm.scope, &vm.applied, &name);
            vec![NamedViewEffect::Save(record)]
        }
        NamedViewAction::ApplyFirstAndSave => {
            let name = normalize_view_name(&vm.name.text);
            if name.is_empty() || is_name_taken(vm, &name) {
                return Vec::new();
            }
            let state = vm.draft.clone().unwrap_or_else(|| vm.applied.clone());
            let record = view_from_state(&vm.scope, &state, &name);
            let mut effects = vec![NamedViewEffect::ApplyDraft];
            effects.push(NamedViewEffect::Save(record));
            effects
        }
        NamedViewAction::StartRename(id) => {
            if let Some(view) = find_view(&vm.views, &id) {
                vm.renaming_id = Some(id);
                vm.rename_value.replace_text(view.name.clone());
            }
            Vec::new()
        }
        NamedViewAction::CancelRename => {
            vm.renaming_id = None;
            Vec::new()
        }
        NamedViewAction::CommitRename(id) => {
            let name = normalize_view_name(&vm.rename_value.text);
            vm.renaming_id = None;
            let Some(view) = find_view(&vm.views, &id).cloned() else {
                return Vec::new();
            };
            if name.is_empty() || view_names_equal(&view.name, &name) {
                return Vec::new();
            }
            let conflict = scoped(vm)
                .into_iter()
                .find(|entry| entry.id != id && view_names_equal(&entry.name, &name));
            if let Some(conflict) = conflict {
                return vec![NamedViewEffect::ConfirmOverwrite {
                    view,
                    new_name: name,
                }];
            }
            vec![NamedViewEffect::Save(NamedView {
                name,
                ..view
            })]
        }
        NamedViewAction::Apply(id) => {
            let Some(view) = find_view(&vm.views, &id).cloned() else {
                return Vec::new();
            };
            let readout = read_view_filters(&view.filters);
            if !readout.unknown_operators.is_empty() {
                vm.error = t("filter.errorUnsupportedOperator")
                    .replace("{operator}", &readout.unknown_operators[0])
                    .replace("{alternatives}", &readout.unknown_operators[1..].join(", "));
                vm.state = MenuState::Error;
                return Vec::new();
            }
            let missing: Vec<&String> = view
                .filters
                .iter()
                .map(|condition| &condition.column)
                .filter(|column| !vm.columns.contains(column))
                .collect();
            if let Some(column) = missing.first() {
                vm.error = t("filter.view.missingColumn").replace("{column}", column);
                vm.state = MenuState::Error;
                return Vec::new();
            }
            // React `handleApply` closes the menu after a successful apply.
            close(vm);
            vec![NamedViewEffect::Apply(view)]
        }
        NamedViewAction::SetDefault(id) => {
            let Some(view) = find_view(&vm.views, &id).cloned() else {
                return Vec::new();
            };
            vec![NamedViewEffect::Save(NamedView {
                is_default: true,
                ..view
            })]
        }
        NamedViewAction::Delete(id) => vec![NamedViewEffect::ConfirmDelete(id)],
        NamedViewAction::Confirm(id) => {
            // The host tells the menu which confirmation was for; a pending
            // rename is encoded in the id as `@rename:<view-id>`.
            if let Some(view_id) = id.strip_prefix("@rename:") {
                let name = normalize_view_name(&vm.rename_value.text);
                let Some(view) = find_view(&vm.views, view_id).cloned() else {
                    return Vec::new();
                };
                // React `handleRename` on a name conflict saves the renamed
                // view under the conflicting view's id (taking it over) and
                // removes the original id. Recompute the conflict from the
                // live state so the takeover id is not lost between the
                // ConfirmOverwrite effect and this confirmation.
                let conflict = scoped(vm)
                    .into_iter()
                    .find(|entry| entry.id != view.id && view_names_equal(&entry.name, &name));
                let takeover_id = conflict.map(|entry| entry.id).unwrap_or_else(|| view.id.clone());
                return vec![
                    NamedViewEffect::Save(NamedView {
                        id: takeover_id,
                        name,
                        ..view.clone()
                    }),
                    NamedViewEffect::Remove(view.id),
                ];
            }
            vec![NamedViewEffect::Remove(id)]
        }
        NamedViewAction::CancelConfirm => Vec::new(),
        NamedViewAction::Export => {
            let views = scoped(vm);
            let content = format!("{{\"version\":1,\"views\":{}}}", views.len());
            vec![NamedViewEffect::WriteExport {
                file_name: format!("{}-views.json", vm.scope.table),
                content,
            }]
        }
        NamedViewAction::Import => {
            vm.state = MenuState::Importing;
            vm.error.clear();
            vec![NamedViewEffect::ReadImport]
        }
    }
}

fn close(vm: &mut NamedViewVm) {
    vm.state = MenuState::Idle;
    vm.error.clear();
    vm.notice.clear();
    vm.renaming_id = None;
}

/// The list command answered.
#[allow(dead_code)] // host result hook; delete with the host view.
pub fn on_list(vm: &mut NamedViewVm, views: Vec<NamedView>) {
    vm.views = sort_named_views(&views);
    vm.state = MenuState::List;
}

/// A save command answered.
#[allow(dead_code)] // host result hook; delete with the host view.
pub fn on_save(vm: &mut NamedViewVm, saved: NamedView) {
    replace_in_list(&mut vm.views, &saved);
    vm.state = MenuState::List;
    vm.error.clear();
}

/// A remove command answered.
#[allow(dead_code)] // host result hook; delete with the host view.
pub fn on_remove(vm: &mut NamedViewVm, remaining: Vec<NamedView>) {
    vm.views = sort_named_views(&remaining);
    vm.state = MenuState::List;
}

/// A command failed.
#[allow(dead_code)] // host result hook; delete with the host view.
pub fn on_error(vm: &mut NamedViewVm, message: String) {
    let classified = if message.contains("grid.view.nameExists") {
        t("filter.view.errorNameExists").to_owned()
    } else {
        message
    };
    vm.error = classified;
    vm.state = MenuState::Error;
}

/// An import file was read: dedupe names and report what the host must save.
#[allow(dead_code)] // host result hook; delete with the host view.
pub fn on_import(vm: &mut NamedViewVm, incoming: Vec<NamedView>) -> Vec<NamedViewEffect> {
    let mut taken: Vec<String> = scoped(vm).into_iter().map(|view| view.name).collect();
    let mut effects = Vec::new();
    let mut imported = 0usize;
    for entry in incoming {
        let readout = read_view_filters(&entry.filters);
        if !readout.unknown_operators.is_empty() {
            let message = t("filter.view.importFailed").replace(
                "{error}",
                &t("filter.errorUnsupportedOperator")
                    .replace("{operator}", &readout.unknown_operators[0])
                    .replace("{alternatives}", &readout.unknown_operators[1..].join(", ")),
            );
            vm.error = message;
            vm.state = MenuState::Error;
            return effects;
        }
        let name = resolve_import_name(&entry.name, &taken);
        taken.push(name.clone());
        let record = NamedView {
            name,
            scope: vm.scope.clone(),
            ..entry
        };
        effects.push(NamedViewEffect::Save(record));
        imported += 1;
    }
    vm.notice = t("filter.view.importDone")
        .replace("{count}", &imported.to_string());
    vm.state = MenuState::List;
    effects
}

mod view;

pub use view::NamedViewMenu;

// Leftover L1 (removal stage): when the filter table view mounts this menu,
// keep the VM in its state slice, bind the two text accessors and translate the
// effects to the host's named-view commands. No IPC, timer or thread is created
// here. The export content is a deterministic stand-in until a Rust
// `buildNamedViewExport` lands; the confirm dialogs are effects for the host.
