//! T262 independent audit: fresh vectors, none shared with the coder
//! file. The angles: the map's answered names round-trip through
//! `kind()` (every catalog name answered by the resolver lands as the
//! mounted glyph), the `?` badge's white ink, the placeholder label
//! passing multi-byte and long labels verbatim (the badge never
//! truncates — the resolver's slice is the resolver's), the default
//! resolver's edge ids (`db.` with an empty type, a bare `db` without
//! the dot), a failing picture decode still mounting the blank slot
//! (no onError here, unlike WappIcon), and the status bar's geometry
//! in light mode with a one-sided load.

use std::rc::Rc;
use std::sync::Arc;

use rune::{col, Cx, El, Env, ImageCache, Runtime};

use datazen_ui::components::themed_icon::{ResolvedIcon, ThemedIcon, ThemedIconImages, lucide_map_icon};
use datazen_ui::components::status_bar::StatusBar;
use datazen_ui::ui::badge::BadgeTone;
use datazen_ui::ui::kit::kit;

const W: f32 = 500.0;
const H: f32 = 300.0;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

#[derive(Default)]
struct Scene {
    id: String,
    answer: Option<ResolvedIcon>,
    wire: Option<ThemedIconImages>,
    size: Option<f32>,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut icon = ThemedIcon::<Scene>::new("audit-icon", scene.id.clone());
    if let Some(size) = scene.size {
        icon = icon.size_pt(size);
    }
    if let Some(answer) = scene.answer.clone() {
        icon = icon.resolver(Rc::new(move |_id| answer.clone()));
    }
    if let Some(wire) = scene.wire.clone() {
        icon = icon.images(wire);
    }
    col()
        .key("audit-host")
        .w_full()
        .h_full()
        .child(icon.build(scene.dark))
}

fn mount(scene: Scene) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, (W, H), env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// A wire whose fixture is a genuinely failing decode (garbage bytes).
fn broken_wire(href: &str) -> ThemedIconImages {
    let cache = ImageCache::new(Arc::new(|| {}));
    let image = cache.load(vec![0x7fu8; 16]);
    let failed = image.wait();
    assert!(failed.is_err(), "the garbage fixture must fail to decode");
    let owned = href.to_owned();
    Rc::new(move |asked: &str| (asked == owned).then(|| image.clone()))
}

#[test]
fn every_catalog_name_the_resolver_answers_mounts_a_glyph() {
    // Fresh angle on the map: not "does the map know the names" but
    // "does every answered name actually mount a glyph" — the 20pt
    // tail of the catalog, one mount each.
    for name in [
        "activity", "braces", "cable", "database-backup", "eye", "git-fork", "hash", "globe",
        "key-round", "message-square-text", "monitor", "moon", "mouse-pointer-click", "palette",
        "plug", "refresh-ccw", "server", "shapes", "sun", "table-2",
    ] {
        let scene = Scene {
            id: "ui.any".to_owned(),
            answer: Some(ResolvedIcon::Lucide(name.to_owned())),
            size: Some(20.0),
            dark: true,
            ..Default::default()
        };
        let rt = mount(scene);
        let glyph = maybe(&rt, "audit-icon")
            .unwrap_or_else(|| panic!("{name} mounts its glyph"));
        assert_eq!(glyph.role, "icon", "{name} answers a registered glyph");
        assert_eq!((glyph.rect[2], glyph.rect[3]), (20.0, 20.0), "{name} at h-5 w-5");
        assert_eq!(text_of(&rt, "audit-icon"), "", "a glyph carries no text");
    }
}

#[test]
fn the_q_badge_paints_white_ink_on_the_slate_wash() {
    let scene = Scene {
        id: "ui.unknown".to_owned(),
        answer: Some(ResolvedIcon::Lucide("miss".to_owned())),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    // React 121: `text-white` on `bg-slate-600`.
    let label = maybe(&rt, "audit-icon-label").unwrap();
    assert_eq!(label.text, Some("?".to_owned()));
    assert_eq!(label.text_color, Some(kit(true).c.white));
}

#[test]
fn the_placeholder_label_passes_verbatim_however_long() {
    // The resolver's slice(0, 2) is the RESOLVER's business; the badge
    // renders whatever label it is handed — multi-byte or long.
    for label in ["数据", "abcde", "Ω"] {
        let scene = Scene {
            id: "db.thing".to_owned(),
            answer: Some(ResolvedIcon::Placeholder {
                label: label.to_owned(),
                tone: BadgeTone::Warning,
            }),
            dark: false,
            ..Default::default()
        };
        let rt = mount(scene);
        assert_eq!(text_of(&rt, "audit-icon-label"), label);
    }
}

#[test]
fn the_default_resolver_edges() {
    use datazen_ui::components::themed_icon::default_resolver;
    // `db.` with an empty type: two characters of an empty string.
    assert_eq!(
        default_resolver("db."),
        ResolvedIcon::Placeholder { label: String::new(), tone: BadgeTone::Neutral }
    );
    // A bare `db` without the dot is not a db id: the `?`.
    assert_eq!(
        default_resolver("db"),
        ResolvedIcon::Placeholder { label: "?".to_owned(), tone: BadgeTone::Neutral }
    );
    // The prefix check is exact: `xdb.foo` is not a db id either.
    assert_eq!(
        default_resolver("xdb.foo"),
        ResolvedIcon::Placeholder { label: "?".to_owned(), tone: BadgeTone::Neutral }
    );
}

#[test]
fn a_failing_decode_still_mounts_the_blank_slot() {
    // Unlike WappIcon (whose onError swaps the puzzle in), ThemedIcon
    // has no onError in React — an `<img>` that fails shows nothing,
    // and the element itself stays. A genuinely errored Image from the
    // wire therefore still leaves the slot mounted (blank), the same
    // element a decoding one would paint.
    let scene = Scene {
        id: "ui.logo".to_owned(),
        answer: Some(ResolvedIcon::Url("datazen://pack/broken.png".to_owned())),
        wire: Some(broken_wire("datazen://pack/broken.png")),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    let slot = maybe(&rt, "audit-icon").expect("the slot stays mounted");
    assert_eq!((slot.rect[2], slot.rect[3]), (16.0, 16.0));
    assert_eq!(text_of(&rt, "audit-icon"), "");
}

#[test]
fn a_one_sided_status_bar_keeps_its_geometry_in_light_mode() {
    // Only the left half loaded; the right slot still mounts at the
    // padding edge, and the 40-point bar keeps its height in light
    // mode.
    let scene = Scene { dark: false, ..Default::default() };
    let dark = scene.dark;
    let mut rt = Runtime::new(
        scene,
        |scene: &Scene, _cx: &mut Cx<Scene>| {
            col().key("bar-host").w_full().h_full().child(
                StatusBar::new("audit-bar")
                    .left(
                        rune::text("只有左半边")
                            .key("audit-left")
                            .font(kit(scene.dark).ty.text_xsmall),
                    )
                    .build(scene.dark),
            )
        },
        (W, H),
        env(dark),
    );
    rt.frame(0.0);

    let bar = maybe(&rt, "audit-bar").unwrap();
    assert_eq!(bar.rect[3], 40.0);
    assert_eq!(text_of(&rt, "audit-left"), "只有左半边");
    assert!(maybe(&rt, "audit-bar-right").is_some(), "the empty right slot mounts");
    let right = maybe(&rt, "audit-bar-right").unwrap();
    assert!(
        (right.rect[0] + right.rect[2] - (W - 16.0)).abs() < 1.0,
        "the right edge holds: {:?}",
        right.rect
    );
}
