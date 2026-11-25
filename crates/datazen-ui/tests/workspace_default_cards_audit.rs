//! Independent audit for T111 (the WorkspaceDefaultCards Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below comes from re-reading
//! `packages/frontend/src/windows/workspace/WorkspaceDefaultCards.tsx` and
//! its React suite (`__tests__/WorkspaceDefaultCards.test.tsx`: one card
//! per page with the icon/name/description/version/author stack, the
//! author-less card's bare `v{version}` byline, the card press opening the
//! page, and the empty state's guidance plus the extensions shortcut) —
//! plus the component's own classes for geometry (the `px-6 py-6` body,
//! the `minmax(240px,1fr) gap-3.5` grid, the `gap-2.5 p-4 rounded-xl`
//! card, the `h-10 w-10` icon well, the `pt-2 border-t` byline). Element
//! evidence is `Runtime::elements()` — layout rects, fills, text colours,
//! roles — never a value the component reports about itself; the presses
//! run through the two host slots (the recorded `onOpen(page)` entries and
//! `onOpenExtensions` calls), and the icon src routing is proven by a
//! picture wire that answers nothing and records every ask (the T017
//! audit's trick: only the one contributed icon ever asks). The enabled
//! filter itself is the derivation's gate (workspacePages.ts, T224) — this
//! audit feeds the already-derived list, as the host will.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::wapp_icon::WappIconImages;
use datazen_ui::views::workspace_default_cards::{WorkspaceDefaultCards, CARD_MIN_PT};
use datazen_ui::views::workspace_navigator::WorkspacePageEntry;

/// The window the view mounts in (the workspace's right-hand pane).
const W: f32 = 960.0;
const H: f32 = 640.0;

// ---- the host the audit stands in for ---------------------------------------

/// The workspace host's slice: the derived page list and the two host
/// seams the presses run through.
struct Scene {
    pages: Vec<WorkspacePageEntry>,
    /// The recorded `onOpen(page)` presses.
    opens: Vec<WorkspacePageEntry>,
    /// Whether the host passes the `onOpenExtensions` prop (React renders
    /// the button only when the prop is given).
    extensions_wired: bool,
    /// The recorded `onOpenExtensions` presses.
    extensions_opens: usize,
    dark: bool,
    /// The picture wire: answers nothing (the puzzle renders) and records
    /// every src the cards asked for.
    icon_wire: WappIconImages,
    asks: Rc<RefCell<Vec<String>>>,
}

/// React's fixture row shape (`makeEntry`): the flattened entry the
/// derivation would hand the view.
fn entry(
    key: &str,
    title: &str,
    icon: Option<&str>,
    version: &str,
    author: Option<&str>,
    description: Option<&str>,
) -> WorkspacePageEntry {
    let (wapp_id, page_id) = key.split_once(':').unwrap();
    WorkspacePageEntry {
        key: key.to_owned(),
        wapp_id: wapp_id.to_owned(),
        page_id: page_id.to_owned(),
        title: title.to_owned(),
        icon: icon.map(str::to_owned),
        version: version.to_owned(),
        author: author.map(str::to_owned),
        description: description.map(str::to_owned),
    }
}

/// React test 1's fixture list: the bill-audit wapp's described, authored
/// page and the AFI wapp's bare one (no description, no author, one
/// contributed icon).
fn react_fixture_pages() -> Vec<WorkspacePageEntry> {
    vec![
        entry(
            "acme.bill-audit:quota-check",
            "Quota Check",
            None,
            "1.0.0",
            Some("Acme"),
            Some("Compare bills against quotas"),
        ),
        entry(
            "acme.afi:pricing",
            "Pricing Viewer",
            Some("assets/icon.svg"),
            "2.1.4",
            None,
            None,
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
        opens: Vec::new(),
        extensions_wired: false,
        extensions_opens: 0,
        dark: true,
        icon_wire,
        asks,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut cards = WorkspaceDefaultCards::new(&s.pages);
    if s.extensions_wired {
        cards = cards.on_open_extensions(Rc::new(|s: &mut Scene| s.extensions_opens += 1));
    }
    cards = cards
        .on_open(Rc::new(|s: &mut Scene, page: &WorkspacePageEntry| {
            s.opens.push(page.clone());
        }))
        .images(Rc::clone(&s.icon_wire));
    col().key("probe").w_full().h_full().child(cards.build(s.dark))
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

// ---- the card grid (React tests 1 and 2) -------------------------------------

#[test]
fn the_cards_root_scrolls_the_grid_under_the_heading() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let root = element(&rt, "workspace-default-cards");
    assert_eq!((root.rect[0], root.rect[1]), (0.0, 0.0));
    assert!((root.rect[2] - W).abs() < 0.5, "fills the pane at {W}pt");
    assert_eq!(root.fill.a, 0.0, "the scroll root carries no bg of its own");

    // The px-6 py-6 body: the heading starts at the pane's 24pt inset.
    let body = element(&rt, "workspace-default-cards-body");
    let heading = element(&rt, "workspace-default-cards-title");
    assert_eq!(heading.text.as_deref(), Some(t("workspace.defaultTitle")));
    assert_eq!(heading.text_color, Some(kit(true).c.text_base));
    assert_eq!((heading.rect[0], heading.rect[1]), (24.0, 24.0), "px-6 py-6");
    let hint = element(&rt, "workspace-default-cards-hint");
    assert_eq!(hint.text.as_deref(), Some(t("workspace.defaultHint")));
    assert_eq!(hint.text_color, Some(kit(true).c.text_muted));
    // The heading is above the hint, the hint above the grid (mb-5).
    assert!(hint.rect[1] > heading.rect[1] + heading.rect[3]);
    let grid = element(&rt, "workspace-default-cards-grid");
    assert!(grid.rect[1] > hint.rect[1] + hint.rect[3]);
    assert_eq!(body.rect[1], 0.0, "the body pads inside the scroll root");
}

#[test]
fn one_card_per_page_with_the_page_keys_and_texts() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let card1 = element(&rt, "workspace-default-card-acme.bill-audit:quota-check");
    let card2 = element(&rt, "workspace-default-card-acme.afi:pricing");
    // The `minmax(240px,1fr) gap-3.5` grid: both cards on one line, 240pt
    // wide, 14pt apart, in fixture order.
    assert_eq!((card1.rect[2], card2.rect[2]), (CARD_MIN_PT, CARD_MIN_PT));
    assert!(
        (card2.rect[0] - (card1.rect[0] + CARD_MIN_PT + 14.0)).abs() < 0.5,
        "gap-3.5"
    );
    assert!((card1.rect[1] - card2.rect[1]).abs() < 0.5, "one grid line");
    assert!((card1.rect[0] - 24.0).abs() < 0.5, "the body's px-6");

    // React's `<button type="button">` cards with the `rounded-xl border
    // border-edge bg-surface-alt` chrome.
    for card in [&card1, &card2] {
        assert_eq!(card.role, "button");
        assert_eq!(card.fill, kit(true).c.layer_01, "bg-surface-alt");
        assert!((card.radius - kit(true).radius.lg).abs() < 0.5, "rounded-xl");
    }

    // React's getByText pins: the title, the description, the bylines.
    assert_eq!(
        element(&rt, "workspace-default-card-acme.bill-audit:quota-check-title").text.as_deref(),
        Some("Quota Check")
    );
    assert_eq!(
        element(&rt, "workspace-default-card-acme.bill-audit:quota-check-desc").text.as_deref(),
        Some("Compare bills against quotas")
    );
    assert_eq!(
        element(&rt, "workspace-default-card-acme.bill-audit:quota-check-meta-text").text.as_deref(),
        Some("v1.0.0 · Acme"),
        "the authored byline"
    );
    assert_eq!(
        element(&rt, "workspace-default-card-acme.afi:pricing-meta-text").text.as_deref(),
        Some("v2.1.4"),
        "no author, no separator"
    );
    // The author-less page renders no description span either.
    assert!(
        !has(&rt, "workspace-default-card-acme.afi:pricing-desc"),
        "no description span without a description"
    );
}

#[test]
fn the_card_carries_the_icon_well_and_the_pinned_byline() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let card = element(&rt, "workspace-default-card-acme.bill-audit:quota-check");
    // The `h-10 w-10 rounded-lg bg-surface-raised` well at the p-4 inset.
    let well = element(&rt, "workspace-default-card-acme.bill-audit:quota-check-icon-slot");
    assert_eq!((well.rect[2], well.rect[3]), (40.0, 40.0), "h-10 w-10");
    assert!((well.rect[0] - (card.rect[0] + 16.0)).abs() < 0.5, "p-4");
    assert!((well.rect[1] - (card.rect[1] + 16.0)).abs() < 0.5, "p-4");
    assert_eq!(well.fill, kit(true).c.layer_02, "bg-surface-raised");
    // The `h-5 w-5` icon (the puzzle: the wire answers nothing).
    let icon = element(&rt, "workspace-default-card-acme.bill-audit:quota-check-icon");
    assert_eq!((icon.rect[2], icon.rect[3]), (20.0, 20.0), "h-5 w-5");

    // The byline row sits at the card's bottom edge (mt-auto) with its
    // `pt-2 border-t` above the text line.
    let meta = element(&rt, "workspace-default-card-acme.bill-audit:quota-check-meta");
    let desc = element(&rt, "workspace-default-card-acme.bill-audit:quota-check-desc");
    assert!(meta.rect[1] > desc.rect[1] + desc.rect[3], "under the description");
    // `mt-auto` pins the byline to the CONTENT box bottom — the card's
    // 16pt `p-4` padding still applies below it (React's flexbox does the
    // same).
    assert!(
        (meta.rect[1] + meta.rect[3] - (card.rect[1] + card.rect[3] - 16.0)).abs() < 0.5,
        "mt-auto pins the byline to the card's content bottom: meta {:?} card {:?}",
        meta.rect, card.rect
    );
    assert_eq!(
        element(&rt, "workspace-default-card-acme.bill-audit:quota-check-meta-text").text_color,
        Some(kit(true).c.text_muted),
        "text-[11px] text-fg-muted"
    );
}

#[test]
fn a_card_press_opens_the_corresponding_page() {
    let mut rt = rt_with(scene(react_fixture_pages()), (W, H));
    click(&mut rt, "workspace-default-card-acme.bill-audit:quota-check");
    click(&mut rt, "workspace-default-card-acme.afi:pricing");
    // React's `onOpen` receives the whole entry.
    assert_eq!(
        rt.state.opens.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
        vec!["acme.bill-audit:quota-check", "acme.afi:pricing"],
        "the presses carry the page entries"
    );
    assert_eq!(rt.state.opens[0].wapp_id, "acme.bill-audit");
    assert_eq!(rt.state.opens[0].page_id, "quota-check");
    assert_eq!(rt.state.opens[1].wapp_id, "acme.afi");
    assert_eq!(rt.state.opens[1].page_id, "pricing");
}

#[test]
fn only_the_contributed_icon_asks_for_its_picture() {
    let rt = rt_with(scene(react_fixture_pages()), (W, H));
    let asks = rt.state.asks.borrow();
    assert!(!asks.is_empty(), "the icon-bearing page asked for its picture");
    assert!(
        asks.iter().all(|src| src == "datazen://acme.afi/assets/icon.svg"),
        "only the AFI pricing icon asks (the icon-less page falls back): {asks:?}"
    );
}

// ---- the empty state (React test 3) ------------------------------------------

#[test]
fn the_empty_state_guides_and_opens_extensions() {
    let mut s = scene(Vec::new());
    s.extensions_wired = true;
    let mut rt = rt_with(s, (W, H));
    // No card grid when no page contributes.
    assert!(!has(&rt, "workspace-default-cards"));
    assert!(!has(&rt, "workspace-default-card-acme.bill-audit:quota-check"));
    let empty = element(&rt, "workspace-empty");
    assert_eq!((empty.rect[0], empty.rect[1]), (0.0, 0.0));
    assert!((empty.rect[2] - W).abs() < 0.5);

    let title = element(&rt, "workspace-empty-title");
    assert_eq!(title.text.as_deref(), Some(t("workspace.emptyTitle")));
    assert_eq!(title.text_color, Some(kit(true).c.text_base));
    let hint = element(&rt, "workspace-empty-hint");
    assert_eq!(hint.text.as_deref(), Some(t("workspace.emptyHint")));
    assert_eq!(hint.text_color, Some(kit(true).c.text_muted));
    // items-center: the hint block centers in the pane, capped at max-w-xs.
    let hint_cx = hint.rect[0] + hint.rect[2] / 2.0;
    assert!((hint_cx - W / 2.0).abs() < 0.5, "centered at {hint_cx}");
    assert!(hint.rect[2] <= 320.5, "max-w-xs caps at 320pt");

    // The jump-to-extensions button, under the title/hint stack (gap-3),
    // pressing the slot — and never `onOpen`.
    let button = element(&rt, "workspace-open-extensions");
    assert_eq!(button.label.as_deref(), Some(t("workspace.openExtensions")));
    assert!(button.rect[1] > hint.rect[1] + hint.rect[3], "gap-3 under the hint");
    assert_eq!(rt.state.extensions_opens, 0);
    click(&mut rt, "workspace-open-extensions");
    assert_eq!(rt.state.extensions_opens, 1, "the onOpenExtensions slot fired once");
    assert!(rt.state.opens.is_empty(), "onOpen never fires from the empty state");

    // Without the prop the button does not render (React's
    // `{onOpenExtensions ? … : null}`).
    let rt = rt_with(scene(Vec::new()), (W, H));
    assert!(!has(&rt, "workspace-open-extensions"));
    assert!(has(&rt, "workspace-empty-hint"), "the hint still guides");
}

// ---- the mount tree at both themes -------------------------------------------

#[test]
fn the_mount_tree_geometry_holds_at_both_themes() {
    for (dark, (w, h)) in [(true, (960.0, 640.0)), (false, (1440.0, 816.0))] {
        let mut s = scene(react_fixture_pages());
        s.dark = dark;
        let rt = rt_with(s, (w, h));
        let k = kit(dark);
        let root = element(&rt, "workspace-default-cards");
        assert!((root.rect[2] - w).abs() < 0.5, "fills the pane (dark={dark})");
        let card = element(&rt, "workspace-default-card-acme.bill-audit:quota-check");
        assert_eq!(card.fill, k.c.layer_01, "bg-surface-alt (dark={dark})");
        let well = element(&rt, "workspace-default-card-acme.bill-audit:quota-check-icon-slot");
        assert_eq!(well.fill, k.c.layer_02, "bg-surface-raised (dark={dark})");
        assert_eq!((well.rect[2], well.rect[3]), (40.0, 40.0), "dark={dark}");
        let heading = element(&rt, "workspace-default-cards-title");
        assert_eq!(heading.text_color, Some(k.c.text_base), "dark={dark}");

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
