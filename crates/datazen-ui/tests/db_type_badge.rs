//! T063: `components/DbTypeBadge.tsx` port — the driver-meta table, the
//! parent map, the sizing rules, and the mounted letter tile.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::db_type_badge::{
    badge_font_size, badge_tone_fill, corner_font_size, db_badge_meta, db_icon_parent, DbTypeBadge,
};
use datazen_ui::ui::badge::BadgeTone;
use datazen_ui::ui::kit::kit;

const PREFIX: &str = "db-type-badge";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Scene {
    db_type: String,
    size: f32,
    dark: bool,
}

impl Scene {
    fn with(db_type: &str, size: f32) -> Self {
        Self {
            db_type: db_type.to_owned(),
            size,
            dark: true,
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DbTypeBadge::new(scene.db_type.clone())
            .size(scene.size)
            .build(scene.dark),
    )
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DbTypeBadge::new(scene.db_type.clone())
            .size(scene.size)
            .key("custom-badge")
            .build(scene.dark),
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

// ---- pure helpers: sizing, parents, meta, fills ----

#[test]
fn badge_font_size_follows_the_react_thresholds() {
    // React: <=20 → 9, <=24 → 10, above → 11.
    assert_eq!(badge_font_size(0.0), 9.0);
    assert_eq!(badge_font_size(20.0), 9.0);
    assert_eq!(badge_font_size(20.5), 10.0);
    assert_eq!(badge_font_size(24.0), 10.0);
    assert_eq!(badge_font_size(24.5), 11.0);
    assert_eq!(badge_font_size(48.0), 11.0);
}

#[test]
fn corner_font_size_is_size_times_038_clamped_to_8_11() {
    // React: Math.min(11, Math.max(8, Math.round(size * 0.38))).
    assert_eq!(corner_font_size(0.0), 8.0);
    assert_eq!(corner_font_size(20.0), 8.0);
    assert_eq!(corner_font_size(24.0), 9.0);
    assert_eq!(corner_font_size(28.0), 11.0);
    assert_eq!(corner_font_size(32.0), 11.0);
}

#[test]
fn the_parent_map_covers_the_six_protocol_reuse_types() {
    for (child, parent) in [
        ("questdb", "postgresql"),
        ("cloudberry", "postgresql"),
        ("doris", "mysql"),
        ("starrocks", "mysql"),
        ("manticore", "mysql"),
        ("ob_oracle", "mysql"),
    ] {
        assert_eq!(
            db_icon_parent(child),
            Some(parent),
            "{child} reuses {parent}"
        );
    }
    for plain in ["mysql", "postgresql", "sqlite", "redis", "unknown"] {
        assert_eq!(db_icon_parent(plain), None, "{plain} has no parent");
    }
}

#[test]
fn the_meta_table_matches_the_shipped_driver_metas() {
    // shortLabel + iconBg, read off crates/drivers/*/ui/meta.ts.
    for (db_type, label, tone) in [
        ("mysql", "My", BadgeTone::Warning),
        ("mariadb", "Ma", BadgeTone::Warning),
        ("doris", "Do", BadgeTone::Warning),
        ("starrocks", "Sr", BadgeTone::Warning),
        ("manticore", "Mc", BadgeTone::Warning),
        ("ob_oracle", "Ob", BadgeTone::Warning),
        ("postgresql", "Pg", BadgeTone::Accent),
        ("questdb", "Qd", BadgeTone::Accent),
        ("cloudberry", "Cb", BadgeTone::Accent),
        ("sqlite", "Lt", BadgeTone::Success),
        ("rqlite", "Rq", BadgeTone::Accent),
        ("turso", "Tu", BadgeTone::Success),
        ("sqlserver", "Ss", BadgeTone::Danger),
        ("clickhouse", "Ch", BadgeTone::Warning),
        ("duckdb", "Dk", BadgeTone::Accent),
        ("redis", "Rd", BadgeTone::Danger),
        ("mongodb", "Mb", BadgeTone::Success),
        ("elasticsearch", "Es", BadgeTone::Warning),
        ("hbase", "Hb", BadgeTone::Warning),
        ("influxdb", "If", BadgeTone::Warning),
        ("vector", "Vc", BadgeTone::Success),
        ("victoriametrics", "Vm", BadgeTone::Accent),
    ] {
        assert_eq!(db_badge_meta(db_type), (label, tone), "{db_type} meta");
    }
}

#[test]
fn unknown_types_fall_back_to_db_on_neutral() {
    // React's getDbIcon fallback, and the git drivers whose metas are
    // not in this checkout (kiwi, presto, trino, superset).
    for db_type in ["unknown", "", "kiwi", "presto", "trino", "superset"] {
        assert_eq!(
            db_badge_meta(db_type),
            ("DB", BadgeTone::Neutral),
            "{db_type} falls back"
        );
    }
}

#[test]
fn every_tone_fills_with_a_distinct_kit_token() {
    let k = kit(true);
    let fills = [
        (BadgeTone::Neutral, k.c.text_faint),
        (BadgeTone::Success, k.c.success.fg),
        (BadgeTone::Warning, k.c.warning.fg),
        (BadgeTone::Danger, k.c.danger.fg),
        (BadgeTone::Accent, k.c.bg_accent),
    ];
    for (tone, expected) in fills {
        assert_eq!(badge_tone_fill(tone, true), expected, "{tone:?} fill");
    }
    let mut seen = Vec::new();
    for (_, fill) in fills {
        assert!(!seen.contains(&fill), "two tones share a fill");
        seen.push(fill);
    }
}

// ---- mounted tile ----

#[test]
fn the_default_tile_is_a_24pt_mysql_tile() {
    let rt = mount(Scene::with("mysql", 24.0), (800.0, 600.0));
    assert!(has_key(&rt, PREFIX));
    let tile = element(&rt, PREFIX);
    assert_eq!(tile.rect[2], 24.0, "tile width is the size");
    assert_eq!(tile.rect[3], 24.0, "tile height is the size");
    assert_eq!(tile.fill, kit(true).c.warning.fg, "mysql paints warning");
    assert!(has_text(&rt, "My"), "the short label shows");
}

#[test]
fn size_20_renders_a_20pt_tile_and_unknown_types_fall_back() {
    let rt = mount(Scene::with("sqlite", 20.0), (800.0, 600.0));
    let tile = element(&rt, PREFIX);
    assert_eq!(tile.rect[2], 20.0);
    assert_eq!(tile.rect[3], 20.0);
    assert_eq!(tile.fill, kit(true).c.success.fg, "sqlite paints success");
    assert!(has_text(&rt, "Lt"));

    let rt = mount(Scene::with("kiwi", 24.0), (800.0, 600.0));
    assert_eq!(element(&rt, PREFIX).fill, kit(true).c.text_faint);
    assert!(has_text(&rt, "DB"), "an unknown type shows DB");
}

#[test]
fn a_custom_key_prefix_moves_the_tile_under_it() {
    let rt = mount_with(Scene::with("mysql", 24.0), (800.0, 600.0), view_custom_key);
    assert!(!has_key(&rt, PREFIX));
    assert!(has_key(&rt, "custom-badge"));
    assert_eq!(
        element(&rt, "custom-badge").fill,
        kit(true).c.warning.fg,
        "the custom-keyed tile still paints its tone"
    );
    assert!(has_text(&rt, "My"));
}

#[test]
fn the_tile_fits_both_window_sizes_in_both_themes() {
    for (dark, size, label) in [
        (true, (960.0, 640.0), "dark 960x640"),
        (false, (960.0, 640.0), "light 960x640"),
        (true, (1440.0, 816.0), "dark 1440x816"),
        (false, (1440.0, 816.0), "light 1440x816"),
    ] {
        let mut scene = Scene::with("postgresql", 24.0);
        scene.dark = dark;
        let rt = mount(scene, size);
        let tile = element(&rt, PREFIX);
        assert_eq!(tile.fill, kit(dark).c.bg_accent, "{label}: accent fill");
        assert!(
            tile.rect[0] >= -0.5 && tile.rect[0] + tile.rect[2] <= size.0 + 0.5,
            "{label}: tile stays inside the window"
        );
    }
}
