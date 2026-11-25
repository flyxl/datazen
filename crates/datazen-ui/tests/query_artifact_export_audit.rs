//! Independent audit for T057 (`QueryArtifactExport` port). Pins branches the
//! coder's own suite does not: the in-flight guard changing nothing on a
//! re-entry, a cancelled / failed outcome leaving a prior saved mark alone
//! (React only ever *sets* saved, on a `saved` receipt), a saved receipt for
//! an unchecked artifact id, the second-layer guard swallowing a press that
//! reaches a stale tree while busy, and the saved + partial lines rendering
//! together. No existing assertion is weakened here.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::query::query_artifact_export::{
    ArtifactExportReceipt, ArtifactExportState, QueryArtifactExport, QueryResultArtifacts,
    ResultCompleteness,
};
use datazen_ui::i18n::t;

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

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
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

// ---- state machine: outcomes React only sets, never clears ----

#[test]
fn begin_export_when_busy_changes_nothing() {
    // React's `if (inFlight.current) return` before any setState: a
    // re-entry leaves the previous outcome exactly as it was.
    let mut state = ArtifactExportState {
        busy: true,
        saved: true,
        error: Some("old failure".to_string()),
    };
    assert!(!state.begin_export());
    assert!(state.busy);
    assert!(state.saved);
    assert_eq!(state.error.as_deref(), Some("old failure"));
}

#[test]
fn a_cancelled_receipt_leaves_a_prior_saved_mark_alone() {
    // React's receipt handling is `if (receipt.status === 'saved')
    // setSaved(true)` — a cancellation never touches the saved mark.
    let mut state = ArtifactExportState {
        saved: true,
        ..ArtifactExportState::default()
    };
    state.finish_export(&ArtifactExportReceipt::Cancelled);
    assert!(!state.busy);
    assert!(state.saved);
    assert_eq!(state.error, None);
}

#[test]
fn a_saved_receipt_for_another_id_still_marks_saved() {
    // React checks only `receipt.status`, never the receipt's artifact id.
    let mut state = ArtifactExportState::default();
    assert!(state.begin_export());
    state.finish_export(&ArtifactExportReceipt::Saved {
        artifact_id: "some-other-artifact".to_string(),
        byte_length: 7,
    });
    assert!(!state.busy);
    assert!(state.saved);
}

#[test]
fn fail_export_leaves_a_prior_saved_mark_alone() {
    // React's catch sets the error and clears busy in `finally`; it never
    // touches the saved mark.
    let mut state = ArtifactExportState {
        saved: true,
        ..ArtifactExportState::default()
    };
    state.fail_export("boom");
    assert!(!state.busy);
    assert!(state.saved);
    assert_eq!(state.error.as_deref(), Some("boom"));
}

// ---- mounted bar: the second-layer guard and co-rendered lines ----

#[test]
fn a_stale_press_while_busy_is_swallowed_by_the_guard() {
    // The bar was built while idle, so its button still carries a press
    // handler; the state flips to busy before the next frame rebuilds the
    // tree. The press must still be a no-op via `begin_export` returning
    // false — the guard's second layer, not just the handler being off.
    let mut rt = mount(
        Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    rt.state.state.busy = true;
    click(&mut rt, &format!("{PREFIX}-button-0"));
    assert!(rt.state.exports.is_empty());
    assert!(rt.state.state.busy);
}

#[test]
fn clicking_the_first_button_exports_only_that_id() {
    let mut rt = mount(
        Scene::with(artifacts(&["a", "b"], ResultCompleteness::Complete)),
        (800.0, 600.0),
    );
    click(&mut rt, &format!("{PREFIX}-button-0"));
    assert_eq!(rt.state.exports, vec!["a".to_string()]);
}

#[test]
fn saved_and_partial_render_together_when_truncated() {
    let mut scene = Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Truncated));
    scene.state.saved = true;
    let rt = mount(scene, (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-partial")),
        t("query.artifactExport.partial")
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-saved")),
        t("query.artifactExport.saved")
    );
    assert_eq!(element(&rt, &format!("{PREFIX}-saved")).role, "status");
}

#[test]
fn busy_bar_shows_the_saving_status_with_its_role() {
    let mut scene = Scene::with(artifacts(&["artifact-1"], ResultCompleteness::Complete));
    scene.state.busy = true;
    let rt = mount(scene, (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-saving")),
        t("query.artifactExport.saving")
    );
    assert_eq!(element(&rt, &format!("{PREFIX}-saving")).role, "status");
}
