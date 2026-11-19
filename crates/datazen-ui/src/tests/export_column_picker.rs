//! T063: `components/export/ExportColumnPicker.tsx` port — display order,
//! selection, toggles, bulk actions, and the mounted picker.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::export::export_column_picker::{
    ExportColumnPicker, ExportColumnPickerColumn,
};
use crate::i18n::{has, t};
use crate::support::data_type_colors::data_type_color;

const PREFIX: &str = "export-column-picker";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn columns() -> Vec<ExportColumnPickerColumn> {
    vec![
        ExportColumnPickerColumn::new("id")
            .with_type("integer")
            .primary_key(),
        ExportColumnPickerColumn::new("name").with_type("varchar"),
        ExportColumnPickerColumn::new("note").with_type("text"),
    ]
}

#[derive(Default)]
struct Scene {
    columns: Vec<ExportColumnPickerColumn>,
    selected: Vec<String>,
    toggled: Vec<String>,
    select_all: usize,
    clear_all: usize,
    reset_default: usize,
    dark: bool,
}

impl Scene {
    fn with(columns: Vec<ExportColumnPickerColumn>, selected: &[&str]) -> Self {
        Self {
            columns,
            selected: selected.iter().map(|s| (*s).to_string()).collect(),
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        ExportColumnPicker::<Scene>::new(
            scene.columns.clone(),
            |s: &Scene, name: &str| s.selected.iter().any(|n| n == name),
            |s: &mut Scene, name: String| {
                s.toggled.push(name.clone());
                if s.selected.iter().any(|n| *n == name) {
                    s.selected.retain(|n| *n != name);
                } else {
                    s.selected.push(name);
                }
            },
            |s: &mut Scene| s.select_all += 1,
            |s: &mut Scene| s.clear_all += 1,
            |s: &mut Scene| s.reset_default += 1,
        )
        .build(scene, scene.dark),
    )
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        ExportColumnPicker::<Scene>::new(
            scene.columns.clone(),
            |s: &Scene, name: &str| s.selected.iter().any(|n| n == name),
            |s: &mut Scene, name: String| {
                s.toggled.push(name);
            },
            |s: &mut Scene| s.select_all += 1,
            |s: &mut Scene| s.clear_all += 1,
            |s: &mut Scene| s.reset_default += 1,
        )
        .key("custom-picker")
        .build(scene, scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    mount_with(scene, size, view)
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

/// The kit checkbox's check mark: opacity 1 when on, 0 when off. The
/// mark key is the kit's own (unprefixed), so this only pins one
/// checkbox at a time — single-column scenes below.
fn mark_opacity<S>(rt: &Runtime<S>) -> f32 {
    element(rt, "mark").opacity
}

fn row_keys<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(&format!("{PREFIX}-column-")) && !k.contains("-checkbox"))
        .collect()
}

// ---- pure model: the column builder ----

#[test]
fn the_column_builder_defaults_to_no_type_and_no_pk() {
    let col = ExportColumnPickerColumn::new("id");
    assert_eq!(col.name, "id");
    assert_eq!(col.col_type, None);
    assert!(!col.is_primary_key);
    let col = ExportColumnPickerColumn::new("id")
        .with_type("integer")
        .primary_key();
    assert_eq!(col.col_type.as_deref(), Some("integer"));
    assert!(col.is_primary_key);
}

#[test]
fn every_picker_key_resolves() {
    for key in [
        "export.selectColumns",
        "common.selectAll",
        "common.deselectAll",
        "export.resetColumns",
    ] {
        assert!(has(key), "{key} is missing from the locale packs");
        assert_ne!(t(key), key, "{key} echoes instead of translating");
    }
}

// ---- mounted picker: order, labels, selection ----

#[test]
fn the_columns_render_in_display_order_with_type_labels_and_a_pk_chip() {
    let rt = mount(
        Scene::with(columns(), &["id", "name", "note"]),
        (800.0, 600.0),
    );
    assert!(has_key(&rt, PREFIX));
    assert!(has_key(&rt, &format!("{PREFIX}-list")));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-label")),
        t("export.selectColumns")
    );
    // Row order is the caller's display order (the export writes in
    // this order); each row carries its name, type, and — for the key
    // column — the PK chip.
    assert_eq!(
        row_keys(&rt),
        vec![
            format!("{PREFIX}-column-id"),
            format!("{PREFIX}-column-id-name"),
            format!("{PREFIX}-column-id-pk"),
            format!("{PREFIX}-column-id-pk-text"),
            format!("{PREFIX}-column-id-type"),
            format!("{PREFIX}-column-name"),
            format!("{PREFIX}-column-name-name"),
            format!("{PREFIX}-column-name-type"),
            format!("{PREFIX}-column-note"),
            format!("{PREFIX}-column-note-name"),
            format!("{PREFIX}-column-note-type"),
        ]
    );
    assert_eq!(text_of(&rt, &format!("{PREFIX}-column-id-name")), "id");
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-column-id-pk-text")),
        "PK"
    );
    assert_eq!(text_of(&rt, &format!("{PREFIX}-column-id-type")), "integer");
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-column-name-type")),
        "varchar"
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-column-name-pk")));
    // The type label paints with its data-type family colour.
    assert_eq!(
        element(&rt, &format!("{PREFIX}-column-id-type")).text_color,
        Some(data_type_color(Some("integer"), None, true))
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-column-note-type")).text_color,
        Some(data_type_color(Some("text"), None, true))
    );
}

#[test]
fn exactly_the_selected_columns_are_checked() {
    // One column per scene: the kit's check-mark key is unprefixed, so
    // the mark opacity only pins a single checkbox at a time.
    let rt = mount(
        Scene::with(vec![ExportColumnPickerColumn::new("name")], &["name"]),
        (800.0, 600.0),
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-column-name-checkbox")).role,
        "checkbox"
    );
    assert!(
        (mark_opacity(&rt) - 1.0).abs() < 0.001,
        "a selected column shows its check mark"
    );

    let rt = mount(
        Scene::with(vec![ExportColumnPickerColumn::new("name")], &[]),
        (800.0, 600.0),
    );
    assert!(
        mark_opacity(&rt).abs() < 0.001,
        "an unselected column hides its check mark"
    );
}

#[test]
fn toggling_a_column_through_its_checkbox_and_its_name() {
    let mut rt = mount(
        Scene::with(columns(), &["id", "name", "note"]),
        (800.0, 600.0),
    );
    click(&mut rt, &format!("{PREFIX}-column-name-checkbox"));
    assert_eq!(rt.state.toggled, vec!["name".to_string()]);
    assert!(
        !rt.state.selected.contains(&"name".to_string()),
        "the press deselects the column in the host state"
    );

    let mut rt = mount(Scene::with(columns(), &["id"]), (800.0, 600.0));
    click(&mut rt, &format!("{PREFIX}-column-note-name"));
    assert_eq!(rt.state.toggled, vec!["note".to_string()]);
    assert!(rt.state.selected.contains(&"note".to_string()));
}

#[test]
fn the_bulk_actions_reach_the_host() {
    let mut rt = mount(Scene::with(columns(), &["id"]), (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-select-all-label")),
        t("common.selectAll")
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-clear-all-label")),
        t("common.deselectAll")
    );
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-reset-default-label")),
        t("export.resetColumns")
    );
    click(&mut rt, &format!("{PREFIX}-select-all"));
    assert_eq!(rt.state.select_all, 1);
    click(&mut rt, &format!("{PREFIX}-clear-all"));
    assert_eq!(rt.state.clear_all, 1);
    click(&mut rt, &format!("{PREFIX}-reset-default"));
    assert_eq!(rt.state.reset_default, 1);
}

#[test]
fn an_empty_column_list_renders_the_empty_box() {
    let rt = mount(Scene::with(vec![], &[]), (800.0, 600.0));
    assert!(has_key(&rt, PREFIX));
    assert!(has_key(&rt, &format!("{PREFIX}-list")));
    assert!(!has_key(&rt, &format!("{PREFIX}-column-id")));
}

#[test]
fn a_custom_key_prefix_moves_every_key_under_it() {
    let rt = mount_with(
        Scene::with(columns(), &["id"]),
        (800.0, 600.0),
        view_custom_key,
    );
    assert!(!has_key(&rt, PREFIX));
    assert!(has_key(&rt, "custom-picker"));
    assert!(has_key(&rt, "custom-picker-list"));
    assert!(has_key(&rt, "custom-picker-column-id-name"));
}

#[test]
fn the_picker_fits_both_window_sizes_in_both_themes() {
    for (dark, size, label) in [
        (true, (960.0, 640.0), "dark 960x640"),
        (false, (960.0, 640.0), "light 960x640"),
        (true, (1440.0, 816.0), "dark 1440x816"),
        (false, (1440.0, 816.0), "light 1440x816"),
    ] {
        let mut scene = Scene::with(columns(), &["id", "name", "note"]);
        scene.dark = dark;
        let rt = mount(scene, size);
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else {
                continue;
            };
            if !key.starts_with(PREFIX) {
                continue;
            }
            let [x, _y, w, _h] = e.rect;
            if x < -0.5 || x + w > size.0 + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{label}: {outside:?} overflow");
    }
}
