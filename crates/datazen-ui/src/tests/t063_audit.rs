//! T063 audit tests (independent tester): the moved `db_badge_meta`
//! table's re-export path, light-theme fills, the JS `Math.round`
//! half-up edge of `corner_font_size`, capture-over-capture, and the
//! PK-chip / type-label no-toggle deviation of the picker.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::db_type_badge::{badge_tone_fill, corner_font_size, db_badge_meta};
use crate::components::error_boundary::ErrorBoundaryState;
use crate::components::export::export_column_picker::{
    ExportColumnPicker, ExportColumnPickerColumn,
};
use crate::ui::badge::BadgeTone;
use crate::ui::kit::kit;
use crate::views::db_badge_meta as backup_db_badge_meta;

const PREFIX: &str = "export-column-picker";

#[test]
fn the_backup_reexport_path_still_resolves_to_the_same_table() {
    // T063 moved the table to components::db_type_badge; the old
    // `views::backup::db_badge_meta` path must keep working (crate-wide
    // as `crate::views::db_badge_meta`, re-exported through
    // views/backup/mod.rs) and return the identical table (22 in-tree
    // types + the unknown fallback).
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
        assert_eq!(
            backup_db_badge_meta(db_type),
            (label, tone),
            "{db_type} via the backup path"
        );
        assert_eq!(
            backup_db_badge_meta(db_type),
            db_badge_meta(db_type),
            "{db_type}: backup path and canonical home agree"
        );
    }
    for db_type in ["unknown", "", "kiwi", "presto", "trino", "superset"] {
        assert_eq!(
            backup_db_badge_meta(db_type),
            ("DB", BadgeTone::Neutral),
            "{db_type} falls back via the backup path"
        );
    }
}

#[test]
fn the_moved_tone_fill_matches_in_both_themes() {
    // The old inline match in backup_window.rs is now
    // `components::db_type_badge::badge_tone_fill` (re-exported from
    // `backup_window`, which is private to `views`, so the crate-wide
    // check is the kit-token projection in both themes); the Backup
    // window's mounted tile is covered by backup_window_audit.
    for tone in [
        BadgeTone::Neutral,
        BadgeTone::Success,
        BadgeTone::Warning,
        BadgeTone::Danger,
        BadgeTone::Accent,
    ] {
        for dark in [true, false] {
            let k = kit(dark);
            let expected = match tone {
                BadgeTone::Neutral => k.c.text_faint,
                BadgeTone::Success => k.c.success.fg,
                BadgeTone::Warning => k.c.warning.fg,
                BadgeTone::Danger => k.c.danger.fg,
                BadgeTone::Accent => k.c.bg_accent,
            };
            assert_eq!(
                badge_tone_fill(tone, dark),
                expected,
                "{tone:?} dark={dark}"
            );
        }
    }
    // Light-theme fills are the same kit tokens and still distinct.
    let k = kit(false);
    let fills = [
        (BadgeTone::Neutral, k.c.text_faint),
        (BadgeTone::Success, k.c.success.fg),
        (BadgeTone::Warning, k.c.warning.fg),
        (BadgeTone::Danger, k.c.danger.fg),
        (BadgeTone::Accent, k.c.bg_accent),
    ];
    let mut seen = Vec::new();
    for (tone, expected) in fills {
        assert_eq!(
            badge_tone_fill(tone, false),
            expected,
            "{tone:?} light fill"
        );
        assert!(!seen.contains(&expected), "two tones share a light fill");
        seen.push(expected);
    }
}

#[test]
fn corner_font_size_rounds_half_up_like_js_math_round() {
    // size 25 -> 25 * 0.38 = 9.5 -> JS Math.round gives 10; Rust
    // f32::round rounds half away from zero, the same for positives.
    assert_eq!(corner_font_size(25.0), 10.0);
    assert_eq!(corner_font_size(30.0), 11.0);
}

#[test]
fn a_second_capture_replaces_the_first_error() {
    let mut state = ErrorBoundaryState::default();
    state.capture("first", Some("stack-one".to_string()));
    state.capture("second", Some("stack-two".to_string()));
    assert!(state.has_error);
    assert_eq!(state.display_message(), "second");
    assert_eq!(state.copy_text(), "stack-two");
    // Replacing with a message-only error drops the old stack.
    state.capture("third", None);
    assert_eq!(state.copy_text(), "third");
}

// ---- picker: PK chip and type label must not toggle ----

#[derive(Default)]
struct Scene {
    columns: Vec<ExportColumnPickerColumn>,
    selected: Vec<String>,
    toggled: Vec<String>,
    dark: bool,
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
            |_s: &mut Scene| {},
            |_s: &mut Scene| {},
            |_s: &mut Scene| {},
        )
        .build(scene, scene.dark),
    )
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn clicking_the_pk_chip_or_the_type_label_does_not_toggle() {
    let scene = Scene {
        columns: vec![
            ExportColumnPickerColumn::new("id")
                .with_type("integer")
                .primary_key(),
            ExportColumnPickerColumn::new("name").with_type("varchar"),
        ],
        selected: vec!["id".to_string()],
        dark: true,
        ..Scene::default()
    };
    let mut rt = Runtime::new(
        scene,
        view,
        (800.0, 600.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    // React's whole-row <label> would toggle on both; Rune's chip and
    // type label are plain text, so these presses must be no-ops.
    click(&mut rt, &format!("{PREFIX}-column-id-pk"));
    click(&mut rt, &format!("{PREFIX}-column-id-pk-text"));
    click(&mut rt, &format!("{PREFIX}-column-id-type"));
    click(&mut rt, &format!("{PREFIX}-column-name-type"));
    assert!(
        rt.state.toggled.is_empty(),
        "chip/type presses toggled: {:?}",
        rt.state.toggled
    );
    assert_eq!(rt.state.selected, vec!["id".to_string()]);
    // Control: the same scene still toggles through the name text.
    click(&mut rt, &format!("{PREFIX}-column-name-name"));
    assert_eq!(rt.state.toggled, vec!["name".to_string()]);
}
