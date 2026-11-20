//! Independent audit for T145 (the WorkspaceNavigator Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below comes from re-reading
//! `packages/frontend/src/windows/workspace/WorkspaceNavigator.tsx` and its
//! React suite (`__tests__/WorkspaceNavigator.test.tsx`: one item per
//! enabled-wapp page in install order with the `data-page-key` keys, the
//! disabled wapp contributing nothing, the title/description lines, the
//! active tab key's `bg-accent/20` tint, the item press opening the page,
//! and the empty state's hint plus the extensions button) — plus the
//! component's own classes for geometry (the fixed `w-[180px]` rail, the
//! `px-3 pt-3 pb-1` header, the `px-2` nav, the `mb-0.5 gap-2 px-2 py-1.5`
//! items, the `h-4 w-4 mt-0.5` icon). Element evidence is
//! `Runtime::elements()` — layout rects, fills, text colours, help texts —
//! never a value the component reports about itself; the presses run
//! through the two host slots (the recorded `openWappPage(wappId, pageId)`
//! pairs and `onOpenExtensions` calls), and the icon src routing is proven
//! by a picture wire that answers nothing and records every ask (the T017
//! audit's trick: only the one contributed icon ever asks). The enabled
//! filter itself is the derivation's gate (workspacePages.ts, T224) — this
//! audit feeds the already-derived list, as the host will. React's unmount
//! test has no Rune counterpart to pin: the state the host drops *is* the
//! instance.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::wapp_icon::WappIconImages;
use crate::views::workspace_navigator::{WorkspaceNavigator, WorkspacePageEntry, DEFAULT_WIDTH_PT};

/// The window the rail mounts in (the workspace's left edge).
const W: f32 = 960.0;
const H: f32 = 640.0;

// ---- the host the audit stands in for ---------------------------------------

/// The workspace host's slice: the derived page list, the active tab key,
/// the resizable width, and the two host seams the presses run through.
struct Scene {
    pages: Vec<WorkspacePageEntry>,
    active_key: Option<String>,
    width: Option<f32>,
    /// Whether the host passes the `onOpenExtensions` prop (React renders
    /// the button only when the prop is given).
    extensions_wired: bool,
    /// The recorded `openWappPage(wappId, pageId)` presses.
    opens: Vec<(String, String)>,
    /// The recorded `onOpenExtensions` presses.
    extensions_opens: usize,
    dark: bool,
    /// The picture wire: answers nothing (the puzzle renders) and records
    /// every src the items asked for.
    icon_wire: WappIconImages,
    asks: Rc<RefCell<Vec<String>>>,
}

/// React's fixture row shape (`makePlugin`): the flattened entry the
/// derivation would hand the rail.
fn entry(key: &str, title: &str, icon: Option<&str>, description: Option<&str>) -> WorkspacePageEntry {
    let (wapp_id, page_id) = key.split_once(':').unwrap();
    WorkspacePageEntry {
        key: key.to_owned(),
        wapp_id: wapp_id.to_owned(),
        page_id: page_id.to_owned(),
        title: title.to_owned(),
        icon: icon.map(str::to_owned),
        version: "1.0.0".to_owned(),
        author: Some("Acme".to_owned()),
        description: description.map(str::to_owned),
    }
}

/// React test 1's fixture list: the bill-audit wapp's one page, the AFI
/// wapp's two (the first contributing an icon), and the disabled `acme.off`
/// wapp contributing nothing (the derivation's gate, already applied).
fn react_fixture_pages() -> Vec<WorkspacePageEntry> {
    vec![
        entry(
            "acme.bill-audit:quota-check",
            "Quota Check",
            None,
            Some("Compare bills against quotas"),
        ),
        entry(
            "acme.afi:pricing",
            "Pricing Viewer",
            Some("assets/icon.svg"),
            Some("Inspect installment pricing rules"),
        ),
        entry(
            "acme.afi:rules",
            "Rules",
            None,
            Some("Inspect installment pricing rules"),
        ),
    ]
}

fn scene(pages: Vec<WorkspacePageEntry>) -> Scene {
    let asks = Rc::new(RefCell::new(Vec::new()));
    let recorder = Rc::clone(&asks);
    let icon_wire: WappIconImages = Rc::new(move |src: &str| {
        recorder.borrow_mut().push(src.to_owned());
        None
    });
    Scene {
        pages,
        active_key: None,
        width: None,
        extensions_wired: false,
        opens: Vec::new(),
        extensions_opens: 0,
        dark: true,
        icon_wire,
        asks,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut nav = WorkspaceNavigator::new(&s.pages, s.active_key.as_deref());
    if let Some(width) = s.width {
        nav = nav.width(width);
    }
    if s.extensions_wired {
        nav = nav.on_open_extensions(Rc::new(|s: &mut Scene| s.extensions_opens += 1));
    }
    nav = nav
        .on_open_page(Rc::new(|s: &mut Scene, wapp_id: &str, page_id: &str| {
            s.opens.push((wapp_id.to_owned(), page_id.to_owned()));
        }))
        .images(Rc::clone(&s.icon_wire));
    col().key("probe").w_full().h_full().child(nav.build(s.dark))
}

fn rt_with(s: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = s.dark;
    let mut rt = Runtime::new(
        s,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    settle(&mut rt);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

/// Real-time frames: dt=0 frames freeze presence animations at their start,
/// where a freshly-mounted button is not hit-testable.
fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    // Move onto the target first (hit-testing follows the pointer; a down
    // without a prior move lands on nothing), then press.
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

// ---- the header and the rail chrome -----------------------------------------

#[test]
fn the_header_label_pins_the_i18n_key_over_the_fixed_rail() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let root = element(&rt, "workspace-navigator");
    // The fixed `w-[180px]` rail, full height, on the window's left edge.
    assert_eq!((root.rect[0], root.rect[1]), (0.0, 0.0));
    assert!((root.rect[2] - DEFAULT_WIDTH_PT).abs() < 0.5);
    assert!((root.rect[3] - H).abs() < 0.5, "h-full at {H}pt");
    // bg-surface-alt → the kit's layer_01.
    assert_eq!(root.fill, kit(true).c.layer_01);

    let label = element(&rt, "workspace-navigator-label");
    // The authored label (the uppercase is a draw-time transform). The
    // label box starts at the rail's top-left corner (its px-3/pt-3 draw
    // inside), and its height carries the pt-3 + 11pt line + pb-1 stack.
    assert_eq!(label.text.as_deref(), Some(t("nav.workspacePages")));
    assert_eq!(label.text_color, Some(kit(true).c.text_muted));
    assert_eq!((label.rect[0], label.rect[1]), (0.0, 0.0));
    assert!(
        (27.0..33.0).contains(&label.rect[3]),
        "pt-3 + the 11pt line + pb-1 = {}",
        label.rect[3]
    );
}

#[test]
fn the_width_prop_resizes_the_rail() {
    let mut rt = rt_with(scene(react_fixture_pages()), (W, H));
    rt.update(|s, _| s.width = Some(240.0));
    settle(&mut rt);
    let root = element(&rt, "workspace-navigator");
    assert!((root.rect[2] - 240.0).abs() < 0.5, "the host's width wins");
    // The items re-flow to the wider rail (240 − nav px-2 × 2).
    let item = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check");
    assert!((item.rect[2] - 224.0).abs() < 0.5);
}

// ---- the item list (React tests 1, 2 and 3) ----------------------------------

#[test]
fn one_item_per_page_in_install_order_with_the_page_keys() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    // React's `data-page-key` order, merged into the item keys.
    let keys = [
        "workspace-nav-item-acme.bill-audit:quota-check",
        "workspace-nav-item-acme.afi:pricing",
        "workspace-nav-item-acme.afi:rules",
    ];
    let items: Vec<_> = keys.iter().map(|key| element(&rt, key)).collect();
    // Install order is vertical order (the nav stacks them).
    for pair in items.windows(2) {
        assert!(pair[0].rect[1] < pair[1].rect[1], "the fixture order holds");
    }
    // The `mb-0.5` stack: each item starts 2pt under the one above.
    assert!(
        (items[1].rect[1] - (items[0].rect[1] + items[0].rect[3] + 2.0)).abs() < 0.5,
        "mb-0.5"
    );

    // The title and description lines (React's getByText pins).
    assert_eq!(
        element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-title")
            .text
            .as_deref(),
        Some("Quota Check")
    );
    assert_eq!(
        element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-desc")
            .text
            .as_deref(),
        Some("Compare bills against quotas")
    );
    assert_eq!(
        element(&rt, "workspace-nav-item-acme.afi:pricing-title")
            .text
            .as_deref(),
        Some("Pricing Viewer")
    );
    assert_eq!(
        element(&rt, "workspace-nav-item-acme.afi:rules-title").text.as_deref(),
        Some("Rules")
    );
    // The wapp's own name never shows (the page title wins).
    for e in rt.elements() {
        assert!(
            e.text.as_deref() != Some("Bill Audit"),
            "the wapp name is not the item title"
        );
    }

    // The rail chrome: the nav fills under the header, its px-2 gutter.
    let label = element(&rt, "workspace-navigator-label");
    let nav = element(&rt, "workspace-navigator-nav");
    assert!((nav.rect[1] - (label.rect[1] + label.rect[3])).abs() < 0.5, "under the label");
    assert!((nav.rect[3] - (H - nav.rect[1])).abs() < 0.5, "flex-1 to the bottom");
    let item = &items[0];
    assert!((item.rect[0] - 8.0).abs() < 0.5, "the nav's px-2");
    assert!((item.rect[2] - (DEFAULT_WIDTH_PT - 16.0)).abs() < 0.5, "w-full in the gutter");
}

#[test]
fn the_item_rows_carry_the_button_role_and_the_tooltip() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let item = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check");
    // React's `<button type="button">`.
    assert_eq!(item.role, "button");
    // React: `title={page.description ?? page.title}` — the description.
    assert_eq!(item.help.as_deref(), Some("Compare bills against quotas"));
    // The icon leads the text: h-4 w-4 at the px-2 gutter, mt-0.5 under the
    // py-1.5 top, then the gap-2 column.
    let icon = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-icon");
    assert_eq!((icon.rect[2], icon.rect[3]), (16.0, 16.0), "h-4 w-4");
    assert!((icon.rect[0] - 16.0).abs() < 0.5, "nav px-2 + item px-2");
    assert!((icon.rect[1] - (item.rect[1] + 8.0)).abs() < 0.5, "py-1.5 + mt-0.5");
    let title = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-title");
    assert!((title.rect[0] - (icon.rect[0] + 16.0 + 8.0)).abs() < 0.5, "gap-2");
    assert!((title.rect[1] - (item.rect[1] + 6.0)).abs() < 0.5, "py-1.5, items-start");
    let desc = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-desc");
    assert!(desc.rect[1] > title.rect[1], "the description is the second line");
    assert_eq!(desc.text_color, Some(kit(true).c.text_muted));

    // A page without a description: no second line, the tooltip falls back
    // to the title (React's `??`).
    let s = scene(vec![
        entry(
            "acme.bill-audit:quota-check",
            "Quota Check",
            None,
            Some("Compare bills against quotas"),
        ),
        entry("acme.afi:rules", "Rules", None, None),
    ]);
    let rt = rt_with(s, (W, H));
    assert!(
        !has(&rt, "workspace-nav-item-acme.afi:rules-desc"),
        "no description span without a description"
    );
    assert_eq!(
        element(&rt, "workspace-nav-item-acme.afi:rules").help.as_deref(),
        Some("Rules"),
        "title={{page.description ?? page.title}}"
    );
    // The description-less row is the shorter one.
    let with_desc = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check");
    let without = element(&rt, "workspace-nav-item-acme.afi:rules");
    assert!(without.rect[3] < with_desc.rect[3]);
}

#[test]
fn the_active_item_tints_and_the_rest_stay_plain() {
    let mut s = scene(react_fixture_pages());
    s.active_key = Some("acme.bill-audit:quota-check".to_owned());
    let rt = rt_with(s, (W, H));
    let k = kit(true);
    let active = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check");
    // React: `bg-accent/20 text-accent`.
    assert_eq!(active.fill, k.alpha(k.c.bg_accent, 0.2), "bg-accent/20");
    assert_eq!(
        element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-title").text_color,
        Some(k.c.text_accent),
        "text-accent"
    );
    // The inactive rows carry no fill of their own and secondary ink.
    for key in ["workspace-nav-item-acme.afi:pricing", "workspace-nav-item-acme.afi:rules"] {
        let item = element(&rt, key);
        assert_eq!(item.fill.a, 0.0, "{key}: no inactive fill");
        let title = element(&rt, &format!("{key}-title"));
        assert_eq!(title.text_color, Some(k.c.text_muted), "{key}: text-fg-secondary");
    }
}

#[test]
fn a_press_opens_the_corresponding_page() {
    let mut rt = rt_with(scene(react_fixture_pages()), (W, H));
    click(&mut rt, "workspace-nav-item-acme.bill-audit:quota-check");
    click(&mut rt, "workspace-nav-item-acme.afi:rules");
    assert_eq!(
        rt.state.opens,
        vec![
            ("acme.bill-audit".to_owned(), "quota-check".to_owned()),
            ("acme.afi".to_owned(), "rules".to_owned()),
        ],
        "the presses carry the wapp and page ids"
    );
}

#[test]
fn only_the_contributed_icon_asks_for_its_picture() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let asks = rt.state.asks.borrow();
    assert!(!asks.is_empty(), "the icon-bearing page asked for its picture");
    assert!(
        asks.iter().all(|src| src == "datazen://acme.afi/assets/icon.svg"),
        "only the AFI pricing icon asks (the icon-less pages fall back): {asks:?}"
    );
}

// ---- the empty state (React test 4) ------------------------------------------

#[test]
fn the_empty_state_guides_and_opens_extensions() {
    let mut s = scene(Vec::new());
    s.extensions_wired = true;
    let mut rt = rt_with(s, (W, H));
    // No nav list when no page contributes.
    assert!(!has(&rt, "workspace-navigator-nav"));
    let empty = element(&rt, "workspace-navigator-empty");
    let hint = element(&rt, "workspace-navigator-empty-hint");
    assert_eq!(hint.text.as_deref(), Some(t("workspace.emptyHint")));
    assert_eq!(hint.text_color, Some(kit(true).c.text_muted));
    // items-center: the hint block centers in the rail's content box.
    let hint_cx = hint.rect[0] + hint.rect[2] / 2.0;
    assert!((hint_cx - DEFAULT_WIDTH_PT / 2.0).abs() < 0.5, "centered at {hint_cx}");
    assert!(empty.rect[1] > element(&rt, "workspace-navigator-label").rect[1]);

    // The jump-to-extensions button, under the hint, pressing the slot.
    let button = element(&rt, "workspace-open-extensions");
    assert_eq!(button.label.as_deref(), Some(t("workspace.openExtensions")));
    assert!(button.rect[1] > hint.rect[1] + hint.rect[3], "gap-3 under the hint");
    assert_eq!(rt.state.extensions_opens, 0);
    click(&mut rt, "workspace-open-extensions");
    assert_eq!(rt.state.extensions_opens, 1, "the onOpenExtensions slot fired once");

    // Without the prop the button does not render (React's
    // `{onOpenExtensions ? … : null}`).
    let rt = rt_with(scene(Vec::new()), (W, H));
    assert!(!has(&rt, "workspace-open-extensions"));
    assert!(has(&rt, "workspace-navigator-empty-hint"), "the hint still guides");
}

// ---- the mount tree at both themes -------------------------------------------

#[test]
fn the_mount_tree_geometry_holds_at_both_themes() {
    for (dark, (w, h)) in [(true, (960.0, 640.0)), (false, (1440.0, 816.0))] {
        let mut s = scene(react_fixture_pages());
        s.dark = dark;
        let rt = rt_with(s, (w, h));
        let k = kit(dark);
        let root = element(&rt, "workspace-navigator");
        assert_eq!(root.fill, k.c.layer_01, "bg-surface-alt (dark={dark})");
        assert!((root.rect[3] - h).abs() < 0.5, "h-full at {h}pt (dark={dark})");

        let label = element(&rt, "workspace-navigator-label");
        assert_eq!(label.text_color, Some(k.c.text_muted), "dark={dark}");
        let nav = element(&rt, "workspace-navigator-nav");
        assert!((nav.rect[1] - (label.rect[1] + label.rect[3])).abs() < 0.5, "dark={dark}");

        // The first item's chrome at the gutter.
        let item = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check");
        assert!((item.rect[0] - 8.0).abs() < 0.5, "dark={dark}");
        assert!((item.rect[2] - (DEFAULT_WIDTH_PT - 16.0)).abs() < 0.5, "dark={dark}");
        let icon = element(&rt, "workspace-nav-item-acme.bill-audit:quota-check-icon");
        assert_eq!((icon.rect[2], icon.rect[3]), (16.0, 16.0), "dark={dark}");

        // Nothing escapes the window.
        for e in rt.elements() {
            if e.exiting {
                continue;
            }
            let [ex, ey, ew, eh] = e.rect;
            assert!(
                ex >= 0.0 && ey >= 0.0 && ex + ew <= w + 0.5 && ey + eh <= h + 0.5,
                "{} at {ex},{ey} {ew}x{eh} escapes {w}x{h} (dark={dark})",
                e.key.clone().unwrap_or_else(|| "?".to_owned())
            );
        }
    }
}
