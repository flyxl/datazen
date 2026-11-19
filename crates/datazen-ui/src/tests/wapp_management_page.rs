//! T072: the `WappManagementPage` port — the wapps area's management page
//! observed through the real `crate::view` (the workspace's `Section::Wapps`
//! slot mounts it): the header with the installed count, the search/filter
//! chips and the derivations behind them, the grouped "all" view, the cards
//! (permission badges, theme hints, the API-mismatch gate), the toggle/
//! uninstall/open actions and their honest host seams, the install press
//! handing T022's dialog, and the geometry at both window sizes and both
//! themes. React's own suite (`WappManagementPage.test.tsx`) is the contract;
//! the observation channel here is the a11y tree (keys, rects, roles,
//! labels, text, fill, opacity, focusability) of the real view — nothing is
//! asked of the modules under test directly.

use super::*;
use crate::i18n::t;
use crate::views::{WappPageSummaryVm, WappSummaryVm, WAPP_API_VERSION};
use rune::{Key, KeyEvent, Mods};

/// React's fixture (`WappManagementPage.test.tsx:82-96`): the workspace wapp
/// with one page and one permission.
fn wapp() -> WappSummaryVm {
    WappSummaryVm {
        id: "acme.bill-audit".into(),
        name: "Bill Audit".into(),
        version: "1.0.0".into(),
        api_version: WAPP_API_VERSION,
        author: Some("Acme".into()),
        description: Some("Compare bills against quotas".into()),
        icon: None,
        enabled: true,
        permissions: vec!["context:connections".into()],
        pages: vec![WappPageSummaryVm {
            id: "quota-check".into(),
            title: "Quota Check".into(),
            icon: None,
        }],
        themes: Vec::new(),
    }
}

/// React's theme-only twin (`pages: []`, one theme contribution, no
/// permissions).
fn theme_wapp() -> WappSummaryVm {
    WappSummaryVm {
        id: "acme.midnight".into(),
        name: "Midnight".into(),
        pages: Vec::new(),
        themes: vec!["midnight-blue".into()],
        permissions: Vec::new(),
        ..wapp()
    }
}

fn page_runtime(wapps: Vec<WappSummaryVm>, size: (f32, f32), dark: bool) -> Runtime<AppState> {
    let mut state = AppState::default();
    // The fetch success transition (the wiring's call once the adapter
    // answers the installed list).
    state.wapps_loaded(wapps);
    state.select(Section::Wapps);
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn escape(rt: &mut Runtime<AppState>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
    // The dismissed layer rides an exit transition; real time completes it
    // (a zero-delta frame holds the fade at its start).
    rt.frame(0.5);
}

/// Search text lands in the page's box the way React's `fireEvent.change`
/// does — through the state the field binds, observed through the cards the
/// next frame renders (the field-kit precedent for driving a bound field:
/// the write itself, then `invalidate`, so the next frame re-runs the view
/// and the cards answer the needle it reads).
fn search(rt: &mut Runtime<AppState>, needle: &str) {
    rt.state.wapp_search = rune::TextEdit::new(needle.to_owned());
    rt.invalidate();
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The laid-out cards' keys — each card's own element. A card's sub-parts
/// carry compound keys off the card key, one dash-part deeper, and wapp ids
/// carry dashes of their own (`acme.bill-audit`), so a dash-part count
/// cannot tell a sub-part from a container: the containers are the
/// `extension-card-*` keys that no other laid-out key extends across a dash.
fn cards(rt: &Runtime<AppState>) -> Vec<String> {
    let keys: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with("extension-card-"))
        .collect();
    keys.iter()
        .filter(|k| {
            !keys.iter().any(|parent| {
                parent.len() < k.len()
                    && k.starts_with(parent.as_str())
                    && k.as_bytes().get(parent.len()) == Some(&b'-')
            })
        })
        .cloned()
        .collect()
}

#[test]
fn the_header_counts_the_installed_and_renders_one_card_per_wapp() {
    // React's first contract (`WappManagementPage.test.tsx:139-152`): the
    // page with its title, the count beside it, one card per installed wapp,
    // and the card's permission badge.
    let rt = page_runtime(vec![wapp(), theme_wapp()], (1440.0, 900.0), true);

    assert!(has_key(&rt, "extension-management-page"));
    assert_eq!(
        element(&rt, "extension-page-title").text.as_deref(),
        Some(t("extensions.page.title")),
        "the title reads the zh-CN table"
    );
    assert_eq!(
        element(&rt, "extension-page-count").text.as_deref(),
        Some(t("extensions.page.count").replace("{count}", "2").as_str()),
        "the count names the whole installed list"
    );
    assert_eq!(
        cards(&rt).len(),
        1,
        "the default Workspace filter hides the theme-only wapp"
    );
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-name").text.as_deref(),
        Some("Bill Audit"),
        "the card carries the wapp's name"
    );
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-version-text").text.as_deref(),
        Some("v1.0.0"),
        "the version chip renders React's `v{{version}}`"
    );
    // The permission badge, one per requested permission string.
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-badge-permission-context:connections")
            .label
            .as_deref(),
        Some("context:connections"),
        "the permission badge renders the permission string"
    );
    // The description line React's `p.description` renders.
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-description").text.as_deref(),
        Some("Compare bills against quotas"),
    );
    // The byline with the manifest's author.
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-byline").text.as_deref(),
        Some("by Acme"),
    );
}

#[test]
fn a_wapp_without_an_author_shows_its_id_as_the_byline() {
    // React: `wapp.author ? `by ${wapp.author}` : wapp.id`
    // (`WappManagementPage.tsx:231-233`) — the missing and the empty author
    // both fall back to the id the card shows instead.
    let no_author = WappSummaryVm { author: None, ..wapp() };
    let rt = page_runtime(vec![no_author], (1440.0, 900.0), true);
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-byline").text.as_deref(),
        Some("acme.bill-audit"),
        "a missing author falls back to the id"
    );

    let empty_author = WappSummaryVm { author: Some(String::new()), ..wapp() };
    let rt = page_runtime(vec![empty_author], (1440.0, 900.0), true);
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-byline").text.as_deref(),
        Some("acme.bill-audit"),
        "an empty author falls back to the id"
    );
}

#[test]
fn the_chips_carry_the_counts_of_the_whole_installed_list() {
    // React renders each chip with its kind's count over the whole installed
    // list, whatever the search shows (`WappManagementPage.tsx:157-164`, the
    // `t(..., { count: counts[f] })` at `362-369`) — the fixture's three
    // wapps split 2 workspace / 1 theme so a swapped pair cannot pass.
    let afi = WappSummaryVm { id: "acme.afi".into(), name: "AFI Pricing".into(), ..wapp() };
    let rt = page_runtime(vec![wapp(), afi, theme_wapp()], (1440.0, 900.0), true);

    assert_eq!(
        element(&rt, "extension-filter-all").label.as_deref(),
        Some(t("extensions.page.filterAll").replace("{count}", "3").as_str()),
        "the all chip counts every installed wapp"
    );
    assert_eq!(
        element(&rt, "extension-filter-workspace").label.as_deref(),
        Some(t("extensions.page.filterWorkspace").replace("{count}", "2").as_str()),
        "the workspace chip counts the workspace kind"
    );
    assert_eq!(
        element(&rt, "extension-filter-theme").label.as_deref(),
        Some(t("extensions.page.filterTheme").replace("{count}", "1").as_str()),
        "the theme chip counts the theme kind"
    );
}

#[test]
fn the_filter_defaults_to_workspace_and_switches_through_all_and_theme_chips() {
    // React (`WappManagementPage.test.tsx:154-180`): the content body
    // defaults to the Workspace filter — theme-only wapps start hidden; the
    // chips switch the view; the selected chip is the accented one (the
    // tree's fill is the observation channel; React asserts bg-accent).
    let mut rt = page_runtime(vec![wapp(), theme_wapp()], (1440.0, 900.0), true);

    assert_eq!(cards(&rt).len(), 1, "the default filter lists the workspace wapp only");
    assert_ne!(
        element(&rt, "extension-filter-workspace").fill,
        rune::Color::TRANSPARENT,
        "the workspace chip is selected"
    );
    assert_eq!(
        element(&rt, "extension-filter-all").fill,
        rune::Color::TRANSPARENT,
        "the all chip is not selected"
    );
    assert_eq!(
        element(&rt, "extension-filter-theme").fill,
        rune::Color::TRANSPARENT,
        "the theme chip is not selected"
    );

    // Theme: the theme-only wapp, and only it.
    click(&mut rt, "extension-filter-theme");
    // React's `transition-colors`: the selected chip's fill rides a color
    // track that starts at the old value — a zero-delta frame holds it
    // there, so the selection reads once the transition settles.
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(cards(&rt).len(), 1, "the theme view lists the theme-only wapp");
    assert!(
        has_key(&rt, "extension-card-acme.midnight-name"),
        "the theme wapp is the one listed"
    );
    assert!(!has_key(&rt, "extension-card-acme.bill-audit-name"), "the workspace wapp is out");
    assert_ne!(
        element(&rt, "extension-filter-theme").fill,
        rune::Color::TRANSPARENT,
        "the theme chip took the selection"
    );

    // All: both.
    click(&mut rt, "extension-filter-all");
    assert_eq!(cards(&rt).len(), 2, "the all view lists both wapps");
}

#[test]
fn the_all_view_groups_into_workspace_and_theme_sections() {
    // React (`WappManagementPage.test.tsx:182-211`): the "all" view's two
    // sections — the workspace pages first, then the theme-only wapps; a
    // wapp contributing both kinds belongs to the Workspace group and shows
    // exactly once.
    let both = WappSummaryVm {
        id: "acme.both".into(),
        name: "Both".into(),
        themes: vec!["blue".into()],
        ..wapp()
    };
    let mut rt = page_runtime(vec![theme_wapp(), wapp(), both], (1440.0, 900.0), true);
    click(&mut rt, "extension-filter-all");

    assert!(has_key(&rt, "extension-group-workspace"), "the workspace group mounts");
    assert_eq!(
        element(&rt, "extension-group-workspace-header").text.as_deref(),
        Some(t("extensions.page.groupWorkspace")),
        "the workspace group's header reads the table"
    );
    assert!(has_key(&rt, "extension-group-theme"), "the theme group mounts");
    assert_eq!(
        element(&rt, "extension-group-theme-header").text.as_deref(),
        Some(t("extensions.page.groupTheme")),
        "the theme group's header reads the table"
    );
    // Two cards in the workspace group (Bill Audit + Both), one in the theme
    // group (Midnight), Both shows exactly once.
    assert!(has_key(&rt, "extension-card-acme.bill-audit-name"));
    assert!(has_key(&rt, "extension-card-acme.both-name"));
    assert!(has_key(&rt, "extension-card-acme.midnight-name"));
    assert_eq!(cards(&rt).len(), 3, "each wapp shows exactly once");
}

#[test]
fn the_all_view_orders_the_workspace_group_above_the_theme_group() {
    // React's `allGroups` memo fixes the order — the workspace pages first,
    // then the theme-only wapps (`WappManagementPage.tsx:167-173`) — read
    // through the two headers' rects (the tree's geometry channel).
    let mut rt = page_runtime(vec![wapp(), theme_wapp()], (1440.0, 900.0), true);
    click(&mut rt, "extension-filter-all");

    let workspace = element(&rt, "extension-group-workspace-header").rect[1];
    let theme = element(&rt, "extension-group-theme-header").rect[1];
    assert!(
        workspace < theme,
        "the workspace group sits above the theme group ({workspace} vs {theme})"
    );
}

#[test]
fn empty_groups_stay_hidden_and_single_kind_filters_keep_the_flat_grid() {
    // React (`WappManagementPage.test.tsx:213-228`): a group with no wapps
    // renders nothing at all, and the non-"all" filters keep the flat grid
    // without headers.
    let mut rt = page_runtime(
        vec![
            wapp(),
            WappSummaryVm { id: "acme.afi".into(), name: "AFI Pricing".into(), ..wapp() },
        ],
        (1440.0, 900.0),
        true,
    );

    click(&mut rt, "extension-filter-all");
    assert!(
        !has_key(&rt, "extension-group-theme"),
        "no theme contributions leaves the theme group out entirely"
    );
    assert!(has_key(&rt, "extension-group-workspace"));
    assert_eq!(cards(&rt).len(), 2);

    // Back on a single-kind filter: the flat grid, no group headers.
    click(&mut rt, "extension-filter-workspace");
    assert!(
        !has_key(&rt, "extension-group-workspace"),
        "the workspace filter renders no group headers"
    );
    assert_eq!(cards(&rt).len(), 2, "the flat grid still lists both");
}

#[test]
fn the_search_narrows_cards_across_name_id_and_description() {
    // React (`WappManagementPage.test.tsx:230-240`): the box narrows across
    // the name, the id and the description, case-insensitively.
    let mut rt = page_runtime(
        vec![
            wapp(),
            WappSummaryVm { id: "acme.afi".into(), name: "AFI Pricing".into(), ..wapp() },
        ],
        (1440.0, 900.0),
        true,
    );

    // The name carries the match.
    search(&mut rt, "afi");
    assert_eq!(cards(&rt).len(), 1, "the name narrows the list");
    assert!(has_key(&rt, "extension-card-acme.afi-name"));

    // A needle nobody carries narrows to nothing (and the empty state takes
    // the grid's place, exactly as React's `filtered.length === 0`).
    search(&mut rt, "midnight");
    assert_eq!(cards(&rt).len(), 0, "an id nobody carries narrows to nothing");
    assert!(has_key(&rt, "extension-page-empty"), "the empty state mounts");

    // The description carries the match, whatever the case.
    search(&mut rt, "QUOTAS");
    assert_eq!(cards(&rt).len(), 2, "the search is case-insensitive over the description");
}

#[test]
fn nothing_matching_renders_the_empty_state() {
    // React (`WappManagementPage.test.tsx:242-249`): an empty installed list
    // renders the empty state and no card.
    let rt = page_runtime(Vec::new(), (1440.0, 900.0), true);

    assert!(has_key(&rt, "extension-page-empty"));
    assert_eq!(
        element(&rt, "extension-page-empty-title").text.as_deref(),
        Some(t("extensions.page.emptyTitle")),
        "the empty title reads the table"
    );
    assert!(cards(&rt).is_empty(), "no card mounts");
    // The page still carries its header and search row.
    assert!(has_key(&rt, "extension-page-title"));
    assert!(has_key(&rt, "extension-search-input"));
}

#[test]
fn entering_the_section_fetches_the_unloaded_store_once() {
    // React's mount effect (`WappManagementPage.tsx:141-143`:
    // `useEffect(() => { if (!loaded) void fetch(); }, [loaded])`) — the
    // store fetches the moment the page mounts, which the rail's entry into
    // the section is here; the unwired adapter answers with the fetch's
    // honest seam error, which the page's error row renders. Once loaded,
    // re-entering fetches nothing (the `!loaded` guard).
    let mut state = AppState::default();
    state.select(Section::Wapps);
    assert!(state.wapps_loaded, "entering the section ran the store's fetch");
    let mut rt = Runtime::new(
        state,
        view,
        (1440.0, 900.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "extension-page-error").text.as_deref(),
        Some(
            "wapp list needs the live host; the wapps list use case is not wired into this Rune build yet"
        ),
        "the fetch's seam error renders in the store's error row"
    );
    assert!(
        has_key(&rt, "extension-page-empty"),
        "the empty list still renders its state under the error"
    );

    // The guard: a loaded store re-enters the section without refetching.
    rt.state.wapps_loaded(vec![wapp()]);
    rt.state.select(Section::Wapps);
    rt.invalidate();
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(
        has_key(&rt, "extension-card-acme.bill-audit-name"),
        "the loaded list renders"
    );
    assert!(
        !has_key(&rt, "extension-page-error"),
        "a loaded store does not refetch on entry"
    );
}

#[test]
fn the_uninstall_confirm_asks_by_name_and_the_answers_route() {
    // React (`WappManagementPage.test.tsx:284-309`): the uninstall press
    // asks with the title and the message naming the wapp; confirm runs
    // the store's remove, cancel keeps the wapp.
    let mut rt = page_runtime(vec![wapp()], (1440.0, 900.0), true);

    // No ask is pending: nothing over the page.
    assert!(!has_key(&rt, "wapp-remove-dialog"), "no ask, no dialog");

    click(&mut rt, "extension-card-acme.bill-audit-uninstall");
    assert!(has_key(&rt, "wapp-remove-dialog"), "the ask opened the confirm");
    // The dialog's mount reveal has to land before a press reaches it, as
    // the live window's frames would (the rail-modals suite settles the
    // same way before pressing inside a dialog).
    rt.settle_all();
    rt.frame(0.0);
    assert_eq!(
        element(&rt, "wapp-remove-dialog-message").text.as_deref(),
        Some(t("extensions.page.uninstallMessage").replace("{name}", "Bill Audit").as_str()),
        "the message names the wapp"
    );
    assert_eq!(
        element(&rt, "extension-remove-cancel").label.as_deref(),
        Some(t("common.cancel")),
        "the cancel answer carries the dialog default label"
    );
    assert_eq!(
        element(&rt, "extension-remove-confirm").label.as_deref(),
        Some(t("common.confirm")),
        "the confirm answer carries the confirm label"
    );

    // Cancel keeps the wapp: no error, the card stays, the dialog leaves.
    click(&mut rt, "extension-remove-cancel");
    assert!(!has_key(&rt, "wapp-remove-dialog"), "the cancel settled the ask");
    assert!(has_key(&rt, "extension-card-acme.bill-audit-name"), "the wapp stays");
    assert!(!has_key(&rt, "extension-page-error"), "cancel runs nothing");

    // Confirm runs the store's remove — the seam's honest failure branch,
    // the error React's catch lands in the page's action slot.
    click(&mut rt, "extension-card-acme.bill-audit-uninstall");
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "extension-remove-confirm");
    assert!(!has_key(&rt, "wapp-remove-dialog"), "the confirm settled the ask");
    assert_eq!(
        element(&rt, "extension-page-error").text.as_deref(),
        Some(
            "wapp uninstall needs the live host; the wapps uninstall use case is not wired into this Rune build yet"
        ),
        "the remove seam's error lands in the page's action slot"
    );
}

#[test]
fn escape_is_the_cancel_answer_and_a_settled_ask_cannot_settle_twice() {
    // React wires `onClose={onCancel}`: the header close and Escape are the
    // cancel answer; and the promise resolves one value only — a stray
    // second answer changes nothing.
    let mut rt = page_runtime(vec![wapp()], (1440.0, 900.0), true);

    click(&mut rt, "extension-card-acme.bill-audit-uninstall");
    escape(&mut rt);
    assert!(!has_key(&rt, "wapp-remove-dialog"), "Escape took the ask down");
    assert!(has_key(&rt, "extension-card-acme.bill-audit-name"));
    assert!(
        rt.state.wapp_remove_request.is_none(),
        "Escape answered cancel"
    );
    assert!(
        rt.state.wapp_action_error.is_none(),
        "Escape ran no remove"
    );

    // A settled ask cannot settle twice: the stray answer lands on nothing.
    rt.state.answer_remove_wapp(true);
    assert!(
        rt.state.wapp_action_error.is_none(),
        "the second answer changed nothing"
    );
}

#[test]
fn the_remove_success_transition_drops_the_row_and_refreshes() {
    // The store's remove success — the row leaves, the tabs close, and the
    // refresh runs (`WappManagementPage.tsx:196-198`:
    // `await remove(wapp.id)` then `await get().fetch()`) — is the
    // `wapp_removed` transition the host wiring will call; the refresh
    // lands the fetch's honest seam error in the store's error row.
    let mut rt = page_runtime(vec![wapp(), theme_wapp()], (1440.0, 900.0), true);
    rt.state.wapp_removed("acme.bill-audit");
    rt.invalidate();
    rt.frame(0.0);
    rt.frame(0.0);

    assert!(
        !has_key(&rt, "extension-card-acme.bill-audit-name"),
        "the removed wapp's card left the list"
    );
    assert_eq!(
        element(&rt, "extension-page-count").text.as_deref(),
        Some(t("extensions.page.count").replace("{count}", "1").as_str()),
        "the count header reads the shrunk list"
    );
    assert_eq!(
        element(&rt, "extension-page-error").text.as_deref(),
        Some(
            "wapp list needs the live host; the wapps list use case is not wired into this Rune build yet"
        ),
        "the refresh's seam error lands in the store's error row"
    );
}

#[test]
fn the_toggle_flips_the_switch_and_lands_the_seam_error() {
    // React (`WappManagementPage.test.tsx:251-282`): the toggle press runs
    // the store's `setEnabled`; a failure surfaces the store's error in the
    // page's catch slot. The optimistic flip survives, exactly as React's
    // does — observed through the Open action's enabled gate, the tree's
    // focusability channel.
    let mut rt = page_runtime(vec![wapp()], (1440.0, 900.0), true);

    assert!(
        element(&rt, "extension-card-acme.bill-audit-open").focusable,
        "an enabled wapp's Open action is live"
    );
    click(&mut rt, "extension-card-acme.bill-audit-toggle");
    assert_eq!(
        element(&rt, "extension-page-error").text.as_deref(),
        Some(
            "wapp enable/disable needs the live host; the wapps enable use case is not wired into this Rune build yet"
        ),
        "the enable seam's error lands in the page's action slot"
    );
    assert!(
        !element(&rt, "extension-card-acme.bill-audit-open").focusable,
        "the optimistic flip disabled the wapp: its Open action went inert"
    );

    // React's second branch (`WappManagementPage.test.tsx:262-270`):
    // enabling a disabled plugin runs the same store call without closing
    // tabs — here the flip back re-enables the Open action.
    click(&mut rt, "extension-card-acme.bill-audit-toggle");
    assert!(
        element(&rt, "extension-card-acme.bill-audit-open").focusable,
        "the flip back re-enabled the Open action"
    );
}

#[test]
fn api_mismatched_cards_are_dimmed_and_gated() {
    // React (`WappManagementPage.test.tsx:349-361`): an API-mismatched wapp
    // renders dimmed with the mismatch badge, and its toggle/open actions
    // are blocked. The dim is the tree's opacity channel; the gate is the
    // focusability channel (React asserts `disabled`).
    let mismatched = WappSummaryVm { api_version: WAPP_API_VERSION + 1, ..wapp() };
    let rt = page_runtime(vec![mismatched], (1440.0, 900.0), true);

    let card = element(&rt, "extension-card-acme.bill-audit");
    assert!(
        (card.opacity - 0.6).abs() < 0.01,
        "the whole card composites at React's 60% (opacity={})",
        card.opacity
    );
    assert_eq!(
        element(&rt, "extension-card-acme.bill-audit-badge-api-mismatch")
            .label
            .as_deref(),
        Some(t("extensions.page.apiMismatch")),
        "the mismatch badge renders"
    );
    assert!(
        !element(&rt, "extension-card-acme.bill-audit-toggle").focusable,
        "the toggle is blocked on a mismatch"
    );
    assert!(
        !has_key(&rt, "extension-card-acme.bill-audit-open"),
        "the Open action is not rendered on a mismatch"
    );
    assert!(has_key(&rt, "extension-card-acme.bill-audit-uninstall"), "uninstall stays");
}

#[test]
fn a_disabled_wapp_is_dimmed_like_a_mismatched_one() {
    // React's dim has two halves (`WappManagementPage.tsx:211`:
    // `const dimmed = apiMismatch || !wapp.enabled`) — the mismatch half is
    // pinned above; the enabled half dims the disabled card at the same
    // 60%, its uninstall still live (React dims, it does not disable).
    let disabled = WappSummaryVm { enabled: false, ..wapp() };
    let rt = page_runtime(vec![disabled], (1440.0, 900.0), true);

    let card = element(&rt, "extension-card-acme.bill-audit");
    assert!(
        (card.opacity - 0.6).abs() < 0.01,
        "the disabled wapp's card composites at React's 60% (opacity={})",
        card.opacity
    );
    assert!(
        has_key(&rt, "extension-card-acme.bill-audit-uninstall"),
        "the dimmed card's uninstall stays live"
    );
}

#[test]
fn theme_only_cards_carry_the_hint_and_no_open_action() {
    // React (`WappManagementPage.test.tsx:329-347`): a theme-only wapp's
    // card carries the theme hint and badge and no Open action. The Theme
    // chip selects the view (the Workspace default hides theme-only wapps).
    let mut rt = page_runtime(vec![theme_wapp()], (1440.0, 900.0), true);
    click(&mut rt, "extension-filter-theme");

    assert_eq!(
        element(&rt, "extension-card-acme.midnight-theme-hint").text.as_deref(),
        Some(t("extensions.page.themeHint")),
        "the settings hint renders"
    );
    assert_eq!(
        element(&rt, "extension-card-acme.midnight-badge-theme").label.as_deref(),
        Some(t("extensions.page.themeBadge")),
        "the theme badge renders"
    );
    assert!(
        !has_key(&rt, "extension-card-acme.midnight-open"),
        "a theme-only wapp has no Open action"
    );
}

#[test]
fn the_open_action_switches_to_the_workspace_section() {
    // React (`WappManagementPage.test.tsx:311-327`): the Open press opens
    // the wapp's first workspace page and switches the view to the
    // workspace — the tabs seam carries the payload; the section switch is
    // what the Rune build lands.
    let mut rt = page_runtime(vec![wapp()], (1440.0, 900.0), true);
    assert_eq!(rt.state.active, Section::Wapps);

    click(&mut rt, "extension-card-acme.bill-audit-open");
    assert_eq!(
        rt.state.active, Section::Workspace,
        "the Open press switches the window to the workspace"
    );
    assert!(
        !has_key(&rt, "extension-management-page"),
        "the management page left with the section"
    );

    // The gate: `open_wapp_page` answers false for a wapp with no pages
    // (the card renders no Open action for it either).
    let mut rt = page_runtime(vec![theme_wapp()], (1440.0, 900.0), true);
    assert!(
        !rt.state.open_wapp_page("acme.midnight"),
        "a wapp with no pages has nothing to open"
    );
}

#[test]
fn the_install_press_hands_the_t022_dialog_the_page_hosts() {
    // React (`WappManagementPage.test.tsx:363-386`): the install press
    // opens the two-step dialog over the page; the flow itself is T022's
    // own suite (`install_wapp_dialog.rs`).
    let mut rt = page_runtime(vec![wapp()], (1440.0, 900.0), true);

    assert!(!has_key(&rt, "install-wapp-dialog"), "the dialog starts closed");
    assert_eq!(
        element(&rt, "extension-install-button").label.as_deref(),
        Some(t("extensions.page.install")),
        "the install press carries its label"
    );
    click(&mut rt, "extension-install-button");
    assert!(has_key(&rt, "install-wapp-dialog"), "the install press opened it");
    assert!(
        rt.state.install_wapp_open,
        "the open press landed in the state"
    );
}

#[test]
fn the_page_fits_both_window_sizes_and_both_themes() {
    // The DoD's geometry contract: at 960×640 and 1440×816, both themes,
    // every laid card stays inside the window horizontally, and the
    // centered content column caps at React's max-w-5xl (1024).
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut rt = page_runtime(vec![wapp(), theme_wapp()], (w, h), dark);
            // The geometry contract walks both kinds of card, and the default
            // Workspace filter hides the theme-only wapp (the filter contract
            // above pins that), so the "all" view is what lays both out.
            click(&mut rt, "extension-filter-all");
            let content = element(&rt, "extension-page-content");
            assert!(
                content.rect[0] >= -0.5 && content.rect[0] + content.rect[2] <= w + 0.5,
                "content out of {w}x{h} (dark={dark}): {:?}",
                content.rect
            );
            assert!(
                content.rect[2] <= 1024.5,
                "the content column keeps React's max-w-5xl: {:?}",
                content.rect
            );
            for key in ["extension-card-acme.bill-audit", "extension-card-acme.midnight"] {
                let card = element(&rt, key);
                assert!(
                    card.rect[0] >= -0.5 && card.rect[0] + card.rect[2] <= w + 0.5,
                    "{key} out of {w}x{h} (dark={dark}): {:?}",
                    card.rect
                );
                assert!(
                    card.rect[2] >= 259.5,
                    "{key} keeps the 260pt grid floor: {:?}",
                    card.rect
                );
            }
        }
    }
}

#[test]
fn the_remove_confirm_fits_both_window_sizes_and_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = AppState::default();
            state.wapps_loaded(vec![wapp()]);
            state.select(Section::Wapps);
            state.ask_remove_wapp("acme.bill-audit");
            let mut rt = Runtime::new(
                state,
                view,
                (w, h),
                Env { system_dark: dark, scale: 1.0 },
            );
            rt.frame(0.0);
            rt.frame(0.0);

            let panel = element(&rt, "wapp-remove-dialog");
            assert_eq!(panel.role, "dialog", "the panel carries the dialog role");
            assert!(
                panel.rect[0] >= -0.5 && panel.rect[0] + panel.rect[2] <= w + 0.5,
                "panel out of {w}x{h} (dark={dark}): {:?}",
                panel.rect
            );
            assert!(
                panel.rect[1] >= -0.5 && panel.rect[1] + panel.rect[3] <= h + 0.5,
                "panel out of {w}x{h} vertically (dark={dark}): {:?}",
                panel.rect
            );
            assert!(
                panel.rect[2] <= 512.5,
                "the panel keeps React's max-w-lg: {:?}",
                panel.rect
            );
            let center = panel.rect[0] + panel.rect[2] / 2.0;
            assert!(
                (center - w / 2.0).abs() < 0.5,
                "the layer centers the panel at {center}, want {}: {:?}",
                w / 2.0,
                panel.rect
            );
        }
    }
}

#[test]
fn no_i18n_key_echoes_into_the_rendered_tree() {
    // The zh-CN table backs every label; a raw key leaking through means a
    // missing entry, which the fallback would render as the key itself.
    let rt = page_runtime(vec![wapp(), theme_wapp()], (1440.0, 900.0), true);
    let leak = rt
        .elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter(|e| {
            e.text.as_deref().is_some_and(|t| t.contains("extensions."))
                || e.label.as_deref().is_some_and(|t| t.contains("extensions."))
        })
        .map(|e| e.key.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(leak.is_empty(), "raw i18n keys leaked into: {leak:?}");
}
