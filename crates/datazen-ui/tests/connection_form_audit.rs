//! T103 independent audit: ConnectionFormBody + ConnectionImportFields.
//!
//! Asserts observable behaviour through `Runtime` (rendered element tree, keys,
//! rects and owner state), derived from the React sources rather than from the
//! port's own self-reports. Every probe here is independent of the coder's
//! `connection_form_parts.rs` suite.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::components::connection::connection_form_body::{ConnectionFormBody, FormBodyVm, FormVariant};
use datazen_ui::components::connection::connection_import_fields::{ConnectionImportFields, ImportVm};
use datazen_ui::i18n::t;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

struct Owner {
    dark: bool,
    form: FormBodyVm,
    import: ImportVm,
}

fn form_view(s: &Owner, _cx: &mut Cx<Owner>) -> El {
    let mut layer = Vec::new();
    let page = ConnectionFormBody::<Owner>::new(s.form.clone())
        .on_vm(|st: &mut Owner| &mut st.form)
        .on_action(|_st: &mut Owner, _a| {})
        .build(s.dark, &mut layer);
    rune_kit::root(page, layer)
}

fn import_view(s: &Owner, _cx: &mut Cx<Owner>) -> El {
    let page = ConnectionImportFields::<Owner>::new(s.import.clone())
        .on_vm(|st: &mut Owner| &mut st.import)
        .on_action(|_st: &mut Owner, _a| {})
        .build(s.dark);
    rune_kit::root(page, Vec::new())
}

fn form_rt(dark: bool, form: FormBodyVm, size: (f32, f32)) -> Runtime<Owner> {
    let mut rt = Runtime::new(
        Owner {
            dark,
            form,
            import: ImportVm::default(),
        },
        form_view,
        size,
        env(dark),
    );
    rt.frame(0.0);
    rt
}

fn import_rt(dark: bool, import: ImportVm, size: (f32, f32)) -> Runtime<Owner> {
    let mut rt = Runtime::new(
        Owner {
            dark,
            form: FormBodyVm::default(),
            import,
        },
        import_view,
        size,
        env(dark),
    );
    rt.frame(0.0);
    rt
}

fn elem<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.find(key).is_some()
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

// ---- D1: the index variant must carry React's IndexConnectionFields label ---------------------

/// React `IndexConnectionFields` renders `<Label>{t('newConn.databaseIndex')}</Label>` above the
/// numeric input. The port's `index_field` is passed straight into `standard_fields` (which
/// pushes a bare `input_field` with no `field_block`), so the label is dropped and the key
/// `newConn.databaseIndex` is never used anywhere in the crate.
#[test]
fn index_variant_shows_database_index_label() {
    let mut vm = FormBodyVm::default();
    vm.form_variant = FormVariant::Index;
    let rt = form_rt(true, vm, (960.0, 640.0));

    let rendered = texts(&rt);
    assert!(
        rendered.iter().any(|x| x == t("newConn.databaseIndex")),
        "index variant must show the newConn.databaseIndex label {:?}; rendered {rendered:?}",
        t("newConn.databaseIndex")
    );
    assert!(has(&rt, "new-conn-database-index"), "index input still mounted");
}

// ---- Coverage: branches the coder's suite does not exercise ------------------------------------

#[test]
fn has_username_false_hides_username_but_keeps_password() {
    let mut vm = FormBodyVm::default();
    vm.has_username = false;
    let rt = form_rt(true, vm, (960.0, 640.0));

    assert!(!has(&rt, "new-conn-username"), "username field hidden");
    assert!(has(&rt, "new-conn-password"), "password field still shown");
    assert!(has(&rt, "new-conn-host"), "standard fields still shown");
}

#[test]
fn show_db_type_select_false_hides_both_own_selects() {
    let rt = form_rt(true, FormBodyVm::default(), (960.0, 640.0));
    assert!(!has(&rt, "new-conn-db-type-field-trigger"), "db type select hidden by default");
    assert!(!has(&rt, "new-conn-group-field-trigger"), "group select hidden by default");
    assert!(!has(&rt, "new-conn-type-group-row"), "type/group row not mounted");
}

#[test]
fn file_flow_without_selected_file_hides_password_and_preview() {
    let mut vm = ImportVm::default();
    vm.app_import = false;
    vm.selected_file = None;
    let rt = import_rt(true, vm, (960.0, 640.0));

    assert!(has(&rt, "import-formats-hint"), "file flow still shows the format hint");
    assert!(!has(&rt, "import-data-path"), "no app data path in the file flow");
    assert!(!has(&rt, "import-selected-file"), "no chosen-file preview without a file");
    assert!(!has(&rt, "import-password-block"), "no password block without a file in file flow");
}

#[test]
fn inline_variant_keeps_all_sections_when_ready() {
    let mut vm = ImportVm::default();
    vm.inline = true;
    vm.path_found = true;
    let rt = import_rt(true, vm, (960.0, 640.0));

    for key in [
        "connection-import-fields",
        "import-data-path",
        "import-data-path-hint",
        "import-browse-folder",
        "import-browse-file",
        "import-password-input",
    ] {
        assert!(has(&rt, key), "inline variant keeps {key}");
    }
    assert_eq!(
        elem(&rt, "import-data-path-hint").text.as_deref(),
        Some(t("connShare.dataPathFoundHint"))
    );
}

// ---- Geometry channel independent of the coder's assertions ------------------------------------

/// Both components must leave every laid out element inside the window, in both themes and at
/// both required sizes, including the densest content states.
#[test]
fn audit_geometry_stays_inside_both_windows_both_themes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut form = FormBodyVm::default();
            form.show_db_type_select = true;
            form.host_error = t("newConn.required").to_owned();
            form.port_error = t("newConn.required").to_owned();
            form.test_err = "a very long connection error string for the minimum window".to_owned();
            form.advanced.show_advanced = true;
            let form_rt = form_rt(dark, form, (w, h));
            audit_inside(&form_rt, w, h, &format!("form dark={dark} {w}x{h}"));

            let mut import = ImportVm::default();
            import.local_error = "a long inline import failure banner that must not overflow".to_owned();
            import.path_found = true;
            let import_rt = import_rt(dark, import, (w, h));
            audit_inside(&import_rt, w, h, &format!("import dark={dark} {w}x{h}"));
        }
    }
}

fn audit_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.clone().unwrap_or_else(|| e.path.clone()), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}
