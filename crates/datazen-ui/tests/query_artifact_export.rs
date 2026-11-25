//! T057: `components/query/QueryArtifactExport.tsx` port — dedup / visibility
//! / labels, the busy-saved-error state machine, and the mounted bar
//! (buttons, partial note, status lines, in-flight guard, window fit).

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::query::query_artifact_export::{
    button_label, dedup_artifact_ids, export_error_message, should_render, ArtifactExportReceipt,
    ArtifactExportState, QueryArtifactExport, QueryResultArtifacts, ResultCompleteness,
};
use datazen_ui::i18n::{has, t};

const PREFIX: &str = "query-artifact-export";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn artifacts(ids: &[&str], completeness: ResultCompleteness) -> QueryResultArtifacts {
    QueryResultArtifacts {
        execution_id: "execution-1".to_string(),
        artifact_ids: ids.iter().map(|id| (*id).to_string()).collect(),
        result_completeness: completeness,
    }
}

#[derive(Default)]
struct Scene {
    artifacts: QueryResultArtifacts,
    state: ArtifactExportState,
    exports: Vec<String>,
    dark: bool,
}

impl Scene {
    fn with(artifacts: QueryResultArtifacts) -> Self {
        Self {
            artifacts,
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        QueryArtifactExport::<Scene>::new(
            scene.artifacts.clone(),
            |s: &Scene| &s.state,
            |s: &mut Scene| s.state.begin_export(),
        )
        .on_export(|s: &mut Scene, id| s.exports.push(id))
        .build(scene, scene.dark),
    )
}

fn view_no_export(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        QueryArtifactExport::<Scene>::new(
            scene.artifacts.clone(),
            |s: &Scene| &s.state,
            |s: &mut Scene| s.state.begin_export(),
        )
        .build(scene, scene.dark),
    )
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        QueryArtifactExport::<Scene>::new(
            scene.artifacts.clone(),
            |s: &Scene| &s.state,
            |s: &mut Scene| s.state.begin_export(),
        )
        .key("custom-export-bar")
        .on_export(|s: &mut Scene, id| s.exports.push(id))
        .build(scene, scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn mount_with(
    scene: Scene,
    size: (f32, f32),
    view: fn(&Scene, &mut Cx<Scene>) -> El,
) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn own_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(PREFIX))
        .collect()
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else {
            continue;
        };
        if !key.starts_with(PREFIX) {
            continue;
        }
        let [x, _y, ew, _eh] = e.rect;
        if x < -0.5 || x + ew > w + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- pure helpers: dedup, visibility, labels, completeness ----

#[test]
fn dedup_keeps_first_seen_order_and_drops_repeats() {
    assert_eq!(
        dedup_artifact_ids(&artifacts(
            &["b", "a", "b", "c", "a"],
            ResultCompleteness::Complete
        )),
        vec!["b".to_string(), "a".to_string(), "c".to_string()]
    );
    assert!(dedup_artifact_ids(&artifacts(&[], ResultCompleteness::Complete)).is_empty());
}

#[test]
fn should_render_is_false_while_pending_or_with_no_ids() {
    // React's early return: pending renders null even with ids, and an empty
    // (or all-duplicate-collapsed) list renders null even when complete.
    assert!(!should_render(&artifacts(
        &["artifact-1"],
        ResultCompleteness::Pending
    )));
    assert!(!should_render(&artifacts(
        &[],
        ResultCompleteness::Complete
    )));
    assert!(!should_render(&artifacts(
        &[],
        ResultCompleteness::Truncated
    )));
    assert!(should_render(&artifacts(
        &["artifact-1"],
        ResultCompleteness::Complete
    )));
    assert!(should_render(&artifacts(
        &["artifact-1"],
        ResultCompleteness::Truncated
    )));
}

#[test]
fn completeness_round_trips_its_wire_spelling() {
    for (value, spelling) in [
        (ResultCompleteness::Pending, "pending"),
        (ResultCompleteness::Complete, "complete"),
        (ResultCompleteness::Truncated, "truncated"),
    ] {
        assert_eq!(value.as_str(), spelling);
        assert_eq!(ResultCompleteness::from_str(spelling), Some(value));
    }
    assert_eq!(ResultCompleteness::from_str("done"), None);
    assert_eq!(ResultCompleteness::from_str(""), None);
}

#[test]
fn button_label_is_plain_for_one_and_numbered_for_many() {
    assert_eq!(button_label(1, 0), t("query.artifactExport.action"));
    assert_eq!(
        button_label(3, 0),
        t("query.artifactExport.numberedAction").replacen("{index}", "1", 1)
    );
    assert_eq!(
        button_label(3, 1),
        t("query.artifactExport.numberedAction").replacen("{index}", "2", 1)
    );
    assert_eq!(
        button_label(2, 1),
        t("query.artifactExport.numberedAction").replacen("{index}", "2", 1)
    );
}

#[test]
fn every_artifact_export_key_resolves() {
    for key in [
        "query.artifactExport.action",
        "query.artifactExport.numberedAction",
        "query.artifactExport.partial",
        "query.artifactExport.saving",
        "query.artifactExport.saved",
        "query.artifactExport.failed",
    ] {
        assert!(has(key), "{key} is missing from the locale packs");
        assert_ne!(t(key), key, "{key} echoes instead of translating");
    }
}

// ---- state machine: the in-flight guard and the three outcomes ----

#[test]
fn begin_export_guards_reentry_and_clears_the_previous_outcome() {
    let mut state = ArtifactExportState {
        saved: true,
        error: Some("old failure".to_string()),
        ..ArtifactExportState::default()
    };
    assert!(state.begin_export());
    assert!(state.busy);
    assert!(!state.saved);
    assert_eq!(state.error, None);
    // React's `if (inFlight.current) return`: the second entry changes nothing.
    assert!(!state.begin_export());
    assert!(state.busy);
}

#[test]
fn a_saved_receipt_marks_saved_and_clears_busy() {
    let mut state = ArtifactExportState::default();
    assert!(state.begin_export());
    state.finish_export(&ArtifactExportReceipt::Saved {
        artifact_id: "artifact-1".to_string(),
        byte_length: 100,
    });
    assert!(!state.busy);
    assert!(state.saved);
    assert_eq!(state.error, None);
}

#[test]
fn a_cancelled_receipt_is_a_normal_exit_with_no_mark() {
    // React: cancelled native dialogs leave no saved mark and no error, and
    // the bar stays usable for a retry.
    let mut state = ArtifactExportState::default();
    assert!(state.begin_export());
    state.finish_export(&ArtifactExportReceipt::Cancelled);
    assert!(!state.busy);
    assert!(!state.saved);
    assert_eq!(state.error, None);
    assert!(state.begin_export());
}

#[test]
fn a_rejection_shows_the_message_and_leaves_a_retry() {
    let mut state = ArtifactExportState::default();
    assert!(state.begin_export());
    state.fail_export(export_error_message(Some("Synthetic export failure")));
    assert!(!state.busy);
    assert!(!state.saved);
    assert_eq!(state.error.as_deref(), Some("Synthetic export failure"));
    // The retry clears the error, exactly as React's setError(null) on entry.
    assert!(state.begin_export());
    assert_eq!(state.error, None);
    state.finish_export(&ArtifactExportReceipt::Saved {
        artifact_id: "artifact-1".to_string(),
        byte_length: 100,
    });
    assert!(state.saved);
}

#[test]
fn export_error_message_falls_back_when_the_failure_carries_none() {
    assert_eq!(
        export_error_message(Some("boom")),
        "boom",
        "an Error's message passes through verbatim"
    );
    assert_eq!(
        export_error_message(None),
        t("query.artifactExport.failed"),
        "a non-Error rejection shows the fallback key's text"
    );
}

// ---- mounted bar: visibility, buttons, status lines ----

#[test]
fn pending_artifacts_render_nothing() {
    let rt = mount(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Pending)),
        (800.0, 600.0),
    );
    assert!(!has_key(&rt, PREFIX));
    assert!(!has_key(&rt, &format!("{PREFIX}-button-0")));
}

#[test]
fn an_empty_id_list_renders_nothing() {
    let rt = mount(
        Scene::with(artifacts(&[], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    assert!(!has_key(&rt, PREFIX));
}

#[test]
fn a_single_artifact_gets_one_plain_labelled_button() {
    let rt = mount(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    assert!(has_key(&rt, PREFIX));
    assert!(has_key(&rt, &format!("{PREFIX}-button-0")));
    assert!(!has_key(&rt, &format!("{PREFIX}-button-1")));
    assert!(has_key(&rt, &format!("{PREFIX}-button-0-icon")));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-button-0-label")),
        t("query.artifactExport.action")
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-partial")));
    assert!(!has_key(&rt, &format!("{PREFIX}-saving")));
    assert!(!has_key(&rt, &format!("{PREFIX}-saved")));
    assert!(!has_key(&rt, &format!("{PREFIX}-error")));
}

#[test]
fn duplicate_ids_collapse_to_numbered_buttons() {
    let rt = mount(
        Scene::with(artifacts(&["a", "b", "a"], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    // Two buttons, not three: the repeated id gets one button.
    assert!(has_key(&rt, &format!("{PREFIX}-button-0")));
    assert!(has_key(&rt, &format!("{PREFIX}-button-1")));
    assert!(!has_key(&rt, &format!("{PREFIX}-button-2")));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-button-0-label")),
        t("query.artifactExport.numberedAction").replacen("{index}", "1", 1)
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-button-1-label")),
        t("query.artifactExport.numberedAction").replacen("{index}", "2", 1)
    );
}

#[test]
fn truncated_results_show_the_partial_note() {
    let rt = mount(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Truncated)),
        (800.0, 600.0),
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-partial")),
        t("query.artifactExport.partial")
    );
}

#[test]
fn clicking_a_button_exports_only_that_id() {
    let mut rt = mount(
        Scene::with(artifacts(&["a", "b"], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    click(&mut rt, &format!("{PREFIX}-button-1"));
    assert_eq!(rt.state.exports, vec!["b".to_string()]);
}

#[test]
fn the_in_flight_guard_blocks_a_second_press_while_busy() {
    let mut rt = mount(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    click(&mut rt, &format!("{PREFIX}-button-0"));
    assert_eq!(rt.state.exports, vec!["artifact-1".to_string()]);
    assert!(rt.state.state.busy);
    // The rebuilt bar leaves the press handler off while busy, so the second
    // press at the same spot records nothing.
    click(&mut rt, &format!("{PREFIX}-button-0"));
    assert_eq!(rt.state.exports, vec!["artifact-1".to_string()]);
}

#[test]
fn a_busy_bar_renders_dimmed_buttons_that_do_not_fire() {
    let mut scene = Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete));
    scene.state.busy = true;
    let mut rt = mount(scene, (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-saving")),
        t("query.artifactExport.saving")
    );
    assert_eq!(element(&rt, &format!("{PREFIX}-button-0")).role, "button");
    assert!(
        (element(&rt, &format!("{PREFIX}-button-0")).opacity - 0.5).abs() < 0.001,
        "a disabled button renders at React's disabled:opacity-50"
    );
    click(&mut rt, &format!("{PREFIX}-button-0"));
    assert!(rt.state.exports.is_empty());
}

#[test]
fn saved_and_error_status_lines_render_with_their_roles() {
    let mut scene = Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete));
    scene.state.saved = true;
    let rt = mount(scene, (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-saved")),
        t("query.artifactExport.saved")
    );
    assert_eq!(element(&rt, &format!("{PREFIX}-saved")).role, "status");
    assert!(!has_key(&rt, &format!("{PREFIX}-saving")));
    assert!(!has_key(&rt, &format!("{PREFIX}-error")));

    let mut scene = Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete));
    scene.state.error = Some("Synthetic export failure".to_string());
    let rt = mount(scene, (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-error")),
        "Synthetic export failure"
    );
    assert_eq!(element(&rt, &format!("{PREFIX}-error")).role, "alert");
    assert!(!has_key(&rt, &format!("{PREFIX}-saved")));
}

#[test]
fn without_an_export_callback_the_buttons_render_but_do_nothing() {
    let mut rt = mount_with(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete)),
        (800.0, 600.0),
        view_no_export,
    );
    assert!(has_key(&rt, &format!("{PREFIX}-button-0")));
    click(&mut rt, &format!("{PREFIX}-button-0"));
    assert!(rt.state.exports.is_empty());
    assert!(!rt.state.state.busy);
}

#[test]
fn a_custom_key_prefix_moves_every_key_under_it() {
    let rt = mount_with(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete)),
        (800.0, 600.0),
        view_custom_key,
    );
    assert!(!has_key(&rt, PREFIX));
    assert!(has_key(&rt, "custom-export-bar"));
    assert!(has_key(&rt, "custom-export-bar-button-0"));
    assert_eq!(
        text_of(&rt, "custom-export-bar-button-0-label"),
        t("query.artifactExport.action")
    );
}

#[test]
fn keys_are_unique_within_the_bar() {
    let rt = mount(
        Scene::with(artifacts(&["a", "b", "c"], ResultCompleteness::Truncated)),
        (800.0, 600.0),
    );
    let mut keys = own_keys(&rt);
    let before = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), before, "duplicate keys in the bar");
    assert!(has_key(&rt, &format!("{PREFIX}-partial")));
}

#[test]
fn the_bar_fits_both_window_sizes_in_both_themes() {
    for (dark, size, label) in [
        (true, (960.0, 640.0), "dark 960x640"),
        (false, (960.0, 640.0), "light 960x640"),
        (true, (1440.0, 816.0), "dark 1440x816"),
        (false, (1440.0, 816.0), "light 1440x816"),
    ] {
        let mut scene = Scene::with(artifacts(
            &["artifact-1", "artifact-2", "artifact-3"],
            ResultCompleteness::Truncated,
        ));
        scene.dark = dark;
        scene.state.saved = true;
        let rt = mount(scene, size);
        assert_no_horizontal_overflow(&rt, size.0, size.1, label);
    }
}
