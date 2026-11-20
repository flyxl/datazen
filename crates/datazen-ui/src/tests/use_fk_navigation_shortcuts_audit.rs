//! T275 audit: the useFkNavigationShortcuts port, verified against the
//! React source re-read line by line (the hook has no direct React
//! test — the FK navigation journeys cover it end to end; the decision
//! table below is re-derived from the handler's own body).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use crate::hooks::use_fk_navigation_shortcuts::{
    decide_fk_navigation_shortcut, FkArrow, FkKeyContext, FkNavigationShortcut,
};

fn ctx(arrow: Option<FkArrow>, composing: bool, editable: bool, mod_key: bool, alt: bool) -> FkKeyContext {
    FkKeyContext {
        composing,
        editable_target: editable,
        mod_key,
        alt_key: alt,
        arrow,
    }
}

/// The happy chords: mod+alt+Left goes back, mod+alt+Right goes forward
/// (⌘ and Ctrl both count as the mod).
#[test]
fn the_chords_navigate() {
    // Ctrl+Alt+Left.
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, false, true, true)),
        FkNavigationShortcut::Back
    );
    // Cmd+Alt+Right.
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Right), false, false, true, true)),
        FkNavigationShortcut::Forward
    );
}

/// The disabled gate: a tab that was never reached by a jump has no
/// stack — every key is left alone.
#[test]
fn disabled_leaves_every_key_alone() {
    for arrow in [Some(FkArrow::Left), Some(FkArrow::Right), None] {
        assert_eq!(
            decide_fk_navigation_shortcut(false, &ctx(arrow, false, false, true, true)),
            FkNavigationShortcut::LeaveAlone
        );
    }
}

/// The composition guard: an IME composition owns EVERY key — a
/// Chinese input in the middle of a word must not navigate away.
#[test]
fn a_composition_owns_the_key() {
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), true, false, true, true)),
        FkNavigationShortcut::LeaveAlone
    );
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Right), true, false, true, true)),
        FkNavigationShortcut::LeaveAlone
    );
}

/// The editable guard: an open cell editor must not lose the arrows.
#[test]
fn an_editable_target_owns_the_key() {
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, true, true, true)),
        FkNavigationShortcut::LeaveAlone
    );
}

/// The chord's negative space: no mod, no alt, or another key — all
/// left alone (no preventDefault, no error).
#[test]
fn half_chords_and_other_keys_are_left_alone() {
    // Alt+Left without the mod.
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, false, false, true)),
        FkNavigationShortcut::LeaveAlone
    );
    // Mod+Left without the alt.
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, false, true, false)),
        FkNavigationShortcut::LeaveAlone
    );
    // Mod+Alt+Up (some other key entirely).
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(None, false, false, true, true)),
        FkNavigationShortcut::LeaveAlone
    );
    // Bare Left.
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, false, false, false)),
        FkNavigationShortcut::LeaveAlone
    );
}

/// The guards run before the chord: a composition inside an editor
/// with the full chord still loses (the order is the React handler's).
#[test]
fn the_guards_precede_the_chord() {
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), true, true, true, true)),
        FkNavigationShortcut::LeaveAlone,
        "composition first"
    );
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, true, true, true)),
        FkNavigationShortcut::LeaveAlone,
        "then the editable target"
    );
    assert_eq!(
        decide_fk_navigation_shortcut(true, &ctx(Some(FkArrow::Left), false, false, true, true)),
        FkNavigationShortcut::Back,
        "only then the chord"
    );
}
