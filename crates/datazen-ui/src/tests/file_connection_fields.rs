//! T193: `components/connection/FileConnectionFields.tsx` ported to Rune.
//!
//! Probes mount the real field group and drive it through the injected host
//! port; every rendered assertion reads the runtime element tree. State-machine
//! assertions read the explicit `FileFieldsState` and its outputs, never a value
//! the component writes for the test to read.

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::components::connection::file_connection_fields::{
    AdbDatabaseFile, AdbPackage, FileConnectionFields, FileFieldsAction, FileFieldsEvent,
    FileFieldsOutput, FileFieldsPort, FileFieldsState,
};
use crate::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone, Default)]
struct Port {
    picks: usize,
    package_calls: usize,
    database_calls: Vec<String>,
    pulls: Vec<(String, String)>,
}

impl FileFieldsPort for Port {
    fn pick_path(&mut self) {
        self.picks += 1;
    }
    fn list_packages(&mut self) {
        self.package_calls += 1;
    }
    fn list_databases(&mut self, package: &str) {
        self.database_calls.push(package.to_owned());
    }
    fn pull_database(&mut self, package: &str, path: &str) {
        self.pulls.push((package.to_owned(), path.to_owned()));
    }
}

#[derive(Clone)]
struct Scene {
    fields: FileFieldsState,
    database: TextEdit,
    db_error: String,
    filter: TextEdit,
    port: Port,
    actions: Vec<FileFieldsAction>,
    dark: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            fields: FileFieldsState::default(),
            database: TextEdit::new(String::new()),
            db_error: String::new(),
            filter: TextEdit::new(String::new()),
            port: Port::default(),
            actions: Vec::new(),
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let fields = FileConnectionFields::<Scene>::new(&s.fields, &s.database, &s.db_error)
        .filter_edit(|st: &mut Scene| &mut st.filter)
        .database_edit(|st: &mut Scene| &mut st.database)
        .on_action(|st: &mut Scene, action| {
            st.actions.push(action.clone());
            st.fields.handle_action(action, &mut st.port);
        })
        .build(s.dark, &mut layer);
    col().w_full().h_full().child(fields).children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {:?}: {outside:?}", outside.len(), size);
}

fn packages() -> Vec<AdbPackage> {
    ["com.example.One", "org.example.Two"]
        .iter()
        .map(|name| AdbPackage { package_name: (*name).to_owned() })
        .collect()
}

// --- 1. path field / browse --------------------------------------------------

#[test]
fn path_field_keeps_t103_keys_and_browse_uses_the_host_port() {
    let mut scene = Scene::default();
    scene.db_error = "path is required".to_owned();
    scene.database = TextEdit::new("/tmp/app.db".to_owned());
    let mut rt = runtime(scene, DESIGN);

    assert!(has_key(&rt, "new-conn-db-file-path"));
    assert!(has_key(&rt, "new-conn-db-file-path-input"));
    assert!(has_key(&rt, "new-conn-db-file-path-browse"));
    assert_eq!(text_of(&rt, "new-conn-db-file-path-error"), "path is required");
    assert!(!has_key(&rt, "new-conn-adb-panel"), "ADB panel stays hidden until toggled");

    click(&mut rt, "new-conn-db-file-path-browse");
    assert!(rt.state.actions.contains(&FileFieldsAction::PickPath));
    assert_eq!(rt.state.port.picks, 1, "browse goes through the injected host port");
}

#[test]
fn path_picked_output_updates_the_form_database() {
    let mut fields = FileFieldsState::default();
    let out = fields.apply(FileFieldsEvent::PathPicked(Some("/picked.db".to_owned())));
    assert_eq!(out, vec![FileFieldsOutput::SetDatabase("/picked.db".to_owned())]);
    assert!(fields.apply(FileFieldsEvent::PathPicked(None)).is_empty(), "a cancelled pick is a no-op");
}

// --- 2. ADB mode / packages --------------------------------------------------

#[test]
fn toggling_adb_lists_packages_once_and_empty_answer_shows_the_exact_error() {
    let mut rt = runtime(Scene::default(), DESIGN);
    click(&mut rt, "new-conn-adb-mode");

    assert!(rt.state.fields.adb_mode);
    assert_eq!(rt.state.port.package_calls, 1, "first toggle loads packages through the host port");
    assert!(rt.state.fields.loading_packages);

    let mut scene = rt.state.clone();
    scene.fields.loading_packages = false;
    scene.fields.packages = Vec::new();
    let _ = scene.fields.apply(FileFieldsEvent::PackagesLoaded(Ok(Vec::new())));
    assert_eq!(scene.fields.error, t("newConn.adbNoPackages"));
    let shown = runtime(scene, DESIGN);
    assert!(has_key(&shown, "new-conn-adb-panel"));
    assert_eq!(text_of(&shown, "new-conn-adb-error"), t("newConn.adbNoPackages"));
}

#[test]
fn package_filter_soft_matches_case_insensitively() {
    let mut fields = FileFieldsState::default();
    fields.packages = packages();
    fields.package_filter = TextEdit::new("EXAMPLE".to_owned());
    let matched = fields.filtered_packages();
    assert_eq!(matched.len(), 2);
    fields.package_filter = TextEdit::new("one".to_owned());
    assert_eq!(fields.filtered_packages().len(), 1);
    assert_eq!(fields.filtered_packages()[0].package_name, "com.example.One");
}

#[test]
fn choosing_a_package_loads_its_databases_and_resets_the_previous_choice() {
    let mut fields = FileFieldsState::default();
    fields.selected_db_path = "/old.db".to_owned();
    fields.databases.push(AdbDatabaseFile { path: "/old.db".to_owned(), name: "old".to_owned() });
    let mut port = Port::default();

    fields.handle_action(FileFieldsAction::SelectPackage("com.example.One".to_owned()), &mut port);
    assert_eq!(port.database_calls, vec!["com.example.One"]);
    assert!(fields.selected_db_path.is_empty(), "a new package clears the old database choice");
    assert!(fields.databases.is_empty());
    assert!(fields.loading_databases);

    fields.handle_action(FileFieldsAction::SelectPackage(String::new()), &mut port);
    assert_eq!(port.database_calls.len(), 1, "an empty package asks for nothing");

    let dbs = vec![AdbDatabaseFile { path: "/data/app.db".to_owned(), name: "app".to_owned() }];
    let _ = fields.apply(FileFieldsEvent::DatabasesLoaded(Ok(dbs.clone())));
    assert_eq!(fields.databases, dbs);
    assert!(!fields.loading_databases);
    let mut fields = FileFieldsState::default();
    let _ = fields.apply(FileFieldsEvent::DatabasesLoaded(Ok(Vec::new())));
    assert_eq!(fields.error, t("newConn.adbNoDatabases"));
}

// --- 3. pull flow ------------------------------------------------------------

#[test]
fn pull_sets_the_saved_database_and_translates_its_failure() {
    let mut fields = FileFieldsState::default();
    fields.selected_package = "com.example.One".to_owned();
    fields.selected_db_path = "/data/app.db".to_owned();
    let mut port = Port::default();

    assert!(fields.can_pull());
    fields.handle_action(FileFieldsAction::Pull, &mut port);
    assert_eq!(port.pulls, vec![("com.example.One".to_owned(), "/data/app.db".to_owned())]);
    assert!(fields.pulling);

    let out = fields.apply(FileFieldsEvent::DatabasePulled(Ok(Some("/saved/app.db".to_owned()))));
    assert_eq!(out, vec![FileFieldsOutput::SetDatabase("/saved/app.db".to_owned())]);
    assert_eq!(fields.success, t("newConn.adbPullSuccess"));
    assert!(!fields.pulling);

    fields.handle_action(FileFieldsAction::Pull, &mut port);
    let out = fields.apply(FileFieldsEvent::DatabasePulled(Err("adb offline".to_owned())));
    assert!(out.is_empty());
    assert_eq!(fields.error, format!("{}: {}", t("newConn.adbPullFailed"), "adb offline"));
}

#[test]
fn pull_is_guarded_until_both_package_and_database_are_chosen() {
    let mut fields = FileFieldsState::default();
    let mut port = Port::default();
    fields.handle_action(FileFieldsAction::Pull, &mut port);
    fields.selected_package = "com.example.One".to_owned();
    fields.handle_action(FileFieldsAction::Pull, &mut port);
    assert!(port.pulls.is_empty(), "an incomplete selection never stages a pull");
}

#[test]
fn error_and_success_messages_render_with_their_exact_keys() {
    let mut scene = Scene::default();
    scene.fields.adb_mode = true;
    scene.fields.packages = packages();
    scene.fields.error = "boom".to_owned();
    scene.fields.success = t("newConn.adbPullSuccess").to_owned();
    let rt = runtime(scene, DESIGN);
    assert_eq!(text_of(&rt, "new-conn-adb-error"), "boom");
    assert_eq!(text_of(&rt, "new-conn-adb-success"), t("newConn.adbPullSuccess"));
    assert!(!has_key(&rt, "new-conn-adb-pull"), "no selection -> no pull button");
}

// --- 4. geometry -------------------------------------------------------------

#[test]
fn fields_and_adb_panel_fit_both_window_sizes_and_both_palettes() {
    let mut adb = Scene::default();
    adb.fields.adb_mode = true;
    adb.fields.packages = packages();
    adb.fields.selected_package = "com.example.One".to_owned();
    adb.fields.databases.push(AdbDatabaseFile { path: "/data/app.db".to_owned(), name: "app".to_owned() });
    adb.fields.selected_db_path = "/data/app.db".to_owned();
    let mut error = Scene::default();
    error.fields.adb_mode = true;
    error.fields.error = "a long adb failure message that wraps across the narrow field column".to_owned();

    for (name, scene) in [
        ("closed", Scene::default()),
        ("adb", adb),
        ("error", error),
    ] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(Scene { dark, ..scene.clone() }, size);
                assert!(has_key(&rt, "new-conn-file-fields"), "{name} {size:?} dark={dark}: root mounted");
                assert_inside(&rt, size, &format!("{name} {size:?} dark={dark}"));
            }
        }
    }
}
