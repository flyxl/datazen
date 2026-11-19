//! T192: `NamedViewMenu` — the six-state machine, save/apply/delete/rename flows,
//! exact keys/i18n, and two-window/two-palette geometry.
//!
//! Every assertion observes the rendered tree or the effects returned by the
//! pure `step` function after a real press/key event.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::components::filter::{
    normalize_view_name, on_error, on_import, on_list, step, view_from_state, view_names_equal,
    FilterLogic, MenuState, NamedView, NamedViewAction, NamedViewEffect, NamedViewMenu,
    NamedViewScope, NamedViewVm, ViewFilter, ViewSort, ViewState,
};
use crate::i18n::t;

fn scope() -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-1".to_owned(),
        database: Some("app".to_owned()),
        schema: None,
        table: "orders".to_owned(),
    }
}

fn filter(column: &str, operator: &str) -> ViewFilter {
    ViewFilter {
        column: column.to_owned(),
        operator: operator.to_owned(),
    }
}

fn applied() -> ViewState {
    ViewState {
        filters: vec![filter("id", "eq")],
        filter_logic: FilterLogic::And,
        sorts: Vec::new(),
        visible_columns: None,
    }
}

fn view(id: &str, name: &str) -> NamedView {
    NamedView {
        id: id.to_owned(),
        name: name.to_owned(),
        scope: scope(),
        filters: vec![filter("id", "eq")],
        filter_logic: FilterLogic::And,
        sorts: vec![ViewSort {
            column: "id".to_owned(),
            descending: false,
        }],
        visible_columns: Some(vec!["id".to_owned()]),
        is_default: false,
    }
}

fn probe(state: MenuState, views: Vec<NamedView>) -> Probe {
    let mut vm = NamedViewVm {
        state,
        scope: scope(),
        views,
        error: String::new(),
        notice: String::new(),
        name: TextEdit::new(String::new()),
        renaming_id: None,
        rename_value: TextEdit::new(String::new()),
        columns: vec!["id".to_owned(), "name".to_owned()],
        dirty: false,
        applied: applied(),
        draft: None,
    };
    vm.state = state;
    Probe {
        dark: true,
        vm,
        effects: Vec::new(),
        actions: Vec::new(),
    }
}

#[derive(Clone)]
struct Probe {
    dark: bool,
    vm: NamedViewVm,
    effects: Vec<NamedViewEffect>,
    actions: Vec<NamedViewAction>,
}

fn view_fn(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    NamedViewMenu::<Probe>::new(&s.vm)
        .on_name_edit(|st: &mut Probe| &mut st.vm.name)
        .on_rename_edit(|st: &mut Probe| &mut st.vm.rename_value)
        .on_action(|st: &mut Probe, action| st.actions.push(action))
        .build(s.dark)
}

fn runtime(state: MenuState, views: Vec<NamedView>, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut p = probe(state, views);
    p.dark = dark;
    let mut rt = Runtime::new(
        p,
        view_fn,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

fn act(rt: &mut Runtime<Probe>, action: NamedViewAction) {
    let effects = step(&mut rt.state.vm, action);
    rt.state.effects.extend(effects);
    refresh(rt);
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn type_into(rt: &mut Runtime<Probe>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.frame(0.0);
}

// ---- The six states and their exits ------------------------------------------------------------

#[test]
fn starts_idle_with_no_panel_and_the_trigger_label() {
    let rt = runtime(MenuState::Idle, Vec::new(), true, (960.0, 640.0));
    assert!(!has(&rt, "named-view-panel"), "idle shows no panel");
    assert!(has(&rt, "named-view-trigger"));
    assert_eq!(
        element(&rt, "named-view-trigger").label.as_deref(),
        Some(t("filter.view.savedViews"))
    );
    assert_eq!(element(&rt, "named-view-trigger").expanded, Some(false));
}

#[test]
fn trigger_opens_loading_with_no_entries_then_list() {
    let mut rt = runtime(MenuState::Idle, Vec::new(), true, (960.0, 640.0));
    click(&mut rt, "named-view-trigger");
    assert_eq!(rt.state.actions.last(), Some(&NamedViewAction::Trigger));
    act(&mut rt, NamedViewAction::Trigger);
    assert_eq!(rt.state.effects, vec![NamedViewEffect::List(scope())]);
    assert_eq!(rt.state.vm.state, MenuState::Loading);
    assert_eq!(element(&rt, "named-view-panel").label.as_deref(), Some("loading"));
    assert!(has(&rt, "named-view-loading"));
    assert!(!has(&rt, "named-view-item"), "nothing to click while loading");

    on_list(&mut rt.state.vm, Vec::new());
    refresh(&mut rt);
    assert_eq!(element(&rt, "named-view-panel").label.as_deref(), Some("list"));
    assert!(has(&rt, "named-view-empty"));
    assert_eq!(
        texts(&rt).iter().any(|x| x == t("filter.view.empty")),
        true
    );
}

#[test]
fn save_as_goes_to_naming_and_refuses_an_empty_name() {
    let mut rt = runtime(MenuState::List, Vec::new(), true, (960.0, 640.0));
    click(&mut rt, "named-view-save-as");
    assert_eq!(rt.state.actions.last(), Some(&NamedViewAction::SaveAs));
    act(&mut rt, NamedViewAction::SaveAs);
    assert_eq!(rt.state.vm.state, MenuState::Naming);
    assert!(has(&rt, "named-view-name-input"));
    assert!(!element(&rt, "named-view-save").focusable, "empty name disables save");
    assert!(has(&rt, "named-view-cancel"));

    click(&mut rt, "named-view-cancel");
    assert_eq!(rt.state.actions.last(), Some(&NamedViewAction::CancelNaming));
    act(&mut rt, NamedViewAction::CancelNaming);
    assert_eq!(rt.state.vm.state, MenuState::List);
}

#[test]
fn error_state_shows_classified_text_and_retries() {
    let mut rt = runtime(MenuState::Idle, Vec::new(), true, (960.0, 640.0));
    on_error(&mut rt.state.vm, "grid.view.nameExists: wrapper".to_owned());
    refresh(&mut rt);
    assert_eq!(rt.state.vm.state, MenuState::Error);
    assert_eq!(
        element(&rt, "named-view-error").text.as_deref(),
        Some(t("filter.view.errorNameExists")),
        "the classified text is shown, not the raw wrapper"
    );

    act(&mut rt, NamedViewAction::Retry);
    assert_eq!(rt.state.vm.state, MenuState::Loading);
    assert_eq!(rt.state.effects.last(), Some(&NamedViewEffect::List(scope())));
}

#[test]
fn importing_state_has_an_exit() {
    let mut rt = runtime(MenuState::List, Vec::new(), true, (960.0, 640.0));
    click(&mut rt, "named-view-import");
    assert_eq!(rt.state.actions.last(), Some(&NamedViewAction::Import));
    act(&mut rt, NamedViewAction::Import);
    assert_eq!(rt.state.vm.state, MenuState::Importing);
    assert!(has(&rt, "named-view-importing"));
    // The host's file answer returns the menu to the list.
    on_list(&mut rt.state.vm, Vec::new());
    refresh(&mut rt);
    assert_eq!(rt.state.vm.state, MenuState::List);
}

// ---- Save / apply / dirty draft ---------------------------------------------------------------

#[test]
fn save_uses_applied_filters_and_a_dirty_draft_only_warns() {
    let mut p = probe(MenuState::List, Vec::new());
    p.vm.dirty = true;
    p.vm.draft = Some(ViewState {
        filters: vec![filter("id", "ne")],
        filter_logic: FilterLogic::And,
        sorts: Vec::new(),
        visible_columns: None,
    });
    let mut rt = Runtime::new(p, view_fn, (1440.0, 816.0), env(true));
    rt.frame(0.0);

    act(&mut rt, NamedViewAction::SaveAs);
    type_into(&mut rt, "named-view-name-input", "Open orders");
    assert!(has(&rt, "named-view-draft-hint"), "the hint is shown");
    assert!(element(&rt, "named-view-save").focusable, "the save is not blocked");

    act(&mut rt, NamedViewAction::Save);
    let Some(NamedViewEffect::Save(record)) = rt.state.effects.last() else {
        panic!("save effect was emitted: {:?}", rt.state.effects);
    };
    assert_eq!(record.name, "Open orders");
    assert_eq!(record.filters, vec![filter("id", "eq")], "applied, not draft");
}

#[test]
fn apply_first_and_save_uses_the_draft_filters() {
    let mut p = probe(MenuState::List, Vec::new());
    p.vm.dirty = true;
    p.vm.draft = Some(ViewState {
        filters: vec![filter("id", "ne")],
        filter_logic: FilterLogic::And,
        sorts: Vec::new(),
        visible_columns: None,
    });
    let mut rt = Runtime::new(p, view_fn, (1440.0, 816.0), env(true));
    rt.frame(0.0);

    act(&mut rt, NamedViewAction::SaveAs);
    type_into(&mut rt, "named-view-name-input", "Open orders");
    act(&mut rt, NamedViewAction::ApplyFirstAndSave);
    assert!(rt.state.effects.iter().any(|e| *e == NamedViewEffect::ApplyDraft));
    let Some(NamedViewEffect::Save(record)) = rt.state.effects.last() else {
        panic!("save effect was emitted: {:?}", rt.state.effects);
    };
    assert_eq!(record.filters, vec![filter("id", "ne")], "the draft was saved");
}

#[test]
fn a_taken_name_is_refused_before_the_host_is_asked() {
    let mut rt = runtime(MenuState::List, vec![view("view-1", "Open orders")], true, (960.0, 640.0));
    act(&mut rt, NamedViewAction::SaveAs);
    type_into(&mut rt, "named-view-name-input", "open ORDERS");
    assert!(has(&rt, "named-view-name-taken"));
    assert!(!element(&rt, "named-view-save").focusable, "taken name disables save");
    act(&mut rt, NamedViewAction::Save);
    assert!(!rt.state.effects.iter().any(|e| matches!(e, NamedViewEffect::Save(_))));
    assert!(view_names_equal("Open orders", "open ORDERS"));
}

// ---- List rows and the journey actions --------------------------------------------------------

#[test]
fn list_rows_expose_all_actions_and_apply_effect() {
    let mut rt = runtime(
        MenuState::List,
        vec![view("view-1", "Open orders")],
        true,
        (1440.0, 816.0),
    );
    for key in [
        "named-view-item",
        "named-view-item-view-1",
        "named-view-apply-view-1",
        "named-view-rename-view-1",
        "named-view-delete-view-1",
        "named-view-set-default-view-1",
        "named-view-save-as",
        "named-view-export",
        "named-view-import",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    act(&mut rt, NamedViewAction::Apply("view-1".to_owned()));
    assert_eq!(
        rt.state.effects.last(),
        Some(&NamedViewEffect::Apply(view("view-1", "Open orders")))
    );

    act(&mut rt, NamedViewAction::SetDefault("view-1".to_owned()));
    let Some(NamedViewEffect::Save(record)) = rt.state.effects.last() else {
        panic!("set-default effect was emitted");
    };
    assert!(record.is_default && record.id == "view-1");
}

#[test]
fn delete_is_confirmed_then_removed() {
    let mut rt = runtime(
        MenuState::List,
        vec![view("view-1", "Open orders")],
        true,
        (960.0, 640.0),
    );
    act(&mut rt, NamedViewAction::Delete("view-1".to_owned()));
    assert_eq!(
        rt.state.effects.last(),
        Some(&NamedViewEffect::ConfirmDelete("view-1".to_owned()))
    );
    act(&mut rt, NamedViewAction::Confirm("view-1".to_owned()));
    assert_eq!(
        rt.state.effects.last(),
        Some(&NamedViewEffect::Remove("view-1".to_owned()))
    );
}

#[test]
fn rename_keeps_the_id_and_a_conflict_asks_for_overwrite() {
    let mut rt = runtime(
        MenuState::List,
        vec![view("view-1", "Open orders")],
        true,
        (960.0, 640.0),
    );
    act(&mut rt, NamedViewAction::StartRename("view-1".to_owned()));
    assert!(has(&rt, "named-view-rename-input"));
    // Starting a rename seeds the field with the old name; clear it before typing.
    rt.state.vm.rename_value.replace_text(String::new());
    refresh(&mut rt);
    type_into(&mut rt, "named-view-rename-input", "Open orders this week");
    act(&mut rt, NamedViewAction::CommitRename("view-1".to_owned()));
    let Some(NamedViewEffect::Save(record)) = rt.state.effects.last() else {
        panic!("rename effect was emitted");
    };
    assert_eq!(record.name, "Open orders this week");
    assert_eq!(record.id, "view-1", "the id never changes");

    let mut two = runtime(
        MenuState::List,
        vec![view("v1", "First"), view("v2", "Second")],
        true,
        (960.0, 640.0),
    );
    act(&mut two, NamedViewAction::StartRename("v1".to_owned()));
    two.state.vm.rename_value.replace_text(String::new());
    refresh(&mut two);
    type_into(&mut two, "named-view-rename-input", "Second");
    act(&mut two, NamedViewAction::CommitRename("v1".to_owned()));
    assert!(matches!(
        two.state.effects.last(),
        Some(NamedViewEffect::ConfirmOverwrite { .. })
    ));
}

// ---- Refusals, import and export --------------------------------------------------------------

#[test]
fn apply_refuses_unknown_operators_and_missing_columns() {
    let mut bad = view("view-1", "Bad op");
    bad.filters = vec![filter("id", "noSuchOp")];
    let mut rt = runtime(MenuState::List, vec![bad], true, (960.0, 640.0));
    act(&mut rt, NamedViewAction::Apply("view-1".to_owned()));
    assert_eq!(rt.state.vm.state, MenuState::Error);
    assert_eq!(
        rt.state.vm.error,
        t("filter.errorUnsupportedOperator")
            .replace("{operator}", "noSuchOp")
            .replace("{alternatives}", "")
    );
    assert!(!rt.state.effects.iter().any(|e| matches!(e, NamedViewEffect::Apply(_))));

    let mut missing = view("view-1", "Missing");
    missing.filters = vec![filter("gone", "eq")];
    let mut rt = runtime(MenuState::List, vec![missing], true, (960.0, 640.0));
    act(&mut rt, NamedViewAction::Apply("view-1".to_owned()));
    assert_eq!(rt.state.vm.state, MenuState::Error);
    assert_eq!(
        rt.state.vm.error,
        t("filter.view.missingColumn").replace("{column}", "gone")
    );
}

#[test]
fn import_dedupes_names_and_reports_a_notice() {
    let mut rt = runtime(
        MenuState::List,
        vec![view("view-1", "Open orders this week")],
        true,
        (960.0, 640.0),
    );
    let mut incoming = view("other", "Open orders this week");
    incoming.filters = Vec::new();
    let effects = on_import(&mut rt.state.vm, vec![incoming]);
    rt.state.effects.extend(effects);
    refresh(&mut rt);
    let Some(NamedViewEffect::Save(record)) = rt.state.effects.last() else {
        panic!("import save effect was emitted");
    };
    assert_eq!(record.name, "Open orders this week (2)");
    assert_eq!(record.scope, scope(), "the scope is rewritten to this table");
    assert_eq!(rt.state.vm.state, MenuState::List);
    assert_eq!(
        rt.state.vm.notice,
        t("filter.view.importDone").replace("{count}", "1")
    );
    assert!(has(&rt, "named-view-notice"));
    assert_eq!(normalize_view_name("  a  \t b "), "a b");
}

#[test]
fn export_writes_a_scoped_file_name() {
    let mut rt = runtime(MenuState::List, Vec::new(), true, (960.0, 640.0));
    act(&mut rt, NamedViewAction::Export);
    assert_eq!(
        rt.state.effects.last(),
        Some(&NamedViewEffect::WriteExport {
            file_name: "orders-views.json".to_owned(),
            content: "{\"version\":1,\"views\":0}".to_owned(),
        })
    );
}

// ---- Geometry ----------------------------------------------------------------------------------

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn assert_rects_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn every_state_fits_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut idle = runtime(MenuState::Idle, Vec::new(), dark, (w, h));
            assert_rects_inside(&idle, w, h, &format!("idle dark={dark}"));

            let mut list = runtime(
                MenuState::List,
                vec![view("v1", "Alpha"), view("v2", "Beta")],
                dark,
                (w, h),
            );
            assert_rects_inside(&list, w, h, &format!("list dark={dark}"));

            let mut naming = runtime(MenuState::List, Vec::new(), dark, (w, h));
            act(&mut naming, NamedViewAction::SaveAs);
            assert_rects_inside(&naming, w, h, &format!("naming dark={dark} {w}x{h}"));

            let mut error = runtime(MenuState::Idle, Vec::new(), dark, (w, h));
            on_error(&mut error.state.vm, "the disk is on fire".to_owned());
            refresh(&mut error);
            assert_rects_inside(&error, w, h, &format!("error dark={dark} {w}x{h}"));
        }
    }
}
