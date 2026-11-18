//! Independent audit for T017 (the WappIcon Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/workspace/WappIcon.tsx` and its React test
//! `__tests__/WappIcon.test.tsx`: the puzzle fallback when no icon is
//! contributed, the `datazen://` desktop src byte-for-byte
//! (`datazen://acme.bill-audit/assets/icon.svg` from `./assets/icon.svg`),
//! the onError → fallback → recovery-on-icon-change cycle, and the
//! sticky-ness of a failed load while the icon value stays the same.
//! Element evidence comes from `Runtime::elements()` (runtime layout), never
//! from a value the component reports about itself; the picture wire is
//! driven with a real `ImageCache` (a ready RGBA fixture and a genuinely
//! failing decode), not a mock flag.

use std::rc::Rc;
use std::sync::Arc;

use rune::{col, Cx, El, Env, ImageCache, Runtime};

use crate::views::wapp_icon::{
    WappIcon, WappIconKind, WappIconState, build_icon_src,
};

// ---- the state machine (React useState + the [icon] effect) -------------

#[test]
fn a_fresh_state_shows_the_picture_branch() {
    // React mounts with useState(false): broken starts false.
    let state = WappIconState::default();
    assert!(!state.is_broken(), "a fresh site is not broken");
}

#[test]
fn on_error_breaks_and_stays_broken_while_the_icon_value_holds() {
    // React test 3, second half: after fireEvent.error the same icon value
    // re-renders broken — the [icon] effect does not fire for the same value.
    let mut state = WappIconState::default();
    state.sync_icon(Some("assets/icon.svg"));
    state.on_error();
    assert!(state.is_broken(), "onError breaks the site");
    state.sync_icon(Some("assets/icon.svg"));
    assert!(
        state.is_broken(),
        "the same icon value again keeps the fallback (React's effect deps compare by value)"
    );
}

#[test]
fn a_changed_icon_value_recovers_the_picture() {
    // React test 3: rerender with a different icon → the img branch again.
    let mut state = WappIconState::default();
    state.sync_icon(Some("assets/icon.svg"));
    state.on_error();
    state.sync_icon(Some("assets/other.svg"));
    assert!(!state.is_broken(), "a changed icon value resets broken");
    // And it can break again the same way.
    state.on_error();
    assert!(state.is_broken());
    state.sync_icon(Some("assets/third.svg"));
    assert!(!state.is_broken());
}

#[test]
fn to_and_from_no_icon_are_changes_too() {
    // The effect deps are the raw prop value: None → Some and Some → None
    // both fire setBroken(false).
    let mut state = WappIconState::default();
    state.sync_icon(Some("a.svg"));
    state.on_error();
    state.sync_icon(None);
    assert!(!state.is_broken(), "losing the icon recovers");
    state.sync_icon(Some("a.svg"));
    state.on_error();
    state.sync_icon(Some(""));
    assert!(!state.is_broken(), "an empty-string icon is a new value (and falsy in render)");
}

// ---- the desktop resolver src (React test 2's pinned bytes) -------------

#[test]
fn build_icon_src_matches_the_desktop_resolver_bytes() {
    // The exact string React's test pins:
    // buildWappIconSrc('acme.bill-audit', './assets/icon.svg').
    assert_eq!(
        build_icon_src("acme.bill-audit", "./assets/icon.svg"),
        "datazen://acme.bill-audit/assets/icon.svg"
    );
    // Without the leading ./ the path passes through untouched.
    assert_eq!(
        build_icon_src("acme.bill-audit", "assets/icon.svg"),
        "datazen://acme.bill-audit/assets/icon.svg"
    );
    // An absent wappId collapses to the empty host (React `wappId || ''`) —
    // the template's slashes stay, so the bytes are `datazen:///x.svg`,
    // byte-identical to React's template literal with an empty target.
    assert_eq!(build_icon_src("", "x.svg"), "datazen:///x.svg");
    // Only ONE leading ./ is stripped, exactly like `replace(/^\.\//, '')`.
    assert_eq!(
        build_icon_src("acme.bill-audit", "././a.svg"),
        "datazen://acme.bill-audit/./a.svg"
    );
}

// ---- the pure branch decision -------------------------------------------

#[test]
fn kind_falls_back_without_an_icon_or_while_broken() {
    // React: `if (!icon || broken) return <Puzzle …/>` — and '' is falsy.
    assert_eq!(
        WappIcon::new(Some("acme.bill-audit"), None).kind(),
        WappIconKind::Fallback
    );
    assert_eq!(
        WappIcon::new(Some("acme.bill-audit"), Some("")).kind(),
        WappIconKind::Fallback
    );
    assert_eq!(
        WappIcon::new(Some("acme.bill-audit"), Some("a.svg"))
            .broken(true)
            .kind(),
        WappIconKind::Fallback
    );
}

#[test]
fn kind_builds_the_picture_src_from_the_target_id() {
    assert_eq!(
        WappIcon::new(Some("acme.bill-audit"), Some("./assets/icon.svg")).kind(),
        WappIconKind::Picture { src: "datazen://acme.bill-audit/assets/icon.svg".to_owned() }
    );
    // `wappId || ''`: an empty-string id collapses like a missing one — and
    // both keep the empty host between the slashes (`datazen:///a.svg`).
    assert_eq!(
        WappIcon::new(Some(""), Some("a.svg")).kind(),
        WappIconKind::Picture { src: "datazen:///a.svg".to_owned() }
    );
    assert_eq!(
        WappIcon::new(None, Some("a.svg")).kind(),
        WappIconKind::Picture { src: "datazen:///a.svg".to_owned() }
    );
    // The icon prop's empty string is falsy, so it never reaches the src.
    assert_eq!(WappIcon::new(None, Some("")).kind(), WappIconKind::Fallback);
}

// ---- runtime evidence (element tree, both branches) ---------------------

/// A scene's inputs: the props, the size, the broken projection and the
/// picture wire. The cache lives inside the wire.
struct Scene {
    wapp_id: Option<String>,
    icon: Option<String>,
    size_pt: f32,
    broken: bool,
    dark: bool,
    wire: crate::views::wapp_icon::WappIconImages,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().key("probe").child(
        WappIcon::new(s.wapp_id.as_deref(), s.icon.as_deref())
            .size_pt(s.size_pt)
            .broken(s.broken)
            .images(Rc::clone(&s.wire))
            .build(s.dark),
    )
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn no_wire() -> crate::views::wapp_icon::WappIconImages {
    Rc::new(|_src: &str| None)
}

/// A cache whose one fixture is a decoded 8×8 opaque square.
fn ready_picture_wire(src: &str) -> crate::views::wapp_icon::WappIconImages {
    let cache = ImageCache::new(Arc::new(|| {}));
    let image = cache.rgba(8, 8, vec![120u8; 8 * 8 * 4]);
    let size = image.wait().expect("the RGBA fixture decodes");
    assert_eq!(size, [8, 8]);
    let owned = src.to_owned();
    Rc::new(move |asked: &str| (asked == owned).then(|| image.clone()))
}

/// A cache whose one fixture is a genuinely failing decode (garbage bytes),
/// so the wire answers with an errored `Image` — React's onError source.
fn broken_picture_wire(src: &str) -> crate::views::wapp_icon::WappIconImages {
    let cache = ImageCache::new(Arc::new(|| {}));
    let image = cache.load(vec![0x7fu8; 16]);
    let failed = image.wait();
    assert!(failed.is_err(), "the garbage fixture must fail to decode");
    let owned = src.to_owned();
    Rc::new(move |asked: &str| (asked == owned).then(|| image.clone()))
}

#[test]
fn no_icon_renders_the_puzzle_and_no_picture() {
    // React test 1: no icon → an svg (the puzzle) and no img.
    let rt = rt(Scene {
        wapp_id: Some("acme.bill-audit".into()),
        icon: None,
        size_pt: 16.0,
        broken: false,
        dark: true,
        wire: ready_picture_wire("datazen://acme.bill-audit/assets/icon.svg"),
    });
    let puzzle = maybe(&rt, "wapp-icon").expect("the puzzle glyph is keyed");
    assert_eq!(puzzle.role, "icon", "the fallback is the lucide glyph");
    assert!(maybe(&rt, "wapp-icon-picture").is_none(), "no img without an icon");
}

#[test]
fn a_contributed_icon_renders_the_picture_at_its_size() {
    // React test 2: a contributed icon renders the img with the pinned src.
    // The wire answers only for the pinned bytes, so finding the picture
    // proves the src the component asked for.
    for (size, dark) in [(14.0, true), (16.0, false), (20.0, true)] {
        let rt = rt(Scene {
            wapp_id: Some("acme.bill-audit".into()),
            icon: Some("./assets/icon.svg".into()),
            size_pt: size,
            broken: false,
            dark,
            wire: ready_picture_wire("datazen://acme.bill-audit/assets/icon.svg"),
        });
        let picture = maybe(&rt, "wapp-icon-picture")
            .expect("the picture element renders for a contributed icon");
        let [x, y, w, h] = picture.rect;
        assert_eq!((w, h), (size, size), "object-contain box at {size}pt (dark={dark})");
        assert!(x >= 0.0 && y >= 0.0, "inside the window at {x},{y}");
        assert!(maybe(&rt, "wapp-icon").is_none(), "no puzzle while the picture shows");
    }
}

#[test]
fn a_picture_the_host_cannot_provide_falls_back_to_the_puzzle() {
    // The wire's None is a load that cannot even start (unknown wapp, no
    // asset): React's img errors, so the puzzle shows.
    let rt = rt(Scene {
        wapp_id: Some("acme.bill-audit".into()),
        icon: Some("assets/icon.svg".into()),
        size_pt: 16.0,
        broken: false,
        dark: true,
        wire: no_wire(),
    });
    assert!(maybe(&rt, "wapp-icon").is_some(), "the fallback shows");
    assert!(maybe(&rt, "wapp-icon-picture").is_none());
}

#[test]
fn a_failing_decode_falls_back_to_the_puzzle() {
    // The wire answers with a genuinely errored Image (the decode failed):
    // that is onError — the puzzle, this same frame.
    let rt = rt(Scene {
        wapp_id: Some("acme.bill-audit".into()),
        icon: Some("assets/icon.svg".into()),
        size_pt: 16.0,
        broken: false,
        dark: false,
        wire: broken_picture_wire("datazen://acme.bill-audit/assets/icon.svg"),
    });
    assert!(maybe(&rt, "wapp-icon").is_some(), "the errored picture falls back");
    assert!(maybe(&rt, "wapp-icon-picture").is_none());
}

#[test]
fn the_broken_projection_overrides_a_good_picture() {
    // React's sticky broken: even with the asset loadable, a site that
    // errored stays on the puzzle until the icon value changes.
    let rt = rt(Scene {
        wapp_id: Some("acme.bill-audit".into()),
        icon: Some("assets/icon.svg".into()),
        size_pt: 16.0,
        broken: true,
        dark: true,
        wire: ready_picture_wire("datazen://acme.bill-audit/assets/icon.svg"),
    });
    assert!(maybe(&rt, "wapp-icon").is_some());
    assert!(maybe(&rt, "wapp-icon-picture").is_none());
}

#[test]
fn the_puzzle_squares_match_across_themes() {
    // The fallback is theme-independent in shape: same square both themes.
    let mut rects = Vec::new();
    for dark in [true, false] {
        let rt = rt(Scene {
            wapp_id: None,
            icon: None,
            size_pt: 16.0,
            broken: false,
            dark,
            wire: no_wire(),
        });
        let puzzle = maybe(&rt, "wapp-icon").expect("the puzzle renders");
        let [x, y, w, h] = puzzle.rect;
        assert_eq!((w, h), (16.0, 16.0), "the glyph is 16pt (dark={dark})");
        rects.push((x, y));
    }
    assert_eq!(rects[0], rects[1], "only the tint changes between themes");
}
