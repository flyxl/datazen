//! T262: the themed icon and the status bar — coder tests.
//!
//! ThemedIcon: the LUCIDE_MAP catalog (all 36 React names resolve, a
//! made-up name does not), the default resolver's module-level
//! degradations (a `db.*` id to its two-character tile, everything else
//! to the `?`), the mounted lucide branch (the glyph at its size), the
//! unknown-name `?` badge, the fallback prop winning over the badge, the
//! placeholder badge with its tone's fill, the url branch's picture slot
//! (ready or blank), and the sizes.
//!
//! StatusBar: both slots mount with the geometry (the 40-point bar, the
//! left half shrinking, the right half fixed at the right edge), and
//! the empty slots still mount.

use std::rc::Rc;
use std::sync::Arc;

use rune::{col, Cx, El, Env, ImageCache, Runtime};

use crate::components::db_type_badge::badge_tone_fill;
use crate::components::themed_icon::{ResolvedIcon, ThemedIcon, ThemedIconImages, lucide_map_icon};
use crate::components::status_bar::StatusBar;
use crate::ui::badge::BadgeTone;
use crate::ui::kit::kit;

const W: f32 = 400.0;
const H: f32 = 200.0;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

// ---------------------------------------------------------------------------
// The pure half
// ---------------------------------------------------------------------------

#[test]
fn lucide_map_icon_covers_the_react_catalog() {
    // React's LUCIDE_MAP: the 36 lucide names the host semantic catalog
    // references, exactly.
    let catalog = [
        "activity", "braces", "bot", "cable", "code-2", "database", "database-backup",
        "download", "eye", "file-text", "folder-open", "git-fork", "gauge", "hash", "globe",
        "key-round", "layout-grid", "message-square-text", "monitor", "moon",
        "mouse-pointer-click", "palette", "play", "plug", "plus", "puzzle", "refresh-ccw",
        "server", "shapes", "settings", "square", "sun", "table-2", "table-properties",
        "zap", "workflow",
    ];
    assert_eq!(catalog.len(), 36, "the React map's size");
    for name in catalog {
        assert!(lucide_map_icon(name).is_some(), "the map resolves {name}");
    }
    assert!(lucide_map_icon("not-a-lucide-name").is_none());
}

#[test]
fn the_default_resolver_degrades_db_ids_and_everything_else() {
    // The module-level seed: empty maps, `placeholderForDb` = the db
    // type's first two characters on slate, everything else the `?`.
    use crate::components::themed_icon::default_resolver;
    assert_eq!(
        default_resolver("db.postgresql"),
        ResolvedIcon::Placeholder { label: "po".to_owned(), tone: BadgeTone::Neutral }
    );
    assert_eq!(
        default_resolver("db.x"),
        ResolvedIcon::Placeholder { label: "x".to_owned(), tone: BadgeTone::Neutral }
    );
    assert_eq!(
        default_resolver("ui.settings"),
        ResolvedIcon::Placeholder { label: "?".to_owned(), tone: BadgeTone::Neutral }
    );
    assert_eq!(
        default_resolver(""),
        ResolvedIcon::Placeholder { label: "?".to_owned(), tone: BadgeTone::Neutral }
    );
}

// ---------------------------------------------------------------------------
// ThemedIcon: the mounted half
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Scene {
    id: String,
    answer: Option<ResolvedIcon>,
    fallback: Option<&'static rune::Icon>,
    wire: Option<ThemedIconImages>,
    size: Option<f32>,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut icon = ThemedIcon::<Scene>::new("themed-icon", scene.id.clone());
    if let Some(size) = scene.size {
        icon = icon.size_pt(size);
    }
    if let Some(answer) = scene.answer.clone() {
        icon = icon.resolver(Rc::new(move |_id| answer.clone()));
    }
    if let Some(fallback) = scene.fallback {
        icon = icon.fallback_icon(fallback);
    }
    if let Some(wire) = scene.wire.clone() {
        icon = icon.images(wire);
    }
    col()
        .key("icon-host")
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

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// A picture wire whose one fixture is a decoded 8×8 opaque square (the
/// wapp-icon audit's pattern).
fn ready_wire(href: &str) -> ThemedIconImages {
    let cache = ImageCache::new(Arc::new(|| {}));
    let image = cache.rgba(8, 8, vec![120u8; 8 * 8 * 4]);
    let size = image.wait().expect("the RGBA fixture decodes");
    assert_eq!(size, [8, 8]);
    let owned = href.to_owned();
    Rc::new(move |asked: &str| (asked == owned).then(|| image.clone()))
}

fn no_wire() -> ThemedIconImages {
    Rc::new(|_href: &str| None)
}

#[test]
fn the_lucide_branch_mounts_the_map_icon_at_its_size() {
    let scene = Scene {
        id: "ui.browse".to_owned(),
        answer: Some(ResolvedIcon::Lucide("database".to_owned())),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    let icon = maybe(&rt, "themed-icon").expect("the glyph is keyed");
    assert_eq!(icon.role, "icon", "the map's glyph mounts");
    assert_eq!((icon.rect[2], icon.rect[3]), (16.0, 16.0), "the default 16pt size");

    let scene = Scene {
        id: "ui.browse".to_owned(),
        answer: Some(ResolvedIcon::Lucide("database".to_owned())),
        size: Some(14.0),
        dark: false,
        ..Default::default()
    };
    let rt = mount(scene);
    assert_eq!(
        (maybe(&rt, "themed-icon").unwrap().rect[2], maybe(&rt, "themed-icon").unwrap().rect[3]),
        (14.0, 14.0),
        "h-3.5 w-3.5's 14pt"
    );
}

#[test]
fn an_unknown_lucide_name_falls_to_the_q_badge() {
    // React 113–128: the map misses, no fallback prop — the slate `?`.
    let scene = Scene {
        id: "ui.unknown".to_owned(),
        answer: Some(ResolvedIcon::Lucide("not-a-lucide-name".to_owned())),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    let badge = maybe(&rt, "themed-icon").expect("the badge is keyed");
    assert_eq!(text_of(&rt, "themed-icon-label"), "?");
    assert_eq!(badge.fill, badge_tone_fill(BadgeTone::Neutral, true));
}

#[test]
fn the_fallback_icon_prop_wins_over_the_q_badge() {
    // React 114–116: `LUCIDE_MAP[resolved.name] ?? fallback` — the
    // fallback glyph replaces the `?` badge.
    let scene = Scene {
        id: "ui.unknown".to_owned(),
        answer: Some(ResolvedIcon::Lucide("not-a-lucide-name".to_owned())),
        fallback: Some(&crate::icons::ICON_WAPPS),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    let icon = maybe(&rt, "themed-icon").expect("the fallback glyph is keyed");
    assert_eq!(icon.role, "icon", "the fallback is a glyph, not the badge");
    assert_ne!(text_of(&rt, "themed-icon"), "?");
}

#[test]
fn the_placeholder_branch_mounts_the_label_on_its_tone() {
    let scene = Scene {
        id: "db.postgresql".to_owned(),
        answer: Some(ResolvedIcon::Placeholder {
            label: "Pg".to_owned(),
            tone: BadgeTone::Accent,
        }),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    assert_eq!(text_of(&rt, "themed-icon-label"), "Pg");
    assert_eq!(
        maybe(&rt, "themed-icon").unwrap().fill,
        badge_tone_fill(BadgeTone::Accent, true)
    );
}

#[test]
fn the_url_branch_mounts_the_picture_slot_ready_or_blank() {
    // A ready picture: the slot mounts at its size (the pixels the host
    // wired paint inside it).
    let scene = Scene {
        id: "ui.logo".to_owned(),
        answer: Some(ResolvedIcon::Url("datazen://pack/logo.png".to_owned())),
        wire: Some(ready_wire("datazen://pack/logo.png")),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    let picture = maybe(&rt, "themed-icon").expect("the picture slot is keyed");
    assert_eq!((picture.rect[2], picture.rect[3]), (16.0, 16.0));

    // A wire with no bytes yet: the slot still mounts, blank — exactly
    // the `<img>` before its bytes arrive (no onError in this
    // component, React has none either).
    let scene = Scene {
        id: "ui.logo".to_owned(),
        answer: Some(ResolvedIcon::Url("datazen://pack/logo.png".to_owned())),
        wire: Some(no_wire()),
        dark: true,
        ..Default::default()
    };
    let rt = mount(scene);
    assert!(has_key(&rt, "themed-icon"), "the blank slot mounts");
    assert_eq!(maybe(&rt, "themed-icon").unwrap().rect[2], 16.0);
}

#[test]
fn the_default_resolver_drives_the_mounted_badge() {
    // Without the resolver prop the default resolver's answers mount:
    // the two-character db tile and the `?`.
    let scene = Scene { id: "db.mysql".to_owned(), dark: true, ..Default::default() };
    let rt = mount(scene);
    assert_eq!(text_of(&rt, "themed-icon-label"), "my");

    let scene = Scene { id: "db.m".to_owned(), dark: true, ..Default::default() };
    let rt = mount(scene);
    assert_eq!(text_of(&rt, "themed-icon-label"), "m");

    let scene = Scene { id: "ui.anything".to_owned(), dark: true, ..Default::default() };
    let rt = mount(scene);
    assert_eq!(text_of(&rt, "themed-icon-label"), "?");
}

// ---------------------------------------------------------------------------
// StatusBar
// ---------------------------------------------------------------------------

fn status_view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let bar = StatusBar::new("status-bar");
    let bar = if scene.id.is_empty() {
        bar
    } else {
        bar.left(
            rune::text("LEFT 状态")
                .key("status-bar-left-text")
                .font(kit(scene.dark).ty.text_xsmall),
        )
    };
    let bar = if scene.fallback.is_some() {
        bar.right(
            rune::text("RIGHT")
                .key("status-bar-right-text")
                .font(kit(scene.dark).ty.text_xsmall),
        )
    } else {
        bar
    };
    col()
        .key("bar-host")
        .w_full()
        .h_full()
        .child(bar.build(scene.dark))
}

fn mount_status(scene: Scene) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, status_view, (W, H), env(dark));
    rt.frame(0.0);
    rt
}

#[test]
fn the_status_bar_mounts_both_slots_with_the_geometry() {
    let scene = Scene {
        id: "left".to_owned(),
        fallback: Some(&crate::icons::ICON_ZAP),
        dark: true,
        ..Default::default()
    };
    let rt = mount_status(scene);

    assert!(has_key(&rt, "status-bar"));
    assert!(has_key(&rt, "status-bar-left"));
    assert!(has_key(&rt, "status-bar-right"));
    assert_eq!(text_of(&rt, "status-bar-left-text"), "LEFT 状态");
    assert_eq!(text_of(&rt, "status-bar-right-text"), "RIGHT");

    // React: `h-10 min-h-[40px]`.
    let bar = maybe(&rt, "status-bar").unwrap();
    assert_eq!(bar.rect[3], 40.0, "the 40-point footer");
    // React: `px-4` — the slots sit inside the 16-point padding.
    let right = maybe(&rt, "status-bar-right").unwrap();
    assert!(
        (right.rect[0] + right.rect[2] - (W - 16.0)).abs() < 1.0,
        "the fixed half ends at the padding edge: {:?}",
        right.rect
    );
}

#[test]
fn the_status_bar_slots_mount_even_when_empty() {
    // React renders both divs whatever the props; an empty slot paints
    // nothing but still exists.
    let scene = Scene { dark: false, ..Default::default() };
    let rt = mount_status(scene);
    assert!(has_key(&rt, "status-bar"));
    assert!(has_key(&rt, "status-bar-left"));
    assert!(has_key(&rt, "status-bar-right"));
    assert_eq!(text_of(&rt, "status-bar-left"), "");
    assert_eq!(text_of(&rt, "status-bar-right"), "");
}
