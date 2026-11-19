//! Independent audit for T174 (the WorkspaceTabBar Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below comes from re-reading
//! `packages/frontend/src/windows/workspace/WorkspaceTabBar.tsx` and its
//! React suite (`__tests__/WorkspaceTabBar.test.tsx`: nothing rendered
//! while no tab is open, one entry per open tab, the entry press
//! activating its key, the close button closing it) — plus the component's
//! own classes for geometry (the `border-b bg-surface-alt` bar, the
//! `border-r px-3 py-2 gap-1.5` entries, the `h-3.5 w-3.5` icon, the
//! `max-w-[160px] truncate` title, the `p-0.5` close, the `h-0.5`
//! accent underline) and its `title={`${tab.title} · v${tab.version}`}`
//! tooltips and `aria-label` close labels. Element evidence is
//! `Runtime::elements()` — layout rects, fills, text colours, roles,
//! labels, help texts — never a value the component reports about itself;
//! the presses run through the two store slots (the recorded
//! `activate(key)` / `close(key)` calls). The workspace-tabs store itself
//! (open/close/activate's neighbor rules) is T143/T172's port — this audit
//! feeds the tab list and active key, as the host will. The close button's
//! group reveal (`opacity-0 group-hover:opacity-100`) is pinned by the
//! press journey: the pointer's move onto the entry is the hover that
//! reveals it, and only then does the press land.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::wapp_icon::WappIconImages;
use crate::views::wapp_page_shell::WorkspaceTabVm;
use crate::views::workspace_tab_bar::WorkspaceTabBar;

/// The window the strip mounts in (the workspace's top edge).
const W: f32 = 960.0;
const H: f32 = 640.0;

// ---- the host the audit stands in for ---------------------------------------

/// The workspace host's slice: the tab list, the active key and the two
/// store seams the presses run through.
struct Scene {
    tabs: Vec<WorkspaceTabVm>,
    active_key: Option<String>,
    /// The recorded `activate(key)` presses.
    activates: Vec<String>,
    /// The recorded `close(key)` presses.
    closes: Vec<String>,
    dark: bool,
    /// The picture wire: answers nothing (the puzzle renders) and records
    /// every src the entries asked for.
    icon_wire: WappIconImages,
    asks: Rc<RefCell<Vec<String>>>,
}

/// React's fixture tab shape (`makeTab`).
fn tab(key: &str, title: &str, icon: Option<&str>) -> WorkspaceTabVm {
    let (wapp_id, page_id) = key.split_once(':').unwrap();
    WorkspaceTabVm {
        key: key.to_owned(),
        wapp_id: wapp_id.to_owned(),
        page_id: page_id.to_owned(),
        title: title.to_owned(),
        icon: icon.map(str::to_owned),
        version: "1.0.0".to_owned(),
    }
}

/// React test 2's fixture list: the bill-audit wapp's page and the AFI
/// wapp's pricing page (the latter contributing an icon).
fn react_fixture_tabs() -> Vec<WorkspaceTabVm> {
    vec![
        tab("acme.bill-audit:quota-check", "Quota Check", None),
        tab("acme.afi:pricing", "Pricing", Some("assets/icon.svg")),
    ]
}

fn scene(tabs: Vec<WorkspaceTabVm>) -> Scene {
    let asks = Rc::new(RefCell::new(Vec::new()));
    let recorder = Rc::clone(&asks);
    let icon_wire: WappIconImages = Rc::new(move |src: &str| {
        recorder.borrow_mut().push(src.to_owned());
        None
    });
    Scene {
        tabs,
        active_key: None,
        activates: Vec::new(),
        closes: Vec::new(),
        dark: true,
        icon_wire,
        asks,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut probe = col().key("probe").w_full().h_full();
    if let Some(bar) = WorkspaceTabBar::new(&s.tabs, s.active_key.as_deref())
        .on_activate(Rc::new(|s: &mut Scene, key: &str| {
            s.activates.push(key.to_owned());
        }))
        .on_close(Rc::new(|s: &mut Scene, key: &str| {
            s.closes.push(key.to_owned());
        }))
        .images(Rc::clone(&s.icon_wire))
        .build(s.dark)
    {
        probe = probe.child(bar);
    }
    probe
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

/// The close button's journey: its group reveal (`opacity-0
/// group-hover:opacity-100`) means the pointer's move onto the entry — the
/// button sits inside it — is the hover that reveals it, so the press lands
/// only after the reveal has settled.
fn click_revealed(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    settle(rt);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

// ---- the nothing-rendered gate (React test 1) ---------------------------------

#[test]
fn nothing_renders_while_no_tab_is_open() {
    let rt = rt_with(scene(Vec::new()), (W, H));
    assert!(has(&rt, "probe"), "the probe window is laid out");
    assert!(!has(&rt, "workspace-tabbar"), "React returns null");
    assert!(!has(&rt, "workspace-tabbar-strip"));
}

// ---- the entry list (React test 2) -------------------------------------------

#[test]
fn one_entry_per_open_tab_with_the_tab_keys_and_tooltips() {
    let mut s = scene(react_fixture_tabs());
    s.active_key = Some("acme.afi:pricing".to_owned());
    let rt = rt_with(s, (W, H));
    let e1 = element(&rt, "workspace-tab-acme.bill-audit:quota-check");
    let e2 = element(&rt, "workspace-tab-acme.afi:pricing");
    // The strip lays the entries out side by side, in store order.
    assert!(e1.rect[0] < e2.rect[0], "the fixture order holds");
    assert!((e1.rect[1] - e2.rect[1]).abs() < 0.5, "one bar line");
    assert!((e1.rect[3] - e2.rect[3]).abs() < 0.5, "same-height entries");
    assert!((e2.rect[0] - (e1.rect[0] + e1.rect[2])).abs() < 0.5, "border-r to border-r");

    // React's getByText pins and the `title` tooltips.
    assert_eq!(
        element(&rt, "workspace-tab-acme.bill-audit:quota-check-title").text.as_deref(),
        Some("Quota Check")
    );
    assert_eq!(
        element(&rt, "workspace-tab-acme.afi:pricing-title").text.as_deref(),
        Some("Pricing")
    );
    assert_eq!(
        e1.help.as_deref(),
        Some("Quota Check · v1.0.0"),
        "the tooltip template: title, middot, v(version)"
    );
    assert_eq!(e2.help.as_deref(), Some("Pricing · v1.0.0"));

    // The activate control is the button React's click lands on.
    assert_eq!(element(&rt, "workspace-tab-acme.bill-audit:quota-check-activate").role, "button");
}

#[test]
fn the_bar_chrome_and_entry_geometry_hold() {
    let mut s = scene(react_fixture_tabs());
    s.active_key = Some("acme.afi:pricing".to_owned());
    let rt = rt_with(s, (W, H));
    let k = kit(true);
    let root = element(&rt, "workspace-tabbar");
    assert_eq!((root.rect[0], root.rect[1]), (0.0, 0.0), "the pane's top edge");
    assert!((root.rect[2] - W).abs() < 0.5, "fills the pane at {W}pt");
    assert_eq!(root.fill, k.c.layer_01, "bg-surface-alt");
    let strip = element(&rt, "workspace-tabbar-strip");
    assert!((strip.rect[2] - W).abs() < 0.5, "flex-1");

    // The entries set the bar's height (shrink-0 bar, natural strip).
    let e1 = element(&rt, "workspace-tab-acme.bill-audit:quota-check");
    assert!((root.rect[3] - e1.rect[3]).abs() < 0.5, "the bar wraps the entries");

    // The px-3 py-2 entry: the 14pt icon leads at the 8pt inset, then the
    // gap-1.5 title column.
    let icon = element(&rt, "workspace-tab-acme.bill-audit:quota-check-icon");
    assert_eq!((icon.rect[2], icon.rect[3]), (14.0, 14.0), "h-3.5 w-3.5");
    assert!((icon.rect[0] - 12.0).abs() < 0.5, "px-3");
    // `py-2` puts the content row 8pt in; the 14pt icon centers inside the
    // title's 16pt line box (text-xs leading), 1pt lower than the row's
    // top — exactly React's items-center math.
    assert!((icon.rect[1] - (e1.rect[1] + 8.0) - 1.0).abs() < 0.75, "py-2: icon {:?} e1 {:?}", icon.rect, e1.rect);
    let title = element(&rt, "workspace-tab-acme.bill-audit:quota-check-title");
    assert!((title.rect[0] - (icon.rect[0] + 14.0 + 6.0)).abs() < 0.5, "gap-1.5");
    assert!(title.rect[2] <= 160.5, "max-w-[160px]");

    // The close button follows at the gap, `p-0.5` around the 12pt X.
    let close = element(&rt, "workspace-tab-acme.bill-audit:quota-check-close");
    assert!((close.rect[0] - (title.rect[0] + title.rect[2] + 6.0)).abs() < 0.5, "gap-1.5");
    assert!((close.rect[2] - 16.0).abs() < 0.5 && (close.rect[3] - 16.0).abs() < 0.5, "p-0.5 + h-3 w-3");
    assert_eq!(close.role, "button");

    // The 2pt underline spans the entry's bottom edge.
    let underline = element(&rt, "workspace-tab-acme.bill-audit:quota-check-underline");
    assert!(
        (underline.rect[1] - (e1.rect[1] + e1.rect[3] - 2.0)).abs() < 0.5
            && (underline.rect[3] - 2.0).abs() < 0.5
            && (underline.rect[2] - e1.rect[2]).abs() < 0.5,
        "absolute inset-x-0 bottom-0 h-0.5, in flow"
    );
}

#[test]
fn the_active_entry_tints_and_carries_the_underline() {
    let mut s = scene(react_fixture_tabs());
    s.active_key = Some("acme.afi:pricing".to_owned());
    let rt = rt_with(s, (W, H));
    let k = kit(true);
    // React: `bg-surface text-fg` + `opacity-100` on the active entry.
    let active = element(&rt, "workspace-tab-acme.afi:pricing");
    assert_eq!(active.fill, k.c.bg_base, "bg-surface");
    assert_eq!(
        element(&rt, "workspace-tab-acme.afi:pricing-title").text_color,
        Some(k.c.text_base),
        "text-fg"
    );
    assert_eq!(
        element(&rt, "workspace-tab-acme.afi:pricing-underline").fill,
        k.c.bg_accent,
        "the accent underline"
    );
    // The inactive entry: no fill of its own, secondary ink, transparent
    // underline (always present, recolored — not absent).
    let inactive = element(&rt, "workspace-tab-acme.bill-audit:quota-check");
    assert_eq!(inactive.fill.a, 0.0, "no inactive fill");
    assert_eq!(
        element(&rt, "workspace-tab-acme.bill-audit:quota-check-title").text_color,
        Some(k.c.text_muted),
        "text-fg-secondary"
    );
    assert_eq!(
        element(&rt, "workspace-tab-acme.bill-audit:quota-check-underline").fill.a,
        0.0,
        "opacity-0"
    );
}

// ---- the two store actions (React tests 3 and 4) ------------------------------

#[test]
fn a_press_activates_the_tab() {
    let mut s = scene(react_fixture_tabs());
    s.active_key = Some("acme.afi:pricing".to_owned());
    let mut rt = rt_with(s, (W, H));
    click(&mut rt, "workspace-tab-acme.bill-audit:quota-check-activate");
    assert_eq!(
        rt.state.activates,
        vec!["acme.bill-audit:quota-check".to_owned()],
        "the press carries the tab key"
    );
    assert!(rt.state.closes.is_empty(), "an entry press never closes");
}

#[test]
fn the_close_button_closes_its_tab_with_the_common_label() {
    let rt = rt_with(scene(react_fixture_tabs()), (W, H));
    // React: `aria-label={`${t('common.close')} ${tab.title}`}`.
    assert_eq!(
        element(&rt, "workspace-tab-acme.bill-audit:quota-check-close").label.as_deref(),
        Some(format!("{} {}", t("common.close"), "Quota Check").as_str()),
        "the aria-label names the tab it closes"
    );

    // The reveal journey: the move onto the entry hovers the group, the
    // button appears, and the press closes the tab.
    let mut rt = rt_with(scene(react_fixture_tabs()), (W, H));
    click_revealed(&mut rt, "workspace-tab-acme.bill-audit:quota-check-close");
    assert_eq!(
        rt.state.closes,
        vec!["acme.bill-audit:quota-check".to_owned()],
        "the close press carries the tab key"
    );
    assert!(rt.state.activates.is_empty(), "a close press never activates");
}

#[test]
fn only_the_contributed_icon_asks_for_its_picture() {
    let rt = rt_with(scene(react_fixture_tabs()), (W, H));
    let asks = rt.state.asks.borrow();
    assert!(!asks.is_empty(), "the icon-bearing tab asked for its picture");
    assert!(
        asks.iter().all(|src| src == "datazen://acme.afi/assets/icon.svg"),
        "only the AFI pricing icon asks (the icon-less tab falls back): {asks:?}"
    );
}

// ---- the mount tree at both themes -------------------------------------------

#[test]
fn the_mount_tree_geometry_holds_at_both_themes() {
    for (dark, (w, h)) in [(true, (960.0, 640.0)), (false, (1440.0, 816.0))] {
        let mut s = scene(react_fixture_tabs());
        s.active_key = Some("acme.afi:pricing".to_owned());
        s.dark = dark;
        let rt = rt_with(s, (w, h));
        let k = kit(dark);
        let root = element(&rt, "workspace-tabbar");
        assert_eq!(root.fill, k.c.layer_01, "bg-surface-alt (dark={dark})");
        assert!((root.rect[2] - w).abs() < 0.5, "fills the pane (dark={dark})");
        let icon = element(&rt, "workspace-tab-acme.bill-audit:quota-check-icon");
        assert_eq!((icon.rect[2], icon.rect[3]), (14.0, 14.0), "dark={dark}");
        assert_eq!(
            element(&rt, "workspace-tab-acme.afi:pricing-underline").fill,
            k.c.bg_accent,
            "the accent underline (dark={dark})"
        );

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
