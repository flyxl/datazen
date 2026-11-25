//! T064: `components/admin/PrivilegeSelector.tsx` port — the
//! `x-datazen` parse, group flattening, and the mounted selector.
//!
//! Pure-logic tests call the helpers directly; runtime tests render the
//! component in a probe `Runtime` and read back the laid-out element
//! tree — never the component's own report of itself. Geometry is
//! asserted on `rt.elements()` rects, which come from the layout
//! engine, not from the code under test. The group label's uppercase is
//! a draw-time `text_case` transform — the dev tree keeps the authored
//! text — so label text is asserted as authored.

use rune::{Cx, El, Env, Runtime, col};
use serde_json::json;

use datazen_ui::components::admin::{
    PrivilegeGroup, PrivilegeSelector, all_privileges, privilege_groups, visible_privilege_groups,
};
use datazen_ui::ui::kit::kit;

const PREFIX: &str = "privilege-selector";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn groups() -> Vec<PrivilegeGroup> {
    vec![
        PrivilegeGroup::new(
            "Tables",
            ["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE"],
        ),
        PrivilegeGroup::new("", ["USAGE"]),
        PrivilegeGroup::new("Admin", ["GRANT", "REVOKE"]),
    ]
}

#[derive(Default)]
struct Scene {
    groups: Vec<PrivilegeGroup>,
    selected: Vec<String>,
    toggled: Vec<String>,
    columns: usize,
    grid_gap: f32,
    item_font_size: f32,
    item_gap: f32,
    dark: bool,
}

impl Scene {
    fn with(groups: Vec<PrivilegeGroup>, selected: &[&str]) -> Self {
        Self {
            groups,
            selected: selected.iter().map(|s| (*s).to_string()).collect(),
            // React's `className` / `itemClassName` defaults.
            columns: 2,
            grid_gap: 8.0,
            item_font_size: 14.0,
            item_gap: 8.0,
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        PrivilegeSelector::<Scene>::new(
            scene.groups.clone(),
            |s: &Scene, privilege: &str| s.selected.iter().any(|n| n == privilege),
            |s: &mut Scene, privilege: String| {
                s.toggled.push(privilege.clone());
                if s.selected.iter().any(|n| *n == privilege) {
                    s.selected.retain(|n| *n != privilege);
                } else {
                    s.selected.push(privilege);
                }
            },
        )
        .columns(scene.columns)
        .grid_gap(scene.grid_gap)
        .item_font_size(scene.item_font_size)
        .item_gap(scene.item_gap)
        .build(scene, scene.dark),
    )
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        PrivilegeSelector::<Scene>::new(
            scene.groups.clone(),
            |s: &Scene, privilege: &str| s.selected.iter().any(|n| n == privilege),
            |s: &mut Scene, privilege: String| {
                s.toggled.push(privilege);
            },
        )
        .key("custom-selector")
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
/// checkbox at a time — single-privilege scenes below.
fn mark_opacity<S>(rt: &Runtime<S>) -> f32 {
    element(rt, "mark").opacity
}

fn item_key(group: &str, privilege: &str) -> String {
    format!("{PREFIX}-group-{group}-item-{privilege}")
}

// ---- pure model: the x-datazen parse ----

#[test]
fn privilege_groups_parses_the_extension_and_drops_the_unusable() {
    let groups = privilege_groups(&json!({
        "x-datazen": {
            "privilegeGroups": [
                {"label": "Tables", "privileges": ["SELECT", 7, "INSERT"]},
                {"label": 5, "privileges": ["USAGE"]},
                {"privileges": ["GRANT"]},
                {"label": "Empty", "privileges": []},
                "nope",
                {"label": "NoPrivs"}
            ]
        }
    }));
    assert_eq!(
        groups,
        vec![
            PrivilegeGroup::new("Tables", ["SELECT", "INSERT"]),
            PrivilegeGroup::new("", ["USAGE"]),
            PrivilegeGroup::new("", ["GRANT"]),
        ],
        "non-string privileges drop, a non-string label becomes \"\", \
         empty groups and non-object entries drop"
    );
}

#[test]
fn privilege_groups_needs_the_extension_object() {
    assert!(privilege_groups(&json!({})).is_empty());
    assert!(privilege_groups(&json!({"x-datazen": "nope"})).is_empty());
    assert!(privilege_groups(&json!({"x-datazen": {"privilegeGroups": "nope"}})).is_empty());
    assert!(privilege_groups(&json!({"x-datazen": {"privilegeGroups": []}})).is_empty());
}

#[test]
fn visible_groups_skip_the_empty_and_all_flattens_in_order() {
    let groups = vec![
        PrivilegeGroup::new("Tables", ["SELECT", "INSERT"]),
        PrivilegeGroup::new("Empty", Vec::<String>::new()),
        PrivilegeGroup::new("", ["USAGE"]),
    ];
    assert_eq!(
        visible_privilege_groups(&groups)
            .iter()
            .map(|g| g.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Tables", ""]
    );
    assert_eq!(
        all_privileges(&groups),
        vec!["SELECT", "INSERT", "USAGE"],
        "`allPrivileges` flattens the visible groups in order"
    );
    assert!(all_privileges(&[]).is_empty());
    assert!(visible_privilege_groups(&[]).is_empty());
}

#[test]
fn the_group_key_is_the_label_or_default() {
    assert_eq!(PrivilegeGroup::new("Tables", ["SELECT"]).key(), "Tables");
    assert_eq!(PrivilegeGroup::new("", ["USAGE"]).key(), "default");
}

// ---- mounted selector: groups, grid, selection ----

#[test]
fn the_groups_render_in_order_with_labels_and_items() {
    let rt = mount(Scene::with(groups(), &["SELECT"]), (800.0, 600.0));
    assert!(has_key(&rt, PREFIX));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-group-Tables-label")),
        "Tables",
        "the dev tree keeps the authored label; the uppercase is draw-time"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-group-Tables-label")).text_color,
        Some(kit(true).c.text_muted),
        "the group label is muted"
    );
    assert!(
        has_key(&rt, &item_key("Tables", "SELECT")),
        "the privilege row itself is keyed"
    );
    assert_eq!(
        text_of(&rt, &format!("{}-name", item_key("Tables", "SELECT"))),
        "SELECT"
    );
    assert_eq!(
        element(&rt, &format!("{}-name", item_key("Tables", "SELECT"))).text_color,
        Some(kit(true).c.text_base),
        "the privilege name is the foreground colour"
    );
    // The unlabelled group renders no label and keys under `default`.
    assert!(!has_key(&rt, &format!("{PREFIX}-group-default-label")));
    assert!(has_key(
        &rt,
        &format!("{}-name", item_key("default", "USAGE"))
    ));
    assert!(has_key(&rt, &format!("{PREFIX}-group-Admin-label")));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-group-Admin-label")),
        "Admin"
    );
}

#[test]
fn the_grid_lays_privileges_in_two_columns_by_default() {
    let rt = mount(Scene::with(groups(), &[]), (800.0, 600.0));
    let rect = |privilege: &str| element(&rt, &item_key("Tables", privilege)).rect;
    let select = rect("SELECT");
    let insert = rect("INSERT");
    let update = rect("UPDATE");
    let delete = rect("DELETE");
    let truncate = rect("TRUNCATE");
    assert!(
        (select[1] - insert[1]).abs() < 0.5,
        "the first two privileges share the first row"
    );
    assert!(
        insert[0] > select[0],
        "the second privilege is the second column"
    );
    assert!(
        update[1] > select[1],
        "the third privilege wraps to the second row"
    );
    assert!(
        (update[0] - select[0]).abs() < 0.5,
        "the second row starts in the first column"
    );
    assert!(
        (delete[1] - update[1]).abs() < 0.5,
        "row two holds two items"
    );
    assert!(
        truncate[1] > update[1],
        "the fifth privilege starts a third row"
    );
}

#[test]
fn three_columns_hold_three_privileges_per_row() {
    let mut scene = Scene::with(groups(), &[]);
    scene.columns = 3;
    let rt = mount(scene, (800.0, 600.0));
    let rect = |privilege: &str| element(&rt, &item_key("Tables", privilege)).rect;
    assert!((rect("SELECT")[1] - rect("INSERT")[1]).abs() < 0.5);
    assert!((rect("SELECT")[1] - rect("UPDATE")[1]).abs() < 0.5);
    assert!(rect("DELETE")[1] > rect("SELECT")[1]);
}

#[test]
fn exactly_the_selected_privileges_are_checked() {
    // One privilege per scene: the kit's check-mark key is unprefixed,
    // so the mark opacity only pins a single checkbox at a time.
    let rt = mount(
        Scene::with(vec![PrivilegeGroup::new("Tables", ["SELECT"])], &["SELECT"]),
        (800.0, 600.0),
    );
    assert_eq!(
        element(&rt, &format!("{}-checkbox", item_key("Tables", "SELECT"))).role,
        "checkbox"
    );
    assert!(
        (mark_opacity(&rt) - 1.0).abs() < 0.001,
        "a selected privilege shows its check mark"
    );

    let rt = mount(
        Scene::with(vec![PrivilegeGroup::new("Tables", ["SELECT"])], &[]),
        (800.0, 600.0),
    );
    assert!(
        mark_opacity(&rt).abs() < 0.001,
        "an unselected privilege hides its check mark"
    );
}

#[test]
fn toggling_a_privilege_through_its_checkbox_and_its_name() {
    let mut rt = mount(Scene::with(groups(), &["SELECT", "INSERT"]), (800.0, 600.0));
    click(
        &mut rt,
        &format!("{}-checkbox", item_key("Tables", "INSERT")),
    );
    assert_eq!(rt.state.toggled, vec!["INSERT".to_string()]);
    assert!(
        !rt.state.selected.contains(&"INSERT".to_string()),
        "the press deselects the privilege in the host state"
    );

    let mut rt = mount(Scene::with(groups(), &[]), (800.0, 600.0));
    click(&mut rt, &format!("{}-name", item_key("Admin", "GRANT")));
    assert_eq!(rt.state.toggled, vec!["GRANT".to_string()]);
    assert!(rt.state.selected.contains(&"GRANT".to_string()));
    // Toggling one privilege leaves the others alone.
    click(&mut rt, &format!("{}-name", item_key("default", "USAGE")));
    assert_eq!(
        rt.state.toggled,
        vec!["GRANT".to_string(), "USAGE".to_string()]
    );
    assert!(rt.state.selected.contains(&"GRANT".to_string()));
}

#[test]
fn an_empty_group_list_renders_the_empty_root() {
    let rt = mount(Scene::with(vec![], &[]), (800.0, 600.0));
    assert!(has_key(&rt, PREFIX));
    assert!(!has_key(&rt, &format!("{PREFIX}-group-Tables")));
    // A list of only empty groups renders the same nothing.
    let rt = mount(
        Scene::with(
            vec![PrivilegeGroup::new("Empty", Vec::<String>::new())],
            &[],
        ),
        (800.0, 600.0),
    );
    assert!(has_key(&rt, PREFIX));
    assert!(!has_key(&rt, &format!("{PREFIX}-group-Empty")));
}

#[test]
fn a_custom_key_prefix_moves_every_key_under_it() {
    let rt = mount_with(
        Scene::with(groups(), &["SELECT"]),
        (800.0, 600.0),
        view_custom_key,
    );
    assert!(!has_key(&rt, PREFIX));
    assert!(has_key(&rt, "custom-selector"));
    assert!(has_key(&rt, "custom-selector-group-Tables-label"));
    assert!(has_key(
        &rt,
        "custom-selector-group-Tables-item-SELECT-checkbox"
    ));
}

#[test]
fn the_privilege_view_overrides_render_three_narrow_columns() {
    // `PrivilegeView` passes `grid-cols-3 gap-1.5` and 11pt items.
    let mut scene = Scene::with(groups(), &[]);
    scene.columns = 3;
    scene.grid_gap = 6.0;
    scene.item_font_size = 11.0;
    scene.item_gap = 6.0;
    let rt = mount(scene, (800.0, 600.0));
    let rect = |privilege: &str| element(&rt, &item_key("Tables", privilege)).rect;
    assert!((rect("SELECT")[1] - rect("UPDATE")[1]).abs() < 0.5);
    assert!(rect("DELETE")[1] > rect("SELECT")[1]);
}

#[test]
fn the_selector_fits_both_window_sizes_in_both_themes() {
    for (dark, size, label) in [
        (true, (960.0, 640.0), "dark 960x640"),
        (false, (960.0, 640.0), "light 960x640"),
        (true, (1440.0, 816.0), "dark 1440x816"),
        (false, (1440.0, 816.0), "light 1440x816"),
    ] {
        let mut scene = Scene::with(groups(), &["SELECT"]);
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
