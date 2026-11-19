//! Independent audit for T067 (the WappPageShell Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below comes from re-reading
//! `packages/frontend/src/windows/workspace/WappPageShell.tsx` and its React
//! suite (`__tests__/WappPageShell.test.tsx`: the lazy mount on first
//! activation, the pinned `datazen://` entry URL bytes, the manifest-entry
//! cache with its version invalidation, the inline-summary preference, the
//! hidden-but-mounted inactive shell, the 10s watchdog with its reload
//! re-arm, and the missing state's retry) plus the bridge suite's shell-level
//! facts (`WappPageShell.bridge.test.tsx`: one iframe per shell, titled with
//! the tab's title). Element evidence is `Runtime::elements()` — layout
//! rects, opacity, the inert flag, fill — never a value the component
//! reports about itself; the page surface is driven with a wire that answers
//! one exact src (finding its marker proves the src the iframe asked for,
//! the T017 audit's `ready_picture_wire` trick) and the manifest fetch with
//! a counter. React's unmount test (the iframe leaves with the shell) has no
//! Rune counterpart to pin: the state the host drops *is* the instance.

use std::rc::Rc;

use rune::{col, el, Cx, El, Env, Runtime, Tx};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::wapp_page_shell::{
    build_page_src, EntryPhase, ResolveStart, WappEntryCache, WappInlineEntry, WappPageShell,
    WappPageShellState, WappPageSurface, WorkspaceTabVm,
};

/// The window the shell mounts in (the workspace's page slot).
const W: f32 = 960.0;
const H: f32 = 640.0;

// ---- the host the audit stands in for ---------------------------------------

/// The workspace host's slice for one open wapp tab: the tab, the shell
/// state, the shared entry cache, and the two host seams the shell calls
/// through — the manifest fetch (a counter + the answer it would give) and
/// the page-surface wire.
struct Scene {
    tab: WorkspaceTabVm,
    shell: WappPageShellState,
    cache: WappEntryCache,
    /// The duck-typed inline `entry` the summary payload would carry.
    inline: Option<WappInlineEntry>,
    /// The manifest the host's fetch would answer: (version, entry). `None`
    /// is a failed fetch or an entry-less manifest (React's catch merges
    /// them into the missing state).
    manifest: Option<(String, String)>,
    /// How many times the host ran `getWappManifest`.
    manifest_calls: usize,
    active: bool,
    dark: bool,
    surface: WappPageSurface,
}

/// React's fixture tab (`makeTab()`).
fn make_tab(key: &str, version: &str) -> WorkspaceTabVm {
    WorkspaceTabVm {
        key: key.to_owned(),
        wapp_id: "acme.bill-audit".to_owned(),
        page_id: "quota-check".to_owned(),
        title: "Quota Check".to_owned(),
        icon: None,
        version: version.to_owned(),
    }
}

fn scene(tab: WorkspaceTabVm) -> Scene {
    Scene {
        tab,
        // Replaced by `mount` before the first frame (the real constructor
        // needs the host's `Tx`).
        shell: WappPageShellState::default(),
        cache: WappEntryCache::default(),
        inline: None,
        manifest: None,
        manifest_calls: 0,
        active: false,
        dark: true,
        surface: Rc::new(|_| None),
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col()
        .key("probe")
        .w_full()
        .h_full()
        .child(
            WappPageShell::new(&s.tab, &s.shell, s.active)
                .surface(Rc::clone(&s.surface))
                .shell(Rc::new(|s: &mut Scene| &mut s.shell))
                .build(s.dark),
        )
}

/// The host's surface wire answering one exact src with a keyed marker
/// element — finding the marker proves the src bytes the iframe asked for.
fn surface_wire(src: &str) -> WappPageSurface {
    let owned = src.to_owned();
    Rc::new(move |asked: &str| {
        (asked == owned).then(|| el().key("surface-marker").w_full().h_full())
    })
}

/// The tab-open moment: the shell state is born from the tab, the mount-time
/// `active` and the shared entry cache (the `useState` initializer plus the
/// watchdog's mount run).
fn mount(rt: &mut Runtime<Scene>) {
    rt.update(|s, tx| {
        let active = s.active;
        s.shell = WappPageShellState::new(&s.tab, active, &s.cache, tx);
    });
    settle(rt);
}

fn rt_with(tab: WorkspaceTabVm) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene(tab),
        view,
        (W, H),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    mount(&mut rt);
    rt
}

/// React's `active` prop turning true (the `[active]` effect latches
/// `everActivated`).
fn activate(rt: &mut Runtime<Scene>) {
    rt.update(|s, _tx| {
        s.active = true;
        s.shell.sync_activation(true);
    });
    settle(rt);
}

/// The host's resolve pump, part one: the entry effect's run. Returns
/// whether the manifest fetch was asked for.
fn begin(rt: &mut Runtime<Scene>) -> bool {
    let mut fetched = false;
    rt.update(|s, tx| {
        let tab = s.tab.clone();
        let inline = s.inline.clone();
        let mut cache = std::mem::take(&mut s.cache);
        if let ResolveStart::FetchManifest =
            s.shell.begin_resolve(&tab, inline.as_ref(), &mut cache, tx)
        {
            s.manifest_calls += 1;
            fetched = true;
        }
        s.cache = cache;
    });
    rt.frame(0.0);
    fetched
}

/// Part two: the host's manifest fetch and its answer (the promise's
/// landing).
fn answer(rt: &mut Runtime<Scene>) {
    rt.update(|s, tx| {
        let tab = s.tab.clone();
        let manifest = s
            .manifest
            .as_ref()
            .map(|(version, entry)| (version.as_str(), entry.as_str()));
        let mut cache = std::mem::take(&mut s.cache);
        s.shell.resolve_answer(&tab, manifest, &mut cache, tx);
        s.cache = cache;
    });
    settle(rt);
}

/// The host's clock moving `dt` seconds, then its poll (the watchdog tick).
fn advance(rt: &mut Runtime<Scene>, dt: f64) {
    rt.frame(dt);
    rt.update(|s, tx| {
        s.shell.tick(tx.clock());
    });
    rt.frame(0.0);
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

/// Real-time frames: dt=0 frames freeze presence animations at their start
/// (opacity 0), where a freshly-mounted button is not hit-testable and a
/// freshly-readied shell still reads opacity 0.
fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn has(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
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

/// A shell whose manifest answers `./index.html` under 1.0.0, with the
/// surface wire answering the pinned src bytes — the ready state React's
/// suite reaches with `getManifestMock.mockResolvedValue`.
fn ready_rt() -> Runtime<Scene> {
    ready_rt_at((W, H), true)
}

fn ready_rt_at(size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut s = scene(make_tab("acme.bill-audit:quota-check", "1.0.0"));
    s.dark = dark;
    s.manifest = Some(("1.0.0".to_owned(), "./index.html".to_owned()));
    s.surface = surface_wire("datazen://acme.bill-audit/index.html?v=1.0.0");
    let mut rt = Runtime::new(
        s,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    mount(&mut rt);
    activate(&mut rt);
    assert!(begin(&mut rt), "the manifest fetch was asked for");
    answer(&mut rt);
    rt
}

// ---- the resolver bytes (React test 2's pinned src) --------------------------

#[test]
fn build_page_src_matches_the_desktop_resolver_bytes() {
    // The exact string React's test pins for entry './index.html'.
    assert_eq!(
        build_page_src("acme.bill-audit", "./index.html", "1.0.0"),
        "datazen://acme.bill-audit/index.html?v=1.0.0"
    );
    assert_eq!(
        build_page_src("acme.bill-audit", "main.html", "1.0.0"),
        "datazen://acme.bill-audit/main.html?v=1.0.0"
    );
    // Only ONE leading ./ strips, exactly like `replace(/^\.\//, '')`.
    assert_eq!(
        build_page_src("acme.bill-audit", "././a.html", "1.0.0"),
        "datazen://acme.bill-audit/./a.html?v=1.0.0"
    );
    // encodeVersion is encodeURIComponent: reserved bytes escape…
    assert_eq!(
        build_page_src("acme.bill-audit", "i.html", "2.0.0-beta+1"),
        "datazen://acme.bill-audit/i.html?v=2.0.0-beta%2B1"
    );
    assert_eq!(
        build_page_src("acme.bill-audit", "i.html", "v 1"),
        "datazen://acme.bill-audit/i.html?v=v%201"
    );
    // …and the unreserved set passes through untouched.
    assert_eq!(
        build_page_src("a.b", "i.html", "1.0.0-X_!.~*'()"),
        "datazen://a.b/i.html?v=1.0.0-X_!.~*'()"
    );
}

// ---- the state machine (the useState/effect halves, pure) --------------------

#[test]
fn the_resolve_gate_is_the_react_effect_guard() {
    let tab = make_tab("acme.bill-audit:quota-check", "1.0.0");
    let mut cache = WappEntryCache::default();
    let mut tx = Tx::default();
    let mut shell = WappPageShellState::new(&tab, false, &cache, &mut tx);
    // Not activated: the effect returns before any resolve.
    assert_eq!(
        shell.begin_resolve(&tab, None, &mut cache, &mut tx),
        ResolveStart::NotStarted
    );
    // Activation latches (and never retracts).
    shell.sync_activation(true);
    shell.sync_activation(false);
    assert!(shell.ever_activated());
    assert_eq!(
        shell.begin_resolve(&tab, None, &mut cache, &mut tx),
        ResolveStart::FetchManifest
    );
    // A resolve already in flight: resolvingRef holds the effect off.
    assert_eq!(
        shell.begin_resolve(&tab, None, &mut cache, &mut tx),
        ResolveStart::NotStarted
    );
    // The answer resolves once: a second landing changes nothing (the
    // promise chain's once-ness).
    shell.resolve_answer(
        &tab,
        Some(("1.0.0", "index.html")),
        &mut cache,
        &mut tx,
    );
    let phase = shell.phase().clone();
    shell.resolve_answer(
        &tab,
        Some(("9.9.9", "other.html")),
        &mut cache,
        &mut tx,
    );
    assert_eq!(shell.phase(), &phase);
    // The cache carries the manifest's version, the src the tab's.
    assert_eq!(
        cache.cached_src(&tab).as_deref(),
        Some("datazen://acme.bill-audit/index.html?v=1.0.0")
    );
}

#[test]
fn the_inline_entry_branch_and_its_empty_string_gate() {
    let tab = make_tab("acme.bill-audit:quota-check", "1.0.0");
    let mut cache = WappEntryCache::default();
    let mut tx = Tx::default();
    let mut shell = WappPageShellState::new(&tab, true, &cache, &mut tx);
    assert_eq!(
        shell.begin_resolve(
            &tab,
            Some(&WappInlineEntry {
                version: "1.0.0".to_owned(),
                entry: "main.html".to_owned(),
            }),
            &mut cache,
            &mut tx,
        ),
        ResolveStart::ResolvedFromSummary
    );
    assert_eq!(
        shell.phase(),
        &EntryPhase::Ready {
            src: "datazen://acme.bill-audit/main.html?v=1.0.0".to_owned()
        }
    );
    // The cache writes the SUMMARY's version (not the tab's).
    assert_eq!(cache.hit("acme.bill-audit").unwrap().version, "1.0.0");

    // An empty inline entry is falsy in React: the manifest branch runs.
    let mut cache2 = WappEntryCache::default();
    let mut shell2 = WappPageShellState::new(&tab, true, &cache2, &mut tx);
    assert_eq!(
        shell2.begin_resolve(
            &tab,
            Some(&WappInlineEntry {
                version: "1.0.0".to_owned(),
                entry: String::new(),
            }),
            &mut cache2,
            &mut tx,
        ),
        ResolveStart::FetchManifest
    );
}

#[test]
fn an_entryless_or_failed_manifest_lands_missing() {
    let tab = make_tab("acme.bill-audit:quota-check", "1.0.0");
    // A failed fetch (React's catch).
    let mut cache = WappEntryCache::default();
    let mut tx = Tx::default();
    let mut shell = WappPageShellState::new(&tab, true, &cache, &mut tx);
    shell.begin_resolve(&tab, None, &mut cache, &mut tx);
    shell.resolve_answer(&tab, None, &mut cache, &mut tx);
    assert_eq!(shell.phase(), &EntryPhase::Missing);
    // A manifest that declares no entry (`!manifest.entry` — the empty
    // string is falsy too) throws into the same catch.
    let mut cache2 = WappEntryCache::default();
    let mut shell2 = WappPageShellState::new(&tab, true, &cache2, &mut tx);
    shell2.begin_resolve(&tab, None, &mut cache2, &mut tx);
    shell2.resolve_answer(&tab, Some(("1.0.0", "")), &mut cache2, &mut tx);
    assert_eq!(shell2.phase(), &EntryPhase::Missing);
}

#[test]
fn the_watchdog_fires_once_at_the_deadline() {
    let tab = make_tab("acme.bill-audit:quota-check", "1.0.0");
    let mut cache = WappEntryCache::default();
    let mut tx = Tx::default(); // clock 0
    let mut shell = WappPageShellState::new(&tab, true, &cache, &mut tx);
    shell.begin_resolve(&tab, None, &mut cache, &mut tx);
    shell.resolve_answer(&tab, Some(("1.0.0", "index.html")), &mut cache, &mut tx);
    assert!(!shell.failed());
    assert!(!shell.tick(9.999), "the budget has not run out yet");
    assert!(!shell.failed());
    assert!(shell.tick(10.0), "the watchdog fires at exactly 10s");
    assert!(shell.failed());
    // Fired once: the effect re-ran with failed=true and armed nothing.
    assert!(!shell.tick(11.0));
    assert!(shell.failed());
}

#[test]
fn load_and_error_disarm_the_watchdog_and_reload_re_arms() {
    let tab = make_tab("acme.bill-audit:quota-check", "1.0.0");

    // onLoad: a loaded frame cannot time out.
    let mut cache = WappEntryCache::default();
    let mut tx = Tx::default();
    let mut shell = WappPageShellState::new(&tab, true, &cache, &mut tx);
    shell.begin_resolve(&tab, None, &mut cache, &mut tx);
    shell.resolve_answer(&tab, Some(("1.0.0", "index.html")), &mut cache, &mut tx);
    shell.on_load();
    assert!(!shell.tick(20.0), "a loaded frame cannot time out");
    assert!(!shell.failed());
    // reload clears the load and re-arms for the fresh frame (clock 0).
    shell.reload(&mut tx);
    assert!(!shell.loaded());
    assert!(!shell.tick(9.999));
    assert!(shell.tick(10.0), "the fresh frame's budget runs out");

    // onError: the failure state, the watchdog disarmed.
    let mut cache2 = WappEntryCache::default();
    let mut shell2 = WappPageShellState::new(&tab, true, &cache2, &mut tx);
    shell2.begin_resolve(&tab, None, &mut cache2, &mut tx);
    shell2.resolve_answer(&tab, Some(("1.0.0", "index.html")), &mut cache2, &mut tx);
    shell2.on_error();
    assert!(shell2.failed());
    assert!(!shell.tick(20.0), "an errored frame has nothing to time out");
}

#[test]
fn reload_resets_the_recovery_state_and_reopens_a_missing_shell() {
    let tab = make_tab("acme.bill-audit:quota-check", "1.0.0");
    let mut cache = WappEntryCache::default();
    let mut tx = Tx::default();
    let mut shell = WappPageShellState::new(&tab, true, &cache, &mut tx);
    shell.begin_resolve(&tab, None, &mut cache, &mut tx);
    shell.resolve_answer(&tab, Some(("1.0.0", "index.html")), &mut cache, &mut tx);
    shell.on_error();
    assert!(shell.failed());
    shell.reload(&mut tx);
    assert!(!shell.failed(), "reload clears the failure");
    assert!(!shell.loaded());
    assert_eq!(shell.reload_nonce(), 1);
    // A ready shell stays ready across reload (only the frame remounts).
    assert!(matches!(shell.phase(), EntryPhase::Ready { .. }));

    // A missing shell reopens: the phase returns to idle so the resolve
    // runs again (the retry's gate).
    let mut cache2 = WappEntryCache::default();
    let mut shell2 = WappPageShellState::new(&tab, true, &cache2, &mut tx);
    shell2.begin_resolve(&tab, None, &mut cache2, &mut tx);
    shell2.resolve_answer(&tab, None, &mut cache2, &mut tx);
    assert_eq!(shell2.phase(), &EntryPhase::Missing);
    shell2.reload(&mut tx);
    assert_eq!(shell2.phase(), &EntryPhase::Idle);
    // And the resolve can run again.
    assert_eq!(
        shell2.begin_resolve(&tab, None, &mut cache2, &mut tx),
        ResolveStart::FetchManifest
    );
}

// ---- runtime evidence (the element tree, branch by branch) -------------------

#[test]
fn an_inactive_tab_lazily_stays_idle_but_mounted() {
    // React test 1: no iframe before the first activation, the shell hidden.
    let mut rt = rt_with(make_tab("acme.bill-audit:quota-check", "1.0.0"));
    let shell = element(&rt, "wapp-page-shell-acme.bill-audit:quota-check");
    assert_eq!(shell.opacity, 0.0, "the hidden class: paints nothing");
    assert!(shell.inert, "display:none passes input through");
    assert!(
        !has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-iframe"),
        "no iframe before activation"
    );
    assert!(
        has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-loading"),
        "the idle phase shows the loading branch"
    );

    // Activation resolves: resolving still loads, the answer mounts. (The
    // manifest the host would answer — React test 1's mockResolvedValue.)
    rt.update(|s, _tx| {
        s.manifest = Some(("1.0.0".to_owned(), "index.html".to_owned()));
        s.surface = surface_wire("datazen://acme.bill-audit/index.html?v=1.0.0");
    });
    activate(&mut rt);
    assert!(
        has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-loading"),
        "the resolving phase shows the loading branch"
    );
    assert!(!has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-iframe"));
    assert!(begin(&mut rt));
    assert!(has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-loading"));
    answer(&mut rt);
    assert!(
        has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-iframe"),
        "the ready iframe mounts after activation + resolve"
    );
    // The active shell is visible and interactive again.
    let shell = element(&rt, "wapp-page-shell-acme.bill-audit:quota-check");
    assert_eq!(shell.opacity, 1.0);
    assert!(!shell.inert);
}

#[test]
fn the_ready_iframe_pins_the_entry_url_and_caches_the_manifest_entry() {
    // React test 2: the sandboxed iframe with the datazen:// entry URL,
    // one manifest fetch, the cache reused, the version invalidating it.
    let mut rt = ready_rt();
    let root = "wapp-page-shell-acme.bill-audit:quota-check";

    let iframe = element(&rt, &format!("{root}-iframe"));
    // title={tab.title} is the iframe's accessible name.
    assert_eq!(iframe.label.as_deref(), Some("Quota Check"));
    // The wire answers only the pinned bytes, so the marker proves the src.
    assert!(has(&rt, "surface-marker"), "the src asked for is the entry URL");
    assert_eq!(rt.state.manifest_calls, 1);

    // Geometry: the iframe fills the body under the 36pt header.
    let [x, y, w, h] = iframe.rect;
    assert_eq!((x, y, w, h), (0.0, 36.0, W, H - 36.0));

    // A second shell of the same wapp+version resolves from the cache.
    rt.update(|s, tx| {
        s.tab = make_tab("acme.bill-audit:other", "1.0.0");
        let active = s.active;
        s.shell = WappPageShellState::new(&s.tab, active, &s.cache, tx);
    });
    rt.frame(0.0);
    assert!(!begin(&mut rt), "the cache answered, no fetch");
    assert!(has(&rt, "wapp-page-shell-acme.bill-audit:other-iframe"));
    assert!(has(&rt, "surface-marker"));
    assert_eq!(rt.state.manifest_calls, 1, "still exactly one manifest fetch");

    // A version change invalidates the cache.
    rt.update(|s, tx| {
        s.tab = make_tab("acme.bill-audit:quota-check", "2.0.0");
        s.surface = surface_wire("datazen://acme.bill-audit/index.html?v=2.0.0");
        s.shell = WappPageShellState::new(&s.tab, true, &s.cache, tx);
    });
    rt.frame(0.0);
    assert!(begin(&mut rt), "the version change re-fetches");
    assert_eq!(rt.state.manifest_calls, 2);
    answer(&mut rt);
    assert!(has(&rt, "surface-marker"), "the src now carries the new version");
}

#[test]
fn an_inline_summary_entry_wins_without_a_manifest_round_trip() {
    // React test 3: the summary's inline entry builds the src directly.
    let mut rt = rt_with(make_tab("acme.bill-audit:quota-check", "1.0.0"));
    rt.update(|s, _tx| {
        s.inline = Some(WappInlineEntry {
            version: "1.0.0".to_owned(),
            entry: "main.html".to_owned(),
        });
        // A manifest is armed to prove it is never asked for.
        s.manifest = Some(("1.0.0".to_owned(), "./index.html".to_owned()));
        s.surface = surface_wire("datazen://acme.bill-audit/main.html?v=1.0.0");
    });
    activate(&mut rt);
    assert!(!begin(&mut rt), "the inline entry needs no manifest");
    assert_eq!(rt.state.manifest_calls, 0);
    assert!(has(&rt, "wapp-page-shell-acme.bill-audit:quota-check-iframe"));
    assert!(
        has(&rt, "surface-marker"),
        "the src is the inline entry's, not the manifest's"
    );
}

#[test]
fn an_inactive_shell_stays_mounted_but_hidden() {
    // React test 4: instance preserved — the same iframe element stays in
    // the tree, the shell hidden and aria-hidden.
    let mut rt = ready_rt();
    rt.update(|s, _tx| s.active = false);
    // The hide fade runs on the clock: settle past it.
    settle(&mut rt);
    let root = "wapp-page-shell-acme.bill-audit:quota-check";
    let shell = element(&rt, root);
    // The hide fade eases out asymptotically: 0.4s in it sits under 1% —
    // visually gone, aria-hidden via inert.
    assert!(shell.opacity < 0.01, "the hidden shell fades to {}", shell.opacity);
    assert!(shell.inert);
    assert!(has(&rt, &format!("{root}-iframe")), "the iframe is still mounted");
}

#[test]
fn the_load_watchdog_fails_after_ten_seconds_and_reloads_remount() {
    // React test 5: no reload button on a fresh frame; the 10s budget flips
    // the shell into its failure state; reload remounts the iframe and
    // re-arms the watchdog for the fresh frame.
    let mut rt = ready_rt();
    let root = "wapp-page-shell-acme.bill-audit:quota-check";
    assert!(!has(&rt, &format!("{root}-reload")));

    advance(&mut rt, 10.0);
    let reload = element(&rt, &format!("{root}-reload"));
    assert_eq!(
        reload.label.as_deref(),
        Some(t("workspace.shell.reload")),
        "the reload button's label"
    );
    let note = element(&rt, &format!("{root}-header-failed"));
    assert_eq!(
        note.text.as_deref(),
        Some(t("workspace.shell.loadFailed")),
        "the header failure note"
    );
    // A timed-out ready shell keeps its iframe (only the note and the
    // reload button appear).
    assert!(has(&rt, &format!("{root}-iframe")));
    assert!(has(&rt, "surface-marker"));

    click(&mut rt, &format!("{root}-reload"));
    // The remount: the nonce suffix keys a fresh iframe, the old one exits.
    assert_eq!(rt.state.shell.reload_nonce(), 1, "the reload press ran");
    assert!(has(&rt, &format!("{root}-iframe#1")), "the reload remounts the iframe");
    assert!(!has(&rt, &format!("{root}-iframe")), "the old frame is gone");
    assert!(!has(&rt, &format!("{root}-reload")));
    assert!(has(&rt, "surface-marker"), "the src is unchanged — no re-resolve");

    // The watchdog re-arms for the fresh frame. The reload press's own
    // 0.4s settle already spent part of the 10s budget, so the windows are
    // margins, not the exact 9.999/0.01 split: well inside stays armed,
    // past the budget fires.
    advance(&mut rt, 9.0);
    assert!(
        !has(&rt, &format!("{root}-reload")),
        "the fresh frame's budget has not run out"
    );
    advance(&mut rt, 1.0);
    assert!(has(&rt, &format!("{root}-reload")), "the re-armed budget runs out");
}

#[test]
fn a_manifest_without_an_entry_lands_missing_and_retries_on_demand() {
    // React test 6: the missing state's failure text and retry button, and
    // the retry re-running the resolve (which still finds no entry).
    let mut rt = rt_with(make_tab("acme.bill-audit:quota-check", "1.0.0"));
    rt.update(|s, _tx| {
        s.manifest = None; // the fetch answers no entry
    });
    activate(&mut rt);
    assert!(begin(&mut rt));
    assert_eq!(rt.state.manifest_calls, 1);
    answer(&mut rt);
    let root = "wapp-page-shell-acme.bill-audit:quota-check";
    assert!(!has(&rt, &format!("{root}-iframe")), "no iframe without an entry");
    let note = element(&rt, &format!("{root}-missing-failed"));
    assert_eq!(
        note.text.as_deref(),
        Some(t("workspace.shell.loadFailed")),
        "the missing state's failure text"
    );
    assert!(has(&rt, &format!("{root}-retry")));
    // The missing block is centered under the header.
    let missing = element(&rt, &format!("{root}-missing")).rect;
    let centre = missing[0] + missing[2] / 2.0;
    assert!((centre - W / 2.0).abs() < 0.5, "the missing block is centered ({centre})");

    click(&mut rt, &format!("{root}-retry"));
    assert!(begin(&mut rt), "the retry re-runs the resolve");
    assert_eq!(rt.state.manifest_calls, 2);
    answer(&mut rt);
    assert_eq!(rt.state.manifest_calls, 2);
    assert!(
        has(&rt, &format!("{root}-missing-failed")),
        "still missing without an entry"
    );
    assert!(!has(&rt, &format!("{root}-iframe")));
}

#[test]
fn the_loading_branch_centers_the_spinner() {
    // The idle/resolving body: a centered 20pt spinner (React's xl tier).
    let mut rt = rt_with(make_tab("acme.bill-audit:quota-check", "1.0.0"));
    activate(&mut rt);
    begin(&mut rt);
    let root = "wapp-page-shell-acme.bill-audit:quota-check";
    let spinner = element(&rt, &format!("{root}-loading-spinner"));
    assert_eq!(
        (spinner.rect[2], spinner.rect[3]),
        (20.0, 20.0),
        "React's xl (h-5 w-5) is the base's Lg tier"
    );
    let cx = spinner.rect[0] + spinner.rect[2] / 2.0;
    let cy = spinner.rect[1] + spinner.rect[3] / 2.0;
    assert_eq!(
        (cx, cy),
        (W / 2.0, 36.0 + (H - 36.0) / 2.0),
        "centered in the body under the header"
    );
    assert!(!has(&rt, &format!("{root}-iframe")));
}

#[test]
fn the_mount_tree_geometry_and_fill_hold_at_both_themes() {
    for (dark, (w, h)) in [(true, (960.0, 640.0)), (false, (1440.0, 816.0))] {
        let mut rt = ready_rt_at((w, h), dark);
        let root = "wapp-page-shell-acme.bill-audit:quota-check";
        let shell = element(&rt, root);
        // bg-surface fills the shell (the kit's base surface token).
        assert_eq!(shell.fill, kit(dark).c.bg_base, "bg-surface (dark={dark})");

        // The header: h-9, at the top, full width.
        let header = element(&rt, &format!("{root}-header"));
        assert_eq!((header.rect[0], header.rect[1], header.rect[3]), (0.0, 0.0, 36.0));
        assert!((header.rect[2] - w).abs() < 0.5, "full width at {w}pt");

        // The identity row's order: icon, then title, then the version chip.
        let icon = element(&rt, &format!("{root}-icon"));
        let title = element(&rt, &format!("{root}-title"));
        let version = element(&rt, &format!("{root}-version"));
        assert!(icon.rect[0] < title.rect[0], "the icon leads the title");
        assert!(title.rect[0] < version.rect[0], "the chip follows the title");
        assert_eq!((icon.rect[2], icon.rect[3]), (14.0, 14.0), "h-3.5 w-3.5");
        assert_eq!(title.text.as_deref(), Some("Quota Check"));
        assert_eq!(
            element(&rt, &format!("{root}-version-text")).text.as_deref(),
            Some("v1.0.0")
        );

        // The iframe fills the body under the header.
        let iframe = element(&rt, &format!("{root}-iframe"));
        let [x, y, iw, ih] = iframe.rect;
        assert_eq!((x, y, iw, ih), (0.0, 36.0, w, h - 36.0), "at {w}x{h} (dark={dark})");

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
