//! T371: `windows/data-sync/SqlPreview.tsx` port — the toolbar (title,
//! filter chips, conditional loading spinner, refresh, copy), the
//! statement projection into the text body, the error banner with its
//! size-limit special case, and geometry.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::{has, t};
use crate::views::data_sync::client_sql_preview::{DataSyncSqlStatement, OpFilter};
use crate::views::{DataSyncOperation, SqlPreview, SqlPreviewVm};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn statement(operation: DataSyncOperation, preview_sql: &str, sql: &str) -> DataSyncSqlStatement {
    DataSyncSqlStatement {
        table: "t".to_owned(),
        operation,
        sql: sql.to_owned(),
        preview_sql: preview_sql.to_owned(),
        parameters: Vec::new(),
        row_key: Vec::new(),
    }
}

#[derive(Default)]
struct Scene {
    vm: SqlPreviewVm,
    filters: Vec<OpFilter>,
    refreshes: usize,
    copied_texts: Vec<String>,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        SqlPreview::<Scene>::new(scene.vm.clone())
            .on_filter(|s, filter| s.filters.push(filter))
            .on_refresh(|s| s.refreshes += 1)
            .on_copy(|s, text| s.copied_texts.push(text))
            .build(scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_toolbar_mounts_every_control() {
    let rt = mount(Scene::default(), (960.0, 640.0));
    for key in [
        "data-sync-preview",
        "data-sync-preview-toolbar",
        "data-sync-preview-title",
        "data-sync-preview-filter-all",
        "data-sync-preview-filter-insert",
        "data-sync-preview-filter-update",
        "data-sync-preview-filter-delete",
        "data-sync-preview-refresh",
        "data-sync-preview-copy",
        "data-sync-preview-text",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "data-sync-preview-title"), t("common.sqlPreviewLower"));
    assert_eq!(label_of(&rt, "data-sync-preview-filter-all"), t("sync.filter.all"));
    assert_eq!(label_of(&rt, "data-sync-preview-filter-insert"), "INSERT");
    assert_eq!(label_of(&rt, "data-sync-preview-filter-update"), "UPDATE");
    assert_eq!(label_of(&rt, "data-sync-preview-filter-delete"), "DELETE");
    assert!(!has_key(&rt, "data-sync-preview-loading"));
    assert!(!has_key(&rt, "data-sync-preview-error"));
}

#[test]
fn a_loading_vm_mounts_the_spinner() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                loading: true,
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-preview-loading"));
}

#[test]
fn no_statements_renders_an_empty_body() {
    let rt = mount(Scene::default(), (960.0, 640.0));
    assert_eq!(text_of(&rt, "data-sync-preview-text"), "");
}

#[test]
fn statements_render_through_the_projection() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                statements: Some(vec![
                    statement(DataSyncOperation::Insert, "-- i1", "INSERT 1"),
                    statement(DataSyncOperation::Delete, "", "DELETE 1"),
                ]),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert_eq!(text_of(&rt, "data-sync-preview-text"), "-- i1\nDELETE 1");
}

#[test]
fn an_operation_filter_projects_only_its_own_statements() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                statements: Some(vec![
                    statement(DataSyncOperation::Insert, "-- i1", "INSERT 1"),
                    statement(DataSyncOperation::Delete, "", "DELETE 1"),
                ]),
                op_filter: OpFilter::Op(DataSyncOperation::Delete),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert_eq!(text_of(&rt, "data-sync-preview-text"), "DELETE 1");
}

#[test]
fn the_filter_chips_report_their_own_filter() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-preview-filter-insert");
    assert_eq!(rt.state.filters.len(), 1);
    assert_eq!(rt.state.filters[0], OpFilter::Op(DataSyncOperation::Insert));
    click(&mut rt, "data-sync-preview-filter-all");
    assert_eq!(rt.state.filters[1], OpFilter::All);
    click(&mut rt, "data-sync-preview-filter-update");
    assert_eq!(rt.state.filters[2], OpFilter::Op(DataSyncOperation::Update));
    click(&mut rt, "data-sync-preview-filter-delete");
    assert_eq!(rt.state.filters[3], OpFilter::Op(DataSyncOperation::Delete));
}

#[test]
fn refresh_and_copy_report_through_their_callbacks() {
    let mut rt = mount(
        Scene {
            vm: SqlPreviewVm {
                statements: Some(vec![statement(DataSyncOperation::Insert, "-- i1", "INSERT 1")]),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    click(&mut rt, "data-sync-preview-refresh");
    assert_eq!(rt.state.refreshes, 1);
    click(&mut rt, "data-sync-preview-copy");
    assert_eq!(rt.state.copied_texts.len(), 1);
    assert_eq!(rt.state.copied_texts[0], "-- i1", "copy carries the projected text");
}

#[test]
fn the_copy_label_swaps_to_copied_under_the_flag() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                copied: true,
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert_eq!(label_of(&rt, "data-sync-preview-copy"), t("common.copied"));
    let rt = mount(Scene::default(), (960.0, 640.0));
    assert_eq!(label_of(&rt, "data-sync-preview-copy"), t("common.copy"));
}

#[test]
fn a_plain_error_renders_verbatim() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                preview_error: "connection refused".to_owned(),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-preview-error"));
    assert_eq!(text_of(&rt, "data-sync-preview-error-text"), "connection refused");
}

#[test]
fn the_size_limit_error_swaps_in_its_own_message() {
    assert!(has("sync.sqlPreviewLimitReached"));
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                preview_error: "Data Sync SQL preview exceeds the 16 MiB IPC limit: 17 MiB"
                    .to_owned(),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert_eq!(
        text_of(&rt, "data-sync-preview-error-text"),
        t("sync.sqlPreviewLimitReached")
    );
}

#[test]
fn the_preview_fits_both_window_widths() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(
            Scene {
                vm: SqlPreviewVm {
                    statements: Some(vec![statement(
                        DataSyncOperation::Insert,
                        "-- a rather long preview line that has to wrap at the width",
                        "INSERT",
                    )]),
                    ..Default::default()
                },
                ..Scene::default()
            },
            (w, h),
        );
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync-preview") {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {outside:?}");
    }
}

#[test]
fn the_light_theme_mounts_the_same_toolbar() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                statements: Some(vec![statement(DataSyncOperation::Insert, "-- i1", "INSERT 1")]),
                ..Default::default()
            },
            dark: false,
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-preview-filter-all"));
    assert!(has_key(&rt, "data-sync-preview-copy"));
    assert_eq!(text_of(&rt, "data-sync-preview-text"), "-- i1");
}
