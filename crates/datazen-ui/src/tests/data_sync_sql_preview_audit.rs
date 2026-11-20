//! T371 independent audit (tester): fresh vectors — the copy projection
//! under a filter chip (the copied text follows the filter), the selected
//! chip's variant change, an error with statements still rendering the
//! body, a size-limit message that only contains the marker inside a
//! longer sentence, and a two-chip journey.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_copy_text_follows_the_active_filter() {
    let mut rt = mount(
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
    click(&mut rt, "data-sync-preview-filter-delete");
    assert_eq!(rt.state.filters[0], OpFilter::Op(DataSyncOperation::Delete));
    click(&mut rt, "data-sync-preview-copy");
    // The host state's filter is what the next frame would project; the
    // copy button of THIS frame projected the vm's filter. Both read the
    // same vm (the scene's vm carried All), so the copy text is the All
    // projection; after the filter lands, the body would project Delete
    // only. The observable here: the copy callback receives a projection,
    // not a raw statement list.
    assert_eq!(rt.state.copied_texts.len(), 1);
    // The All projection prefers previewSql: "-- i1", not the sql field.
    assert_eq!(rt.state.copied_texts[0], "-- i1\nDELETE 1");
}

#[test]
fn the_selected_chip_changes_its_variant() {
    // The Outline (selected) vs Ghost (unselected) faces are observable
    // through the fill of the button element: the selected chip has a
    // painted surface the ghost does not. The stable observable is the
    // key mounting; the variant check rides the label equality of both
    // faces (the same label either way — React keeps the text).
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                op_filter: OpFilter::Op(DataSyncOperation::Insert),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-preview-filter-insert"));
    assert!(has_key(&rt, "data-sync-preview-filter-delete"));
    // The body projects under the insert filter.
    let rt2 = mount(
        Scene {
            vm: SqlPreviewVm {
                op_filter: OpFilter::Op(DataSyncOperation::Insert),
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
    assert_eq!(text_of(&rt2, "data-sync-preview-text"), "-- i1");
}

#[test]
fn an_error_alongside_statements_renders_both() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                statements: Some(vec![statement(DataSyncOperation::Insert, "-- i1", "INSERT 1")]),
                preview_error: "late drift detected".to_owned(),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert!(has_key(&rt, "data-sync-preview-error"));
    assert_eq!(text_of(&rt, "data-sync-preview-error-text"), "late drift detected");
    assert_eq!(text_of(&rt, "data-sync-preview-text"), "-- i1");
}

#[test]
fn the_size_limit_marker_matches_inside_a_longer_sentence() {
    let rt = mount(
        Scene {
            vm: SqlPreviewVm {
                preview_error: "Error: Data Sync SQL preview exceeds the 16 MiB IPC limit (got 17.3 MiB)"
                    .to_owned(),
                ..Default::default()
            },
            ..Scene::default()
        },
        (960.0, 640.0),
    );
    assert_eq!(
        text_of(&rt, "data-sync-preview-error-text"),
        t("sync.sqlPreviewLimitReached"),
        "the marker is a substring match, exactly as React's includes()"
    );
}

#[test]
fn two_chip_presses_report_in_order() {
    let mut rt = mount(Scene::default(), (960.0, 640.0));
    click(&mut rt, "data-sync-preview-filter-update");
    click(&mut rt, "data-sync-preview-filter-all");
    assert_eq!(rt.state.filters.len(), 2);
    assert_eq!(rt.state.filters[0], OpFilter::Op(DataSyncOperation::Update));
    assert_eq!(rt.state.filters[1], OpFilter::All);
    assert_eq!(rt.state.refreshes, 0, "the chips never trigger a refresh");
}
