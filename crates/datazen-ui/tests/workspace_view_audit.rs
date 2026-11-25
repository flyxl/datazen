//! Independent audit for T200 (the WorkspaceView Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below comes from re-reading
//! `packages/frontend/src/windows/workspace/WorkspaceView.tsx` and its
//! React suite (`__tests__/WorkspaceView.test.tsx`: the navigator plus
//! default cards when no tab is open with the initial wapp load fired,
//! one preserved shell per open tab instead of the default view with the
//! active flag, the `wapps:open-page` deep link opening a tab for a valid
//! target and ignoring unknown/disabled wapps and missing/unknown pages,
//! the external-refresh diff closing the tabs of disabled/removed wapps
//! (never enabled ones, never before the store has loaded), and the
//! resizable sidebar handle beside the 200px rail) plus the component's
//! own classes for geometry (the `flex h-full min-h-0 flex-1` row, the
//! `w-1` handle, the `flex min-h-0 min-w-0 flex-1 flex-col` right column,
//! the `relative min-h-0 flex-1` shell stack) and the composed children's
//! testids (`workspace-navigator`, `workspace-default-cards`,
//! `workspace-tabbar`, `wapp-page-shell-<key>`). Element evidence is
//! `Runtime::elements()` — layout rects, opacity, the inert flag, fills,
//! help texts — never a value the component reports about itself; the
//! three effect bodies run through recorded slots (the fetch counter, the
//! `closeByWapp` ids, the `openWappPage` pairs), exactly the seams the
//! React suite mocks.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, el, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::wapp_page_shell::{WappPageShellState, WorkspaceTabVm};
use datazen_ui::views::workspace_navigator::{OpenWappPageSlot, WorkspacePageEntry};
use datazen_ui::views::workspace_view::{
    CloseByWappSlot, FetchWappsSlot, OpenPageEventPayload, WorkspaceView,
};
use datazen_ui::views::{WappPageSummaryVm, WappSummaryVm};

/// The windows the view mounts in (the connection page's right column).
const W: f32 = 960.0;
const H: f32 = 640.0;

// ---- the host the audit stands in for ---------------------------------------

/// The workspace host's slice: the derived page list, the open tabs with
/// their per-tab shell states, the active key, and the recorded presses
/// the four slots drain into.
struct Scene {
    pages: Vec<WorkspacePageEntry>,
    tabs: Vec<WorkspaceTabVm>,
    shells: BTreeMap<String, WappPageShellState>,
    active_key: Option<String>,
    /// The host-fed `useResizable` size (React test 7's 200px default).
    sidebar_width: f32,
    dark: bool,
    /// The recorded `openWappPage(wappId, pageId)` calls.
    opens: Vec<(String, String)>,
    /// The recorded `activate(key)` presses.
    activates: Vec<String>,
    /// The recorded `close(key)` presses.
    closes: Vec<String>,
    /// The recorded `onOpenExtensions` presses.
    extension_opens: usize,
}

/// React's fixture tab shape (test 2's `tabsState.tabs` entry).
fn tab(key: &str, title: &str) -> WorkspaceTabVm {
    let (wapp_id, page_id) = key.split_once(':').unwrap();
    WorkspaceTabVm {
        key: key.to_owned(),
        wapp_id: wapp_id.to_owned(),
        page_id: page_id.to_owned(),
        title: title.to_owned(),
        icon: None,
        version: "1.0.0".to_owned(),
    }
}

/// React's fixture page shape (`makePlugin`'s contributed page).
fn page(key: &str, title: &str) -> WorkspacePageEntry {
    let (wapp_id, page_id) = key.split_once(':').unwrap();
    WorkspacePageEntry {
        key: key.to_owned(),
        wapp_id: wapp_id.to_owned(),
        page_id: page_id.to_owned(),
        title: title.to_owned(),
        icon: None,
        version: "1.0.0".to_owned(),
        author: None,
        description: None,
    }
}

/// React's `makePlugin()` — one enabled wapp contributing one page.
fn wapp(id: &str, enabled: bool, pages: &[&str]) -> WappSummaryVm {
    WappSummaryVm {
        id: id.to_owned(),
        name: id.to_owned(),
        version: "1.0.0".to_owned(),
        api_version: 2,
        author: None,
        description: None,
        icon: None,
        enabled,
        permissions: Vec::new(),
        pages: pages
            .iter()
            .map(|id| WappPageSummaryVm {
                id: (*id).to_owned(),
                title: (*id).to_owned(),
                icon: None,
            })
            .collect(),
        themes: Vec::new(),
    }
}

fn scene() -> Scene {
    Scene {
        pages: Vec::new(),
        tabs: Vec::new(),
        shells: BTreeMap::new(),
        active_key: None,
        sidebar_width: 200.0,
        dark: true,
        opens: Vec::new(),
        activates: Vec::new(),
        closes: Vec::new(),
        extension_opens: 0,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col()
        .key("probe")
        .w_full()
        .h_full()
        .child(
            WorkspaceView::new(&s.pages, &s.tabs, &s.shells, s.active_key.as_deref())
                .sidebar_width(s.sidebar_width)
                .on_open_page(Rc::new(|s: &mut Scene, wapp: &str, page: &str| {
                    s.opens.push((wapp.to_owned(), page.to_owned()));
                }))
                .on_open_extensions(Rc::new(|s: &mut Scene| s.extension_opens += 1))
                .on_activate_tab(Rc::new(|s: &mut Scene, key: &str| {
                    s.activates.push(key.to_owned());
                }))
                .on_close_tab(Rc::new(|s: &mut Scene, key: &str| {
                    s.closes.push(key.to_owned());
                }))
                .shell_lens(Rc::new(|s: &mut Scene, key: &str| {
                    s.shells.entry(key.to_owned()).or_default()
                }))
                .images(Rc::new(|_src: &str| None))
                .build(s.dark),
        )
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

/// The shell states for a tab list, one fresh state per tab (the host's
/// discipline; the states themselves are T067's audited business).
fn shells_for(tabs: &[WorkspaceTabVm]) -> BTreeMap<String, WappPageShellState> {
    tabs.iter()
        .map(|tab| (tab.key.clone(), WappPageShellState::default()))
        .collect()
}

// ---- the layout branches (React tests 1, 2 and 7) -----------------------------

#[test]
fn no_open_tab_renders_the_navigator_and_default_cards() {
    let mut s = scene();
    s.pages = vec![page("acme.bill-audit:quota-check", "Quota Check")];
    let rt = rt_with(s, (W, H));
    assert!(has(&rt, "workspace-view"));
    assert!(has(&rt, "workspace-navigator"), "React test 1");
    assert!(has(&rt, "workspace-default-cards"));
    assert!(!has(&rt, "workspace-tabbar"), "the strip renders nothing");
    assert!(!has(&rt, "workspace-shells"), "no shell without a tab");

    // The React row geometry: the 200px rail, the 1px handle, the rest.
    let nav = element(&rt, "workspace-navigator");
    assert!(
        (nav.rect[0] - 0.0).abs() < 0.5 && (nav.rect[2] - 200.0).abs() < 0.5,
        "the resizable rail at 200px: {:?}",
        nav.rect
    );
    assert!((nav.rect[3] - H).abs() < 0.5, "h-full");
    let handle = element(&rt, "workspace-sidebar-resize");
    assert!(
        (handle.rect[0] - 200.0).abs() < 0.5 && (handle.rect[2] - 1.0).abs() < 0.5,
        "w-1 flush after the rail: {:?}",
        handle.rect
    );
    assert!((handle.rect[3] - H).abs() < 0.5, "the handle spans the pane");
    let right = element(&rt, "workspace-right");
    assert!((right.rect[0] - 201.0).abs() < 0.5, "rail + handle");
    assert!(
        (right.rect[2] - (W - 201.0)).abs() < 0.5,
        "min-w-0 flex-1 takes the remainder: {:?}",
        right.rect
    );
    let cards = element(&rt, "workspace-default-cards");
    assert!((cards.rect[3] - H).abs() < 0.5, "the cards fill the column");
}

#[test]
fn one_preserved_shell_per_open_tab_replaces_the_default_view() {
    let mut s = scene();
    s.pages = vec![page("acme.bill-audit:quota-check", "Quota Check")];
    s.tabs = vec![
        tab("acme.bill-audit:quota-check", "Quota Check"),
        tab("acme.afi:pricing", "Pricing"),
    ];
    s.active_key = Some("acme.bill-audit:quota-check".to_owned());
    s.shells = shells_for(&s.tabs);
    let rt = rt_with(s, (W, H));

    assert!(!has(&rt, "workspace-default-cards"), "React test 2");
    assert!(has(&rt, "workspace-tabbar"));
    let bar = element(&rt, "workspace-tabbar");
    assert!((bar.rect[1] - 0.0).abs() < 0.5, "the strip leads the column");
    assert!((bar.rect[0] - 201.0).abs() < 0.5, "inside the right column");

    // The `relative min-h-0 flex-1` stack: both shells fill it, overlapping
    // (React's `absolute inset-0` children), in tab order.
    let stack = element(&rt, "workspace-shells");
    assert!(stack.rect[1] >= bar.rect[3] - 0.5, "the stack sits under the strip");
    for key in [
        "workspace-shell-slot-acme.bill-audit:quota-check",
        "workspace-shell-slot-acme.afi:pricing",
    ] {
        let slot = element(&rt, key);
        assert!(
            (slot.rect[0] - stack.rect[0]).abs() < 0.5
                && (slot.rect[1] - stack.rect[1]).abs() < 0.5
                && (slot.rect[2] - stack.rect[2]).abs() < 0.5
                && (slot.rect[3] - stack.rect[3]).abs() < 0.5,
            "{key} fills the stack (inset-0): {:?} vs {:?}",
            slot.rect,
            stack.rect
        );
    }

    // The preserved instance: the active shell shows, the inactive one stays
    // mounted but hidden and untouchable (React's `data-active` pins).
    let active = element(&rt, "wapp-page-shell-acme.bill-audit:quota-check");
    let inactive = element(&rt, "wapp-page-shell-acme.afi:pricing");
    assert!(
        active.opacity > 0.99 && !active.inert,
        "the active tab's shell shows (opacity {}, inert {})",
        active.opacity,
        active.inert
    );
    assert!(
        inactive.opacity < 0.01 && inactive.inert,
        "the inactive shell is preserved, hidden and inert (opacity {}, inert {})",
        inactive.opacity,
        inactive.inert
    );
}

#[test]
fn the_resize_handle_carries_the_react_identity_over_the_host_splitter() {
    // The fallback handle: React's transparent base, no drag.
    let rt = rt_with(scene(), (W, H));
    let handle = element(&rt, "workspace-sidebar-resize");
    assert_eq!(
        handle.help.as_deref(),
        Some(t("main.sidebar.resize")),
        "the title tooltip"
    );
    assert_eq!(handle.fill.a, 0.0, "bg-transparent");
    assert!((handle.rect[2] - 1.0).abs() < 0.5, "w-1");
    assert!((handle.rect[3] - H).abs() < 0.5, "the full pane height");

    // The host's splitter handle: re-keyed to the React testid and
    // re-skinned with the tooltip, its own separator semantics kept.
    let host_handle_view = |s: &Scene, _cx: &mut Cx<Scene>| {
        col().key("probe").w_full().h_full().child(
            WorkspaceView::<Scene>::new(&s.pages, &s.tabs, &s.shells, s.active_key.as_deref())
                .sidebar_width(200.0)
                .resize_handle(
                    el()
                        .key("host-splitter")
                        .role("separator")
                        .label("Resize sidebar")
                        .w(1.0)
                        .h_full(),
                )
                .build(s.dark),
        )
    };
    let mut rt = Runtime::new(
        scene(),
        host_handle_view,
        (W, H),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    settle(&mut rt);
    assert!(has(&rt, "workspace-sidebar-resize"));
    assert!(
        !has(&rt, "host-splitter"),
        "the React testid replaces the splitter key"
    );
    let handle = element(&rt, "workspace-sidebar-resize");
    assert_eq!(handle.role, "separator", "the host handle's own semantics");
    assert_eq!(handle.help.as_deref(), Some(t("main.sidebar.resize")));
    assert!((handle.rect[2] - 1.0).abs() < 0.5, "w-1");
}

// ---- the press fan-out (the composition's own wiring) -------------------------

#[test]
fn the_card_and_navigator_presses_route_through_the_one_open_seam() {
    let mut s = scene();
    s.pages = vec![page("acme.bill-audit:quota-check", "Quota Check")];
    let mut rt = rt_with(s, (W, H));
    // React's cards wiring: `onOpen={(page) => openWappPage(page.wappId,
    // page.pageId)}`.
    click(&mut rt, "workspace-default-card-acme.bill-audit:quota-check");
    assert_eq!(
        rt.state.opens,
        vec![("acme.bill-audit".to_owned(), "quota-check".to_owned())],
        "the card press carries the wapp and page ids"
    );
    // The navigator's items call the same `openWappPage`.
    click(&mut rt, "workspace-nav-item-acme.bill-audit:quota-check");
    assert_eq!(
        rt.state.opens,
        vec![
            ("acme.bill-audit".to_owned(), "quota-check".to_owned()),
            ("acme.bill-audit".to_owned(), "quota-check".to_owned()),
        ],
        "the navigator item routes through the same seam"
    );
}

#[test]
fn the_tab_strip_presses_route_through_the_store_slots() {
    let mut s = scene();
    s.tabs = vec![
        tab("acme.bill-audit:quota-check", "Quota Check"),
        tab("acme.afi:pricing", "Pricing"),
    ];
    s.active_key = Some("acme.afi:pricing".to_owned());
    s.shells = shells_for(&s.tabs);
    let mut rt = rt_with(s, (W, H));
    click(&mut rt, "workspace-tab-acme.bill-audit:quota-check-activate");
    assert_eq!(
        rt.state.activates,
        vec!["acme.bill-audit:quota-check".to_owned()],
        "the entry press activates its key"
    );
    click_revealed(&mut rt, "workspace-tab-acme.afi:pricing-close");
    assert_eq!(
        rt.state.closes,
        vec!["acme.afi:pricing".to_owned()],
        "the close press closes its key"
    );
}

#[test]
fn both_empty_states_receive_the_extensions_slot() {
    // No page, no tab: the navigator's and the cards' empty states each
    // render the jump-to-extensions button — React's two
    // `data-testid="workspace-open-extensions"` buttons — and both were
    // handed the one slot (only one was, and only one button would render).
    let rt = rt_with(scene(), (W, H));
    let buttons = rt
        .elements()
        .iter()
        .filter(|e| !e.exiting && e.key.as_deref() == Some("workspace-open-extensions"))
        .count();
    assert_eq!(buttons, 2, "both empty states render the button");
}

// ---- the three effect bodies (React tests 1, 3, 4, 5 and 6) -------------------

/// The recorder host the effect functions run against.
#[derive(Default)]
struct Recorder {
    opens: Vec<(String, String)>,
    closed_by_wapp: Vec<String>,
    fetches: usize,
}

fn open_slot() -> OpenWappPageSlot<Recorder> {
    Rc::new(|r: &mut Recorder, wapp: &str, page: &str| {
        r.opens.push((wapp.to_owned(), page.to_owned()));
    })
}

fn close_slot() -> CloseByWappSlot<Recorder> {
    Rc::new(|r: &mut Recorder, wapp: &str| {
        r.closed_by_wapp.push(wapp.to_owned());
    })
}

#[test]
fn a_valid_deep_link_opens_the_page_through_the_seam() {
    let wapps = vec![wapp("acme.bill-audit", true, &["quota-check"])];
    let mut recorder = Recorder::default();
    let open = open_slot();
    let passed = WorkspaceView::open_page_deep_link(
        &OpenPageEventPayload {
            wapp_id: Some("acme.bill-audit".to_owned()),
            page_id: Some("quota-check".to_owned()),
        },
        &wapps,
        &open,
        &mut recorder,
    );
    assert!(passed, "the link passed the gates");
    assert_eq!(
        recorder.opens,
        vec![("acme.bill-audit".to_owned(), "quota-check".to_owned())],
        "React test 3: the deep link opens the target page"
    );
}

#[test]
fn invalid_deep_links_are_ignored() {
    let wapps = vec![
        wapp("acme.bill-audit", true, &["quota-check"]),
        wapp("acme.off", false, &["main"]),
    ];
    let payloads = [
        // An unknown wapp.
        OpenPageEventPayload {
            wapp_id: Some("ghost".to_owned()),
            page_id: Some("main".to_owned()),
        },
        // A disabled wapp.
        OpenPageEventPayload {
            wapp_id: Some("acme.off".to_owned()),
            page_id: Some("main".to_owned()),
        },
        // A page the wapp does not contribute.
        OpenPageEventPayload {
            wapp_id: Some("acme.bill-audit".to_owned()),
            page_id: Some("nope".to_owned()),
        },
        // Missing ids (React's `{}` and envelope shapes) and empty ones
        // (truthiness).
        OpenPageEventPayload::default(),
        OpenPageEventPayload {
            wapp_id: Some(String::new()),
            page_id: Some("quota-check".to_owned()),
        },
        OpenPageEventPayload {
            wapp_id: Some("acme.bill-audit".to_owned()),
            page_id: Some(String::new()),
        },
        OpenPageEventPayload {
            wapp_id: None,
            page_id: Some("quota-check".to_owned()),
        },
        OpenPageEventPayload {
            wapp_id: Some("acme.bill-audit".to_owned()),
            page_id: None,
        },
    ];
    for payload in &payloads {
        let mut recorder = Recorder::default();
        let open = open_slot();
        let passed = WorkspaceView::open_page_deep_link(payload, &wapps, &open, &mut recorder);
        assert!(!passed, "{payload:?} must fail the gates");
        assert!(recorder.opens.is_empty(), "{payload:?} must not open");
    }
}

#[test]
fn an_external_refresh_closes_disabled_and_removed_wapp_tabs() {
    let tabs = vec![
        tab("acme.bill-audit:quota-check", "Quota Check"),
        tab("acme.keep:main", "Keep"),
    ];
    let close = close_slot();

    // Another window disables `acme.bill-audit`; the refreshed list arrives.
    let mut recorder = Recorder::default();
    let wapps = vec![
        wapp("acme.bill-audit", false, &["quota-check"]),
        wapp("acme.keep", true, &["main"]),
    ];
    WorkspaceView::resync_wapps(&wapps, true, &tabs, &close, &mut recorder);
    assert_eq!(
        recorder.closed_by_wapp,
        vec!["acme.bill-audit".to_owned()],
        "React test 5: the disabled wapp's tabs close"
    );

    // A later refresh where the wapp is gone entirely also closes its tabs.
    recorder.closed_by_wapp.clear();
    let wapps = vec![wapp("acme.keep", true, &["main"])];
    WorkspaceView::resync_wapps(&wapps, true, &tabs, &close, &mut recorder);
    assert_eq!(
        recorder.closed_by_wapp,
        vec!["acme.bill-audit".to_owned()],
        "the removed wapp's tabs close"
    );

    // Enabled wapps are never touched.
    recorder.closed_by_wapp.clear();
    let wapps = vec![
        wapp("acme.bill-audit", true, &["quota-check"]),
        wapp("acme.keep", true, &["main"]),
    ];
    WorkspaceView::resync_wapps(&wapps, true, &tabs, &close, &mut recorder);
    assert!(
        recorder.closed_by_wapp.is_empty(),
        "an all-enabled refresh closes nothing"
    );
}

#[test]
fn the_diff_close_visits_each_wapp_once_and_waits_for_the_load() {
    let close = close_slot();

    // Two tabs of one disabled wapp: the visited set closes it once.
    let tabs = vec![tab("acme.off:main", "Main"), tab("acme.off:other", "Other")];
    let mut recorder = Recorder::default();
    let wapps = vec![wapp("acme.off", false, &["main", "other"])];
    WorkspaceView::resync_wapps(&wapps, true, &tabs, &close, &mut recorder);
    assert_eq!(
        recorder.closed_by_wapp,
        vec!["acme.off".to_owned()],
        "one closeByWapp per wapp, however many tabs it has"
    );

    // Before the store loads, the (possibly empty) list closes nothing —
    // restored tabs survive the initial fetch (React test 6).
    let mut recorder = Recorder::default();
    WorkspaceView::resync_wapps(&[], false, &tabs, &close, &mut recorder);
    assert!(
        recorder.closed_by_wapp.is_empty(),
        "the diff does not run before the store has loaded"
    );

    // Once loaded with the wapp still enabled, the tab survives.
    let wapps = vec![wapp("acme.off", true, &["main", "other"])];
    WorkspaceView::resync_wapps(&wapps, true, &tabs, &close, &mut recorder);
    assert!(
        recorder.closed_by_wapp.is_empty(),
        "an enabled wapp's tabs survive the load"
    );
}

#[test]
fn the_initial_load_fetches_only_while_unloaded() {
    let mut recorder = Recorder::default();
    let fetch: FetchWappsSlot<Recorder> = Rc::new(|r: &mut Recorder| r.fetches += 1);
    WorkspaceView::initial_wapp_load(false, &fetch, &mut recorder);
    assert_eq!(recorder.fetches, 1, "React test 1: the mount triggers the load");
    WorkspaceView::initial_wapp_load(true, &fetch, &mut recorder);
    assert_eq!(
        recorder.fetches, 1,
        "an already-loaded store does not refetch"
    );
}

// ---- the resizable bounds (the `useResizable` initializer rule) ---------------

#[test]
fn a_fed_sidebar_width_clamps_into_the_resizable_bounds() {
    // React's `useResizable` initializer clamps a persisted value into
    // `[minSize, maxSize]` (150–480); the view clamps whatever the host
    // feeds the same way.
    for (fed, rail) in [(10.0, 150.0), (9999.0, 480.0), (200.0, 200.0), (260.0, 260.0)] {
        let mut s = scene();
        s.sidebar_width = fed;
        let rt = rt_with(s, (W, H));
        let nav = element(&rt, "workspace-navigator");
        assert!(
            (nav.rect[2] - rail).abs() < 0.5,
            "a fed width of {fed} renders the rail at {rail}: {:?}",
            nav.rect
        );
        let handle = element(&rt, "workspace-sidebar-resize");
        assert!(
            (handle.rect[0] - rail).abs() < 0.5,
            "the handle follows the clamped rail"
        );
    }
}

// ---- the mount tree at both themes -------------------------------------------

#[test]
fn the_mount_tree_holds_at_both_themes() {
    for (dark, (w, h)) in [(true, (960.0, 640.0)), (false, (1440.0, 816.0))] {
        let mut s = scene();
        s.pages = vec![page("acme.bill-audit:quota-check", "Quota Check")];
        s.tabs = vec![
            tab("acme.bill-audit:quota-check", "Quota Check"),
            tab("acme.afi:pricing", "Pricing"),
        ];
        s.active_key = Some("acme.bill-audit:quota-check".to_owned());
        s.shells = shells_for(&s.tabs);
        s.dark = dark;
        let rt = rt_with(s, (w, h));
        let k = kit(dark);

        // The composed rail keeps its skin at both themes.
        assert_eq!(
            element(&rt, "workspace-navigator").fill,
            k.c.layer_01,
            "bg-surface-alt (dark={dark})"
        );
        let handle = element(&rt, "workspace-sidebar-resize");
        assert!((handle.rect[2] - 1.0).abs() < 0.5, "w-1 (dark={dark})");
        assert!(
            (handle.rect[0] - 200.0).abs() < 0.5,
            "beside the rail (dark={dark})"
        );
        assert!(
            element(&rt, "wapp-page-shell-acme.afi:pricing").inert,
            "the inactive shell stays preserved (dark={dark})"
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
