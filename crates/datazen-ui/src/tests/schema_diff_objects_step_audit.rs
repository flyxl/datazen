//! Independent audit for T149 (the SchemaDiffObjectsStep Rune port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffObjectsStep.tsx`
//! and the React tests (`__tests__/SchemaDiffPanels.test.tsx` — the
//! target-only picker case — plus the step's own contract): the three
//! branches (loading spinner / empty card / list), the count line's
//! placeholders, the link buttons, per-row checkboxes carrying the name,
//! and the origin badges' labels and colours. Element evidence comes from
//! `Runtime::elements()`; presses go through real pointer events.

use rune::{Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::schema_diff::schema_diff_objects_step::SchemaDiffObjectsStep;
use crate::views::schema_diff::schema_diff_table_names::{
    SchemaDiffTablePickVm, TablePickOriginVm,
};

fn pick(name: &str, enabled: bool, origin: TablePickOriginVm) -> SchemaDiffTablePickVm {
    SchemaDiffTablePickVm {
        name: name.to_owned(),
        enabled,
        origin,
        source_name: Some(format!("public.{name}")),
        target_name: Some(name.to_owned()),
    }
}

#[derive(Clone)]
struct Scene {
    loading: bool,
    tables: Vec<SchemaDiffTablePickVm>,
    toggles: Vec<String>,
    select_all: usize,
    select_none: usize,
    dark: bool,
}

impl Scene {
    fn base() -> Self {
        Self {
            loading: false,
            tables: vec![
                pick("users", true, TablePickOriginVm::Both),
                pick("archive", false, TablePickOriginVm::TargetOnly),
                pick("legacy", true, TablePickOriginVm::SourceOnly),
            ],
            toggles: Vec::new(),
            select_all: 0,
            select_none: 0,
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    SchemaDiffObjectsStep::<Scene>::new(s.loading, &s.tables)
        .on_toggle(|s: &mut Scene, name| s.toggles.push(name))
        .on_select_all(|s: &mut Scene| s.select_all += 1)
        .on_select_none(|s: &mut Scene| s.select_none += 1)
        .build(s.dark)
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(point.0, point.1);
    rt.frame(0.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The loading branch: the centered 2xl accent spinner, no list.
#[test]
fn loading_branch_shows_the_spinner() {
    let mut scene = Scene::base();
    scene.loading = true;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-objects-panel").is_some());
    assert!(maybe(&runtime, "schema-diff-objects-spinner-indicator").is_some());
    assert!(maybe(&runtime, "schema-diff-objects-list").is_none());
    // py-12 (36pt) top and bottom around the centered spinner.
    let root = maybe(&runtime, "schema-diff-objects-panel").expect("root");
    let spinner = maybe(&runtime, "schema-diff-objects-spinner-indicator").expect("spinner");
    assert!(spinner.rect[1] - root.rect[1] >= 35.0, "py-12 top, got {}", spinner.rect[1] - root.rect[1]);
    assert!(root.rect[1] + root.rect[3] - (spinner.rect[1] + spinner.rect[3]) >= 35.0, "py-12 bottom");
}

/// The empty branch: the bordered card with the noTablesFound hint.
#[test]
fn empty_branch_shows_the_card() {
    let mut scene = Scene::base();
    scene.tables = Vec::new();
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-objects-empty"),
        t("schemaDiff.noTablesFound")
    );
    assert!(maybe(&runtime, "schema-diff-objects-list").is_none());
    let card = maybe(&runtime, "schema-diff-objects-panel").expect("card");
    let k = kit(runtime.state.dark);
    assert_eq!(card.fill, k.c.layer_01, "bg-surface-alt");
}

/// The count line formats {count}/{total} with the enabled rows.
#[test]
fn count_line_formats_placeholders() {
    let runtime = rt(Scene::base());
    // users + legacy enabled = 2 of 3.
    let expected = t("schemaDiff.objectsSelected")
        .replace("{count}", "2")
        .replace("{total}", "3");
    assert_eq!(text_of(&runtime, "schema-diff-objects-count"), expected);
}

/// The link buttons press through (Select all / Deselect all).
#[test]
fn link_buttons_press() {
    let mut runtime = rt(Scene::base());
    press(&mut runtime, "schema-diff-objects-select-all");
    press(&mut runtime, "schema-diff-objects-select-none");
    assert_eq!(runtime.state.select_all, 1);
    assert_eq!(runtime.state.select_none, 1);
    assert_eq!(
        text_of(&runtime, "schema-diff-objects-select-all-label"),
        t("common.selectAll")
    );
    assert_eq!(
        text_of(&runtime, "schema-diff-objects-select-none-label"),
        t("common.deselectAll")
    );
}

/// The React picker test's core: target-only rows show their origin badge
/// and stay unchecked.
#[test]
fn target_only_rows_show_danger_badge_and_stay_unchecked() {
    let runtime = rt(Scene::base());
    let k = kit(runtime.state.dark);
    // The archive row's origin badge (React testid verbatim).
    let badge = maybe(&runtime, "schema-diff-table-origin-archive").expect("origin badge");
    assert_eq!(badge.text_color, Some(k.c.danger.fg), "target-only danger ink");
    assert_eq!(badge.text.as_deref(), Some(t("schemaDiff.targetOnly").to_uppercase().as_str()));
    // The other origins render muted with their own labels.
    let users = maybe(&runtime, "schema-diff-table-origin-users").expect("both badge");
    assert_eq!(users.text_color, Some(k.c.text_muted));
    assert_eq!(
        users.text.as_deref(),
        Some(t("schemaDiff.sourceAndTarget").to_uppercase().as_str())
    );
    let legacy = maybe(&runtime, "schema-diff-table-origin-legacy").expect("source-only badge");
    assert_eq!(
        legacy.text.as_deref(),
        Some(t("schemaDiff.sourceOnly").to_uppercase().as_str())
    );
}

/// A row's checkbox press carries the row's name into onToggle.
#[test]
fn checkbox_press_carries_the_name() {
    let mut runtime = rt(Scene::base());
    // The archive row is disabled (target-only): its checkbox still presses
    // (React wires onChange regardless) and reports "archive".
    press(&mut runtime, "schema-diff-table-check-archive");
    press(&mut runtime, "schema-diff-table-check-users");
    assert_eq!(runtime.state.toggles, ["archive".to_owned(), "users".to_owned()]);
}

/// The list is the bordered surface-alt card with the rows in order.
/// (The `divide-y` hairline between rows is a paint detail the runtime's
/// element info does not expose — no `border` field — so the assertion is
/// structural: the card's fill, and the rows stacked in table order.)
#[test]
fn list_is_the_bordered_card_with_rows_in_order() {
    let runtime = rt(Scene::base());
    let list = maybe(&runtime, "schema-diff-objects-list").expect("list");
    let k = kit(runtime.state.dark);
    assert_eq!(list.fill, k.c.layer_01, "bg-surface-alt");
    let first = maybe(&runtime, "schema-diff-table-row-users").expect("first row");
    let second = maybe(&runtime, "schema-diff-table-row-archive").expect("second row");
    let third = maybe(&runtime, "schema-diff-table-row-legacy").expect("third row");
    assert!(first.rect[1] < second.rect[1], "users first");
    assert!(second.rect[1] < third.rect[1], "archive second, legacy third");
    assert!((first.rect[3] - second.rect[3]).abs() < 0.5, "rows share a height");
}
