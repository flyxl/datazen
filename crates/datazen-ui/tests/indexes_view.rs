//! T448: `windows/connection/IndexesView.tsx` — exact keys/i18n, six-column
//! rows and unique/primary flags, create/drop actions through the host port,
//! the empty/loading/error branches and rect geometry at both window sizes in
//! both themes. Every command is recorded as an [`IndexesAction`]; no fake IPC.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::relation_metadata::{ColumnSchema, IndexInfo};
use datazen_ui::views::indexes_view::{
    build, IndexMethod, IndexesAction, IndexesActionFn, IndexesHostPort, IndexesIdentity,
    IndexesViewState, IndexesVmFn, CREATE_KEY, DELETE_KEY, EMPTY_KEY, ERROR_KEY, HEADER_KEY,
    LOADING_KEY, ROOT_KEY, TABLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    identity: IndexesIdentity,
    vm: IndexesViewState,
    actions: Rc<RefCell<Vec<IndexesAction>>>,
    navigated: Rc<RefCell<Vec<String>>>,
    dark: bool,
    no_port: bool,
}

fn identity() -> IndexesIdentity {
    IndexesIdentity {
        db_session_id: "session-1".to_owned(),
        table_name: "users".to_owned(),
        database: "app_db".to_owned(),
        database_type: "postgresql".to_owned(),
        safe_mode: false,
        create_index_trigger: 0,
    }
}

fn index(name: &str, columns: &[&str], unique: bool, primary: bool, kind: Option<&str>) -> IndexInfo {
    IndexInfo {
        name: name.to_owned(),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        is_unique: unique,
        is_primary: primary,
        index_type: kind.map(|k| k.to_owned()),
    }
}

fn column(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        ..Default::default()
    }
}

fn scene(state: IndexesViewState, dark: bool) -> Scene {
    Scene {
        identity: identity(),
        vm: state,
        actions: Rc::new(RefCell::new(Vec::new())),
        navigated: Rc::new(RefCell::new(Vec::new())),
        dark,
        no_port: false,
    }
}

fn table_state() -> IndexesViewState {
    IndexesViewState {
        indexes: vec![
            index("users_pkey", &["id"], true, true, Some("btree")),
            index("idx_users_email", &["email"], true, false, None),
            index("idx_users_name", &["name"], false, false, Some("hash")),
        ],
        columns: vec![column("id", "integer"), column("email", "text"), column("name", "text")],
        ..Default::default()
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    if s.no_port {
        build::<Scene>(
            &s.identity,
            &s.vm,
            &IndexesHostPort::<Scene>::default(),
            None,
            s.dark,
            &mut layer,
        );
        return col().w_full().h_full().children(layer);
    }
    let actions = s.actions.clone();
    let table_name = s.identity.table_name.clone();
    let on_action: IndexesActionFn<Scene> = Rc::new(move |state: &mut Scene, action| {
        state.vm.reduce(&action, &table_name);
        actions.borrow_mut().push(action);
    });
    let navigated = s.navigated.clone();
    let on_edit: Rc<dyn Fn(&mut Scene, &str)> = Rc::new(move |_state: &mut Scene, name| {
        navigated.borrow_mut().push(name.to_owned());
    });
    let host = IndexesHostPort {
        on_action: Some(on_action),
        on_edit_structure: Some(on_edit),
    };
    let vm_edit: IndexesVmFn<Scene> = Rc::new(|s: &mut Scene| &mut s.vm);
    build::<Scene>(&s.identity, &s.vm, &host, Some(vm_edit), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn settle(rt: &mut Runtime<Scene>) {
    // Rune fades newly mounted modal panels in; let the entrance finish so the
    // dialog's controls are fully opaque and take pointer events.
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn all_actions(rt: &Runtime<Scene>) -> Vec<IndexesAction> {
    rt.state.actions.borrow().clone()
}

#[test]
fn the_six_headers_and_count_are_the_exact_react_copy() {
    let mut rt = runtime(scene(table_state(), true), DESIGN);
    rt.frame(0.0);
    let expected = [
        t("indexes.colName"),
        t("indexes.colColumns"),
        t("indexes.colType"),
        t("indexes.colUnique"),
        t("indexes.colPrimary"),
        t("indexes.colActions"),
    ];
    for (at, title) in expected.iter().enumerate() {
        assert_eq!(
            text_of(&rt, &format!("idx-head-{at}")),
            *title,
            "header {at} uses the React i18n key"
        );
    }
    assert_eq!(text_of(&rt, "idx-title"), "users", "the table name is the title");
    let count = t("common.indexCount").replace("{count}", "3");
    assert_eq!(text_of(&rt, "idx-count"), count, "the count uses common.indexCount");
    assert_eq!(
        text_of(&rt, "idx-new-index-label"),
        t("common.newIndex"),
        "the new-index button uses common.newIndex"
    );
    for key in [ROOT_KEY, HEADER_KEY, TABLE_KEY, "idx-head", "idx-row-users_pkey"] {
        assert!(has_key(&rt, key), "{key} is composed");
    }
}

#[test]
fn rows_carry_columns_flags_and_a_default_type_badge() {
    let rt = runtime(scene(table_state(), false), MIN);
    assert_eq!(
        text_of(&rt, "idx-name-idx_users_email"),
        "idx_users_email",
        "the name cell shows the index name"
    );
    assert_eq!(
        text_of(&rt, "idx-unique-idx_users_email"),
        "YES",
        "the unique flag renders YES for a unique index"
    );
    assert_eq!(
        text_of(&rt, "idx-unique-idx_users_name"),
        "NO",
        "the unique flag renders NO for a non-unique index"
    );
    assert_eq!(
        text_of(&rt, "idx-primary-users_pkey"),
        "YES",
        "the primary flag renders YES for the primary index"
    );
    assert_eq!(
        text_of(&rt, "idx-primary-idx_users_email"),
        "NO",
        "the primary flag renders NO for a secondary index"
    );
    assert!(has_key(&rt, "idx-column-idx_users_email-0"), "the column chip is composed");
    // React's `idx.indexType ?? 'btree'`: a missing type shows btree.
    let badge_types: Vec<String> = rt
        .elements()
        .into_iter()
        .filter(|e| {
            e.key.as_deref().is_some_and(|k| k.starts_with("idx-type-text-"))
        })
        .filter_map(|e| e.text)
        .collect();
    assert!(badge_types.contains(&"btree".to_owned()), "a missing indexType falls back to btree");
    assert!(badge_types.contains(&"hash".to_owned()), "an explicit indexType is shown verbatim");
}

#[test]
fn delete_is_hidden_for_primary_indexes_and_emits_a_request() {
    let mut rt = runtime(scene(table_state(), true), DESIGN);
    rt.frame(0.0);
    assert!(has_key(&rt, "idx-delete-index-idx_users_email"), "a secondary index can be deleted");
    assert!(
        !has_key(&rt, "idx-delete-index-users_pkey"),
        "the primary index never offers a delete button"
    );
    press(&mut rt, "idx-delete-index-idx_users_email");
    assert!(
        all_actions(&rt).contains(&IndexesAction::RequestDelete("idx_users_email".to_owned())),
        "pressing the row trash requests the matching delete target"
    );
    assert!(has_key(&rt, DELETE_KEY), "the request opens the confirm dialog");
    assert_eq!(
        text_of(&rt, "idx-delete-title"),
        t("indexes.confirmDeleteTitle"),
        "the confirm title uses the React key"
    );
    let message = t("indexes.confirmDeleteMsg").replace("{name}", "idx_users_email");
    assert_eq!(text_of(&rt, "idx-delete-msg"), message, "the confirm message carries the name");
}

#[test]
fn the_delete_confirm_emits_the_dialect_drop_sql() {
    let mut state = table_state();
    state.delete_target = Some("idx_users_email".to_owned());
    let mut rt = runtime(scene(state, false), MIN);
    rt.frame(0.0);
    press(&mut rt, "idx-delete-confirm");
    let actions = all_actions(&rt);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            IndexesAction::DropIndex { sql } if sql == "DROP INDEX \"idx_users_email\""
        )),
        "postgres drop SQL omits ON table; got {actions:?}"
    );
}

#[test]
fn safe_mode_never_emits_a_drop_command() {
    let mut scene = scene(table_state(), true);
    scene.identity.safe_mode = true;
    scene.vm.delete_target = Some("idx_users_email".to_owned());
    let mut rt = runtime(scene, MIN);
    rt.frame(0.0);
    assert!(!has_key(&rt, "idx-delete-index-idx_users_email"), "safe mode hides row trash");
    press(&mut rt, "idx-delete-confirm");
    assert!(
        !all_actions(&rt).iter().any(|a| matches!(a, IndexesAction::DropIndex { .. })),
        "safe mode leaves the confirm inert"
    );
}

#[test]
fn create_dialog_previews_and_emits_the_exact_sql() {
    let mut rt = runtime(scene(table_state(), true), DESIGN);
    rt.frame(0.0);
    press(&mut rt, "idx-new-index");
    assert!(has_key(&rt, CREATE_KEY), "the create dialog opens");
    assert_eq!(
        text_of(&rt, "idx-create-title"),
        t("common.newIndex"),
        "the create title uses common.newIndex"
    );
    press(&mut rt, "idx-col-toggle-email");
    assert!(
        rt.state.vm.selected_cols.iter().any(|c| c == "email"),
        "pressing a column row toggles it into the selection"
    );
    press(&mut rt, "idx-unique-toggle");
    assert!(rt.state.vm.is_unique, "pressing the unique row sets the flag");
    press(&mut rt, "idx-method-gin");
    assert_eq!(rt.state.vm.index_type, IndexMethod::Gin, "pressing a method selects it");
    assert_eq!(
        text_of(&rt, "idx-sql-text"),
        "CREATE UNIQUE INDEX \"idx_users_email\" ON \"users\" USING gin (\"email\")",
        "the preview mirrors getCreateIndexSql for postgresql"
    );
    assert_eq!(
        text_of(&rt, "idx-col-order-email"),
        "#1",
        "the selected column shows its selection order"
    );
    press(&mut rt, "idx-create-submit");
    let actions = all_actions(&rt);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            IndexesAction::CreateIndex { sql }
                if sql == "CREATE UNIQUE INDEX \"idx_users_email\" ON \"users\" USING gin (\"email\")"
        )),
        "submit hands the previewed SQL to the host; got {actions:?}"
    );
}

#[test]
fn empty_loading_and_error_branches_pin_their_keys() {
    let mut rt = runtime(scene(IndexesViewState::default(), true), MIN);
    rt.frame(0.0);
    assert!(has_key(&rt, EMPTY_KEY), "an empty index list shows the empty state");
    assert_eq!(
        text_of(&rt, "idx-empty-text"),
        t("indexes.noIndexes"),
        "the empty line uses indexes.noIndexes"
    );
    assert!(has_key(&rt, "idx-edit-in-structure-empty"), "the empty edit button is kept");
    assert!(has_key(&rt, "idx-create-first"), "the empty create button is kept");
    press(&mut rt, "idx-create-first");
    assert!(has_key(&rt, CREATE_KEY), "create-first opens the dialog too");

    let mut loading = IndexesViewState::default();
    loading.loading = true;
    let rt = runtime(scene(loading, false), MIN);
    assert!(has_key(&rt, LOADING_KEY), "the loading branch shows the spinner state");
    assert_eq!(text_of(&rt, "idx-loading-text"), t("indexes.loading"));

    let mut error = IndexesViewState::default();
    error.error = "disk gone".to_owned();
    let mut rt = runtime(scene(error, false), MIN);
    rt.frame(0.0);
    assert!(has_key(&rt, ERROR_KEY), "the error branch shows the copyable error");
    assert!(has_key(&rt, "idx-error-text"), "the error message node is composed");
    press(&mut rt, "idx-retry");
    assert!(
        all_actions(&rt).contains(&IndexesAction::Retry),
        "retry asks the host to reload"
    );
}

#[test]
fn the_edit_structure_port_navigates_with_the_table_name() {
    let mut rt = runtime(scene(table_state(), true), DESIGN);
    rt.frame(0.0);
    press(&mut rt, "idx-edit-in-structure");
    let navigated = rt.state.navigated.borrow().clone();
    assert_eq!(navigated, vec!["users".to_owned()], "edit-in-structure forwards tableName");
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut state = table_state();
            state.indexes.extend((0..8).map(|i| index(&format!("idx_extra_{i}"), &["id"], false, false, None)));
            let mut rt = runtime(scene(state, dark), (width, height));
            rt.frame(0.0);
            // Check the table first, then the create and delete overlays.
            for phase in 0..3 {
                if phase == 1 {
                    press(&mut rt, "idx-new-index");
                } else if phase == 2 {
                    press(&mut rt, "idx-create-cancel");
                    press(&mut rt, "idx-delete-index-idx_users_email");
                }
                for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, w, h] = element.rect;
                    assert!(
                        x >= -0.5 && x + w <= width + 0.5,
                        "{width}x{height} dark={dark} phase={phase}: {:?} {:?} overflows horizontally",
                        element.key,
                        element.rect
                    );
                    assert!(
                        y >= -0.5 && y + h <= height + 0.5,
                        "{width}x{height} dark={dark} phase={phase}: {:?} {:?} overflows vertically",
                        element.key,
                        element.rect
                    );
                }
            }
        }
    }
}

#[test]
fn no_host_port_keeps_the_view_read_only() {
    let mut scene = scene(table_state(), true);
    scene.no_port = true;
    let mut rt = runtime(scene, MIN);
    rt.frame(0.0);
    assert!(has_key(&rt, ROOT_KEY), "no-port composition still renders the table");
    assert!(has_key(&rt, "idx-new-index"), "the new-index button is still shown");
    let (x, y) = rt.find("idx-new-index").unwrap_or_else(|| panic!("new-index button exists"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert!(!has_key(&rt, CREATE_KEY), "without a callback the button is inert");
}
