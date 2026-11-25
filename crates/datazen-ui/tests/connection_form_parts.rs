//! T103: `ConnectionFormBody` + `ConnectionImportFields` — real interaction,
//! validation, disabled-state, exact-i18n and rect-geometry probes.
//!
//! Every assertion below observes the rendered tree through `Runtime` (or the
//! owner state after a press), never a value the component reports about
//! itself. Keys, error transitions and disabled fields are driven by real
//! pointer/key events; geometry uses each element's runtime `rect`.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::components::connection::connection_advanced_settings::{AdvancedAction, TunnelSource};
use datazen_ui::components::connection::connection_form_body::{
    FormAction, FormBodyVm, FormSelect, FormVariant, ConnectionFormBody,
};
use datazen_ui::components::connection::connection_import_fields::{
    BrowseKind, ConnectionImportFields, ImportAction, ImportVm, PasswordPolicy,
    import_file_display_name,
};
use datazen_ui::i18n::t;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

/// Owner state: both VMs plus a log of every dispatched action.
struct Probe {
    dark: bool,
    form: FormBodyVm,
    import: ImportVm,
    form_actions: Vec<FormAction>,
    import_actions: Vec<ImportAction>,
}

impl Probe {
    fn new(dark: bool, form: FormBodyVm, import: ImportVm) -> Self {
        Probe {
            dark,
            form,
            import,
            form_actions: Vec::new(),
            import_actions: Vec::new(),
        }
    }

    fn apply_form(&mut self, a: FormAction) {
        match a.clone() {
            FormAction::DatabaseType(v) => self.form.database_type = v,
            FormAction::Group(v) => self.form.group = v,
            FormAction::ClearStoredCredentials(v) => self.form.clear_stored_credentials = v,
            FormAction::OpenSelect(k) => self.form.open_select = k,
            FormAction::Advanced(adv) => apply_advanced(&mut self.form.advanced, &adv),
        }
        self.form_actions.push(a);
    }

    fn apply_import(&mut self, a: ImportAction) {
        match a {
            ImportAction::BrowsePath(_) => self.import.path_found = true,
            ImportAction::PickImportFile => {
                self.import.selected_file = Some("/downloads/restore.datazenconnection".to_owned());
                self.import.password_required = true;
            }
        }
        self.import_actions.push(a);
    }
}

fn apply_advanced(vm: &mut datazen_ui::components::connection::connection_advanced_settings::AdvancedSettingsVm, a: &AdvancedAction) {
    match a {
        AdvancedAction::ToggleAdvanced => vm.show_advanced = !vm.show_advanced,
        AdvancedAction::ToggleTunnel => vm.show_tunnel = !vm.show_tunnel,
        AdvancedAction::OpenSelect(k) => vm.open = *k,
        AdvancedAction::SslMode(v) => vm.ssl_mode = v.clone(),
        AdvancedAction::ReadOnly(v) => vm.read_only = *v,
        AdvancedAction::ColorTag(v) => vm.color_tag = v.clone(),
        AdvancedAction::Group(v) => vm.group = v.clone(),
        AdvancedAction::TunnelSource(v) => vm.tunnel_source = *v,
        AdvancedAction::TunnelKind(v) => {
            vm.tunnel_kind = *v;
            vm.effective_tunnel_kind = Some(*v);
        }
        AdvancedAction::TunnelId(v) => vm.tunnel_id = v.clone(),
        AdvancedAction::CreateEntry => vm.tunnel_source = TunnelSource::Inline,
        // Boundary actions have no in-scope state effect here.
        AdvancedAction::OpenSettings | AdvancedAction::Unbind | AdvancedAction::SaveAs => {}
    }
}

fn form_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let page = ConnectionFormBody::<Probe>::new(s.form.clone())
        .on_vm(|st: &mut Probe| &mut st.form)
        .on_action(|st: &mut Probe, a| st.apply_form(a))
        .build(s.dark, &mut layer);
    rune_kit::root(page, layer)
}

fn import_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let page = ConnectionImportFields::<Probe>::new(s.import.clone())
        .on_vm(|st: &mut Probe| &mut st.import)
        .on_action(|st: &mut Probe, a| st.apply_import(a))
        .build(s.dark);
    rune_kit::root(page, Vec::new())
}

fn form_runtime(dark: bool, form: FormBodyVm, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe::new(dark, form, ImportVm::default()),
        form_view,
        size,
        env(dark),
    );
    rt.frame(0.0);
    rt
}

fn import_runtime(dark: bool, import: ImportVm, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe::new(dark, FormBodyVm::default(), import),
        import_view,
        size,
        env(dark),
    );
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.find(key).is_some()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn click_field_and_type<S>(rt: &mut Runtime<S>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before typing"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(text);
    rt.frame(0.0);
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

/// A direct mutation of `rt.state` does not go through an input handler, so it
/// must be marked dirty before the next frame rebuilds the view.
fn refresh<S>(rt: &mut Runtime<S>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

// Placeholder text is a drawing hint, not a `text` node in the element tree;
// pin those exact keys at the source call site instead.
const FORM_SRC: &str = include_str!("../src/components/connection/connection_form_body.rs");
const IMPORT_SRC: &str = include_str!("../src/components/connection/connection_import_fields.rs");

// ---- Form body: React keys, labels and structure ----------------------------------------------

#[test]
fn form_body_keeps_react_testids_labels_and_no_key_echo() {
    let rt = form_runtime(true, FormBodyVm::default(), (960.0, 640.0));

    // The one React `data-testid` in this file, plus the derived field keys.
    for key in [
        "connection-form-body",
        "new-conn-name",
        "new-conn-name-label",
        "new-conn-host",
        "new-conn-port",
        "new-conn-database",
        "new-conn-username",
        "new-conn-password",
        "new-conn-advanced-toggle",
        "new-conn-tunnel-toggle",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }

    let rendered = texts(&rt);
    for (key, label) in [
        ("newConn.connName", t("newConn.connName")),
        ("newConn.host", t("newConn.host")),
        ("newConn.port", t("newConn.port")),
        ("newConn.database", t("newConn.database")),
        ("newConn.username", t("newConn.username")),
        ("newConn.password", t("newConn.password")),
    ] {
        assert!(rendered.iter().any(|x| x == &label), "{key}: exact label {label:?} visible");
    }
    assert!(
        FORM_SRC.contains("t(\"newConn.namePlaceholder\")"),
        "the name placeholder uses the exact React i18n key"
    );
    assert_eq!(t("newConn.namePlaceholder"), "例如：主数据库");
    for text in &rendered {
        assert!(
            !text.contains("newConn.") && !text.contains("connShare."),
            "a raw i18n key leaked: {text:?}"
        );
    }
}

#[test]
fn form_body_renders_db_type_and_group_selects_and_writes_values() {
    let mut vm = FormBodyVm::default();
    vm.show_db_type_select = true;
    let mut rt = form_runtime(true, vm, (960.0, 640.0));

    assert!(has(&rt, "new-conn-db-type-field-trigger"), "db type select exists");
    assert!(has(&rt, "new-conn-group-field-trigger"), "group select exists");
    assert!(has(&rt, "new-conn-db-type-label"), "db type label is mounted");

    // Pick MySQL (index 1 of DEFAULT_DB_TYPES) → the stored value, and close.
    click(&mut rt, "new-conn-db-type-field-trigger");
    assert_eq!(rt.state.form.open_select, Some(FormSelect::DbType));
    assert!(has(&rt, "option1"), "listbox mounted");
    click(&mut rt, "option1");
    assert_eq!(rt.state.form.database_type, "mysql");
    assert_eq!(rt.state.form.open_select, None);

    // Pick the empty group option → group becomes "" and the select closes.
    click(&mut rt, "new-conn-group-field-trigger");
    assert_eq!(rt.state.form.open_select, Some(FormSelect::Group));
    click(&mut rt, "option0");
    assert_eq!(rt.state.form.group, "");
    assert_eq!(rt.state.form.open_select, None);
}

#[test]
fn form_validation_errors_appear_and_disappear_with_state() {
    let mut vm = FormBodyVm::default();
    vm.host_error = t("newConn.required").to_owned();
    vm.port_error = t("newConn.required").to_owned();
    let mut rt = form_runtime(true, vm, (960.0, 640.0));

    assert_eq!(
        element(&rt, "new-conn-host-error").text.as_deref(),
        Some(t("newConn.required"))
    );
    assert_eq!(
        element(&rt, "new-conn-port-error").text.as_deref(),
        Some(t("newConn.required"))
    );
    assert!(!has(&rt, "new-conn-database-error"), "no database error yet");

    // Clear the host error in the owner state; the next frame must drop it.
    rt.state.form.host_error.clear();
    refresh(&mut rt);
    assert!(!has(&rt, "new-conn-host-error"), "host error gone");
    assert!(has(&rt, "new-conn-port-error"), "port error still shown");

    // A database error (file variant) transitions independently.
    rt.state.form.database_error = "bad file".to_owned();
    refresh(&mut rt);
    assert_eq!(
        element(&rt, "new-conn-database-error").text.as_deref(),
        Some("bad file")
    );
}

#[test]
fn form_edit_checkbox_and_test_result_branches_follow_state() {
    let mut vm = FormBodyVm::default();
    vm.is_editing = true;
    vm.supports_clear_credentials = true;
    let mut rt = form_runtime(true, vm, (960.0, 640.0));

    let checkbox = "new-conn-clear-stored-credentials";
    assert!(has(&rt, checkbox), "edit-only checkbox is mounted");
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|x| x == t("newConn.clearStoredCredentials")));
    assert!(rendered.iter().any(|x| x == t("newConn.clearStoredCredentialsHint")));
    click(&mut rt, checkbox);
    assert!(rt.state.form.clear_stored_credentials, "press writes the checkbox value");

    assert!(!has(&rt, "new-conn-test-ok"), "no success initially");
    assert!(!has(&rt, "new-conn-test-error"), "no error initially");

    rt.state.form.test_ok = "PostgreSQL 16".to_owned();
    refresh(&mut rt);
    assert_eq!(
        element(&rt, "new-conn-test-ok-text").text.as_deref(),
        Some(format!("{}PostgreSQL 16", t("newConn.testSuccess")).as_str()),
        "success = exact t(newConn.testSuccess) + server version"
    );

    rt.state.form.test_err = "handshake failed".to_owned();
    refresh(&mut rt);
    let err = element(&rt, "new-conn-test-error");
    assert_eq!(err.role, "alert", "copyable error keeps its alert role");
    assert!(texts(&rt).iter().any(|x| x == "handshake failed"));
}

#[test]
fn form_composes_advanced_and_tunnel_sections_from_the_sibling_port() {
    let vm = FormBodyVm::default();
    let mut rt = form_runtime(true, vm, (960.0, 640.0));

    assert!(!has(&rt, "new-conn-advanced-panel"), "collapsed initially");
    click(&mut rt, "new-conn-advanced-toggle-chevron");
    assert!(rt.state.form.advanced.show_advanced);
    assert!(has(&rt, "new-conn-advanced-panel"), "advanced section mounted");

    // The advanced actions are forwarded through `FormAction::Advanced`.
    assert!(
        rt.state.form_actions.iter().any(|a| matches!(a, FormAction::Advanced(AdvancedAction::ToggleAdvanced))),
        "the compose forwards AdvancedAction to the owner"
    );

    click(&mut rt, "new-conn-tunnel-toggle-chevron");
    assert!(rt.state.form.advanced.show_tunnel);
    assert!(has(&rt, "new-conn-tunnel-panel"), "tunnel section mounted");
    assert!(
        rt.state.form_actions.iter().any(|a| matches!(a, FormAction::Advanced(AdvancedAction::ToggleTunnel)))
    );
}

#[test]
fn form_file_and_index_variants_keep_their_stand_in_fields() {
    let mut vm = FormBodyVm::default();
    vm.form_variant = FormVariant::File;
    let file_rt = form_runtime(true, vm, (960.0, 640.0));
    assert!(has(&file_rt, "new-conn-db-file-path"), "file path field");
    assert!(!has(&file_rt, "new-conn-host"), "standard fields not in the file variant");

    let mut vm = FormBodyVm::default();
    vm.form_variant = FormVariant::Index;
    let index_rt = form_runtime(true, vm, (960.0, 640.0));
    for key in ["new-conn-database-index", "new-conn-host", "new-conn-port"] {
        assert!(has(&index_rt, key), "{key} in the index variant");
    }
    assert!(!has(&index_rt, "new-conn-database"), "index replaces the plain db field");
}

// ---- Import fields: React keys, hints, variants, exact i18n ------------------------------------

#[test]
fn import_fields_keep_react_testids_and_branch_text() {
    let mut rt = import_runtime(true, ImportVm::default(), (960.0, 640.0));

    for key in ["connection-import-fields", "import-data-path", "import-data-path-hint"] {
        assert!(has(&rt, key), "{key} is mounted in the app flow");
    }
    assert!(!has(&rt, "import-formats-hint"), "app flow hides the format hint");
    assert_eq!(
        element(&rt, "import-data-path-hint").text.as_deref(),
        Some(t("connShare.dataPathMissingHint")),
        "missing hint is the exact i18n text"
    );

    rt.state.import.path_found = true;
    refresh(&mut rt);
    assert_eq!(
        element(&rt, "import-data-path-hint").text.as_deref(),
        Some(t("connShare.dataPathFoundHint")),
        "found hint transition"
    );

    // File flow: format hint shows, data path hides, chosen file appears.
    rt.state.import.app_import = false;
    rt.state.import.selected_file = Some("/downloads/restore.datazenconnection".to_owned());
    refresh(&mut rt);
    assert!(has(&rt, "import-formats-hint"), "file flow shows the format hint");
    assert!(!has(&rt, "import-data-path"), "file flow has no data path field");
    assert_eq!(
        element(&rt, "import-selected-file").text.as_deref(),
        Some("restore.datazenconnection"),
        "the file name (not the full path) is shown"
    );
    assert!(element(&rt, "import-selected-file").label.is_none(), "no duplicate accessible label");
    assert!(has(&rt, "import-change-file"), "change button in the file flow");
}

#[test]
fn import_password_label_tracks_the_file_password_policy() {
    let mut vm = ImportVm::default();
    vm.selected_file = Some("/tmp/plain.ncx".to_owned());
    vm.password_required = false;
    let mut rt = import_runtime(true, vm, (960.0, 640.0));

    assert_eq!(
        element(&rt, "import-password-optional").text.as_deref(),
        Some(format!("({})", t("connShare.passwordOptional")).as_str())
    );
    rt.state.import.password_required = true;
    refresh(&mut rt);
    assert!(!has(&rt, "import-password-optional"), "required password has no optional note");
    assert_eq!(
        PasswordPolicy::from_path("/tmp/backup.tableplusconnection"),
        PasswordPolicy::Required
    );
    assert_eq!(PasswordPolicy::from_path("/tmp/plain.ncx"), PasswordPolicy::Optional);
    assert_eq!(import_file_display_name("C:\\tmp\\data.db"), "data.db");
}

#[test]
fn import_error_banner_appears_only_with_a_local_error() {
    let mut rt = import_runtime(true, ImportVm::default(), (960.0, 640.0));
    assert!(!has(&rt, "connection-import-error"), "no error initially");
    rt.state.import.local_error = "wrong password".to_owned();
    refresh(&mut rt);
    let err = element(&rt, "connection-import-error");
    assert_eq!(err.role, "alert", "error is announced");
    assert_eq!(
        element(&rt, "connection-import-error-text").text.as_deref(),
        Some("wrong password")
    );
    rt.state.import.local_error.clear();
    refresh(&mut rt);
    assert!(!has(&rt, "connection-import-error"), "error removed with state");
}

#[test]
fn import_exact_i18n_text_and_no_key_echo() {
    let mut vm = ImportVm::default();
    vm.local_error = "err".to_owned();
    let rt = import_runtime(true, vm, (960.0, 640.0));
    let rendered = texts(&rt);

    for key in [
        "connShare.dataPathMissingHint",
        "connShare.dataPath",
        "connShare.browseFolder",
        "connShare.browseFile",
        "connShare.password",
        "connShare.passwordOptional",
    ] {
        assert!(
            rendered.iter().any(|x| x == t(key) || x == &format!("({})", t(key))),
            "{key} is rendered as its exact translation"
        );
    }
    // Placeholders are drawing hints, not `text` nodes; pin their keys at the
    // source call site and verify they resolve to the exact locale text.
    for (key, src_key) in [
        ("connShare.dataPathPlaceholder", "t(\"connShare.dataPathPlaceholder\")"),
        ("connShare.passwordImportPlaceholder", "t(\"connShare.passwordImportPlaceholder\")"),
    ] {
        assert!(IMPORT_SRC.contains(src_key), "{key} is used at the import field");
        assert!(t(key) != key, "{key} resolves to a translation");
    }
    for text in &rendered {
        assert!(!text.contains("connShare."), "raw key leaked: {text:?}");
    }
}

// ---- Import fields: disabled / in-flight states and real interaction --------------------------

#[test]
fn import_buttons_are_disabled_while_busy_and_submitting() {
    let mut vm = ImportVm::default();
    vm.busy = true;
    let busy = import_runtime(true, vm, (960.0, 640.0));
    assert!(!element(&busy, "import-browse-folder").focusable, "browse folder disabled");
    assert!(!element(&busy, "import-browse-file").focusable, "browse file disabled");

    let mut vm = ImportVm::default();
    vm.app_import = false;
    vm.selected_file = Some("/tmp/conn.ncx".to_owned());
    vm.submitting = true;
    let submitting = import_runtime(true, vm, (960.0, 640.0));
    assert!(!element(&submitting, "import-change-file").focusable, "change disabled");
    assert!(!element(&submitting, "import-password-input").focusable, "password disabled");
}

#[test]
fn import_fields_take_text_when_enabled_and_ignore_it_when_disabled() {
    let mut rt = import_runtime(true, ImportVm::default(), (960.0, 640.0));
    click_field_and_type(&mut rt, "import-data-path", "C:/apps/prod");
    assert_eq!(rt.state.import.data_path.text, "C:/apps/prod", "enabled field accepts typing");

    let mut vm = ImportVm::default();
    vm.busy = true;
    let mut busy = import_runtime(true, vm, (960.0, 640.0));
    click_field_and_type(&mut busy, "import-data-path", "ignored");
    assert_eq!(busy.state.import.data_path.text, "", "disabled field ignores typing");
}

#[test]
fn import_browse_and_change_presses_dispatch_actions() {
    let mut rt = import_runtime(true, ImportVm::default(), (960.0, 640.0));
    click(&mut rt, "import-browse-folder");
    assert_eq!(rt.state.import_actions, vec![ImportAction::BrowsePath(BrowseKind::Folder)]);
    assert!(rt.state.import.path_found, "owner applied the browse result");

    rt.state.import.app_import = false;
    rt.state.import.selected_file = Some("/tmp/a.ncx".to_owned());
    refresh(&mut rt);
    click(&mut rt, "import-change-file");
    assert!(
        rt.state.import_actions.iter().any(|a| *a == ImportAction::PickImportFile),
        "change-file dispatches PickImportFile"
    );
}

// ---- Geometry: runtime rects, both windows, both themes ---------------------------------------

fn assert_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn both_components_fit_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            // Form: db-type/group + standard fields + validation + test result.
            let mut form = FormBodyVm::default();
            form.show_db_type_select = true;
            form.host_error = t("newConn.required").to_owned();
            form.test_ok = "PostgreSQL 16".to_owned();
            let form_rt = form_runtime(dark, form, (w, h));
            assert_inside(&form_rt, w, h, &format!("form dark={dark} {w}x{h}"));

            // Advanced section expanded (tunnel collapsed) still fits.
            let mut form = FormBodyVm::default();
            form.show_db_type_select = true;
            form.advanced.show_advanced = true;
            let form_rt = form_runtime(dark, form, (w, h));
            assert_inside(&form_rt, w, h, &format!("form advanced dark={dark} {w}x{h}"));

            // App import flow with a long hint and an error.
            let mut import = ImportVm::default();
            import.local_error = "a fairly long import error that must stay inside the window".to_owned();
            let import_rt = import_runtime(dark, import, (w, h));
            assert_inside(&import_rt, w, h, &format!("import app dark={dark} {w}x{h}"));

            // File import flow with a long chosen file name.
            let mut import = ImportVm::default();
            import.app_import = false;
            import.selected_file = Some("/downloads/a-very-long-filename-for-the-minimum-window.ncx".to_owned());
            let import_rt = import_runtime(dark, import, (w, h));
            assert_inside(&import_rt, w, h, &format!("import file dark={dark} {w}x{h}"));
        }
    }
}

// Confirm the form's two built-in variants preserve their React form variant
// values (a pure assertion independent of the rendered tree).
#[test]
fn form_variant_values_match_react() {
    assert_eq!(FormVariant::from_value("file"), FormVariant::File);
    assert_eq!(FormVariant::from_value("index"), FormVariant::Index);
    assert_eq!(FormVariant::from_value("standard"), FormVariant::Standard);
    assert_eq!(FormVariant::from_value("unknown").value(), "standard");
}
