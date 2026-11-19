//! T193 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped tests do not pin: a filter with no
//! match, host failures on the package/database loads, a cancelled pull, the
//! exact i18n keys, and an independent `visible`-rect geometry observation with
//! long error text and many packages at both sizes and palettes.

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::components::connection::file_connection_fields::{
    AdbPackage, FileConnectionFields, FileFieldsEvent, FileFieldsState,
};
use crate::i18n::{has, t};

fn packages() -> Vec<AdbPackage> {
    ["com.example.One", "org.example.Two"]
        .iter()
        .map(|name| AdbPackage { package_name: (*name).to_owned() })
        .collect()
}

#[test]
fn a_filter_with_no_match_yields_none_and_clearing_restores_all() {
    let mut fields = FileFieldsState::default();
    fields.packages = packages();
    fields.package_filter = TextEdit::new("NO-SUCH-PACKAGE".to_owned());
    assert!(fields.filtered_packages().is_empty(), "a soft filter hides non-matches");
    fields.package_filter = TextEdit::new(String::new());
    assert_eq!(fields.filtered_packages().len(), 2, "clearing the filter restores all");
}

#[test]
fn failed_package_and_database_loads_project_the_raw_message_and_clear_loading() {
    let mut fields = FileFieldsState::default();
    fields.loading_packages = true;
    assert!(fields.apply(FileFieldsEvent::PackagesLoaded(Err("adb offline".to_owned()))).is_empty());
    assert!(!fields.loading_packages, "a failed package load clears its spinner");
    assert_eq!(fields.error, "adb offline");

    fields.loading_databases = true;
    assert!(fields.apply(FileFieldsEvent::DatabasesLoaded(Err("device gone".to_owned()))).is_empty());
    assert!(!fields.loading_databases, "a failed database load clears its spinner");
    assert_eq!(fields.error, "device gone");
}

#[test]
fn a_cancelled_pull_is_a_noop_and_keeps_the_previous_success() {
    let mut fields = FileFieldsState::default();
    fields.pulling = true;
    fields.success = t("newConn.adbPullSuccess").to_owned();
    let out = fields.apply(FileFieldsEvent::DatabasePulled(Ok(None)));
    assert!(out.is_empty(), "a cancelled save does not set the database");
    assert!(!fields.pulling, "the spinner clears even on cancellation");
    assert_eq!(fields.success, t("newConn.adbPullSuccess"), "a cancelled pull is not a failure");
}

#[test]
fn every_i18n_key_the_field_group_uses_resolves_without_echoing() {
    for key in [
        "newConn.dbFilePath",
        "newConn.adbMode",
        "newConn.adbSelectPackage",
        "newConn.adbSearchPackage",
        "newConn.adbLoadingPackages",
        "newConn.adbSelectDatabase",
        "newConn.adbLoadingDatabases",
        "newConn.adbPull",
        "newConn.adbPulling",
        "newConn.adbPullSuccess",
        "newConn.adbPullFailed",
        "newConn.adbNoPackages",
        "newConn.adbNoDatabases",
    ] {
        assert!(has(key) && t(key) != key, "key {key} must resolve, not echo");
    }
}

#[derive(Clone)]
struct AuditScene {
    fields: FileFieldsState,
    database: TextEdit,
    db_error: String,
    filter: TextEdit,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self {
            fields: FileFieldsState::default(),
            database: TextEdit::new(String::new()),
            db_error: String::new(),
            filter: TextEdit::new(String::new()),
            dark: true,
        }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let fields = FileConnectionFields::<AuditScene>::new(&s.fields, &s.database, &s.db_error)
        .filter_edit(|st: &mut AuditScene| &mut st.filter)
        .database_edit(|st: &mut AuditScene| &mut st.database)
        .build(s.dark, &mut layer);
    col().w_full().h_full().child(fields).children(layer)
}

#[test]
fn long_errors_and_many_packages_stay_inside_the_window_via_visible_rects() {
    let mut scene = AuditScene::default();
    scene.fields.adb_mode = true;
    scene.fields.packages = (0..40)
        .map(|i| AdbPackage { package_name: format!("com.example.package.with.a.very.long.name.{i}") })
        .collect();
    scene.fields.selected_package = "com.example.package.with.a.very.long.name.0".to_owned();
    scene.fields.error = "x".repeat(500);
    scene.db_error = "database path is invalid and this message is deliberately very long".to_owned();

    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = Runtime::new(
                AuditScene { dark, ..scene.clone() },
                view,
                size,
                Env { system_dark: dark, scale: 1.0 },
            );
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path),
                    e.visible,
                    e.rect
                );
            }
        }
    }
}
