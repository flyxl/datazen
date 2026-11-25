//! T224: the `workspacePages.ts` port — plain unit tests on the pure
//! derivation, no view runtime (the module under test owns no `El`). The
//! contract is the React file's own behavior: the enabled-wapp × page
//! flattening in install order, the per-page field mapping with its
//! `page.title || wapp.name` fallback, the tab payload `buildWorkspaceTab`
//! produces, and the `openWappPage` resolve gates (missing/disabled/
//! page-less wapp, unknown page, the falsy-`pageId`-means-first-page edge).
//! The wapp fixtures follow `WappManagementPage.test.tsx`'s shapes so the
//! two suites read against the same installed-list world.

use std::rc::Rc;

use datazen_ui::views::wapp_page_shell::WorkspaceTabVm;
use datazen_ui::views::workspace_pages::{
    build_workspace_tab, derive_workspace_pages, open_wapp_page, workspace_tab_key,
    OpenPageEventPayload, OpenWorkspaceTabSlot, WAPPS_OPEN_PAGE_EVENT,
};
use datazen_ui::views::{WappPageSummaryVm, WappSummaryVm, WAPP_API_VERSION};

/// One page contribution (`types/wapp.ts`'s `WappPageSummary`).
fn page(id: &str, title: &str) -> WappPageSummaryVm {
    WappPageSummaryVm {
        id: id.into(),
        title: title.into(),
        icon: None,
    }
}

/// One page contribution carrying its own icon path.
fn page_with_icon(id: &str, title: &str, icon: &str) -> WappPageSummaryVm {
    WappPageSummaryVm {
        icon: Some(icon.into()),
        ..page(id, title)
    }
}

/// One installed wapp row (`types/wapp.ts`'s `WappSummary`), with the
/// metadata the entry mapping reads.
fn wapp(id: &str, name: &str, enabled: bool, pages: Vec<WappPageSummaryVm>) -> WappSummaryVm {
    WappSummaryVm {
        id: id.into(),
        name: name.into(),
        version: "1.2.0".into(),
        api_version: WAPP_API_VERSION,
        author: Some("Acme".into()),
        description: Some("Compare bills against quotas".into()),
        icon: Some("package-icon.png".into()),
        enabled,
        permissions: Vec::new(),
        pages,
        themes: Vec::new(),
    }
}

/// The workspace fixture: an enabled wapp contributing two pages, the first
/// with its own icon.
fn bill_audit() -> WappSummaryVm {
    wapp(
        "acme.bill-audit",
        "Bill Audit",
        true,
        vec![
            page_with_icon("quota-check", "Quota Check", "quota.svg"),
            page("history", "History"),
        ],
    )
}

/// The tabs-store `open` seam's recorder: the host state is the opened-tab
/// log, and the slot appends to it — the store double React's own tests use.
fn open_log() -> (Vec<WorkspaceTabVm>, OpenWorkspaceTabSlot<Vec<WorkspaceTabVm>>) {
    (Vec::new(), Rc::new(|tabs: &mut Vec<WorkspaceTabVm>, tab| tabs.push(tab)))
}

// -- the event name ----------------------------------------------------------

#[test]
fn event_name_mirrors_the_backend_protocol() {
    // React pins the literal against `src-tauri/src/wapps/protocol.rs`; the
    // kernel constant is `crates/datazen/src/wapps/protocol.rs`'s
    // `WAPPS_OPEN_PAGE_EVENT`.
    assert_eq!(WAPPS_OPEN_PAGE_EVENT, "wapps:open-page");
}

// -- the derivation ----------------------------------------------------------

#[test]
fn derives_enabled_wapp_pages_in_install_order() {
    let wapps = vec![
        // A disabled wapp with pages contributes nothing.
        wapp("corp.disabled", "Disabled", false, vec![page("p", "P")]),
        bill_audit(),
        // An enabled wapp contributing no pages contributes nothing either.
        wapp("corp.themes", "Themes", true, Vec::new()),
        wapp("corp.data-sync", "Data Sync", true, vec![page("sync", "Sync")]),
    ];
    let pages = derive_workspace_pages(&wapps);
    // Wapp-major order: the wapp list's order, each wapp's pages in their
    // contribution order (React's `filter` + `flatMap`).
    let keys: Vec<&str> = pages.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "acme.bill-audit:quota-check",
            "acme.bill-audit:history",
            "corp.data-sync:sync",
        ]
    );
}

#[test]
fn derive_on_an_empty_or_barren_list_is_empty() {
    assert!(derive_workspace_pages(&[]).is_empty());
    assert!(derive_workspace_pages(&[wapp("corp.themes", "Themes", true, Vec::new())]).is_empty());
}

#[test]
fn entry_maps_the_page_and_wapp_fields() {
    let wapp = bill_audit();
    let pages = derive_workspace_pages(std::slice::from_ref(&wapp));
    assert_eq!(pages.len(), 2);

    let quota = &pages[0];
    assert_eq!(quota.wapp_id, "acme.bill-audit");
    assert_eq!(quota.page_id, "quota-check");
    assert_eq!(quota.title, "Quota Check");
    // React: `icon: page.icon` — the page's own icon, not the package icon.
    assert_eq!(quota.icon.as_deref(), Some("quota.svg"));
    // version/author/description are the wapp's.
    assert_eq!(quota.version, "1.2.0");
    assert_eq!(quota.author.as_deref(), Some("Acme"));
    assert_eq!(quota.description.as_deref(), Some("Compare bills against quotas"));

    // The second page has no icon of its own — `None`, no package fallback.
    assert_eq!(pages[1].icon, None);
}

#[test]
fn empty_page_title_falls_back_to_the_wapp_name() {
    // React: `title: page.title || wapp.name` — truthiness, so only the
    // empty string falls back; whitespace survives as the title.
    let wapp = wapp(
        "acme.sparse",
        "Sparse",
        true,
        vec![page("untitled", ""), page("blank", " ")],
    );
    let pages = derive_workspace_pages(std::slice::from_ref(&wapp));
    assert_eq!(pages[0].title, "Sparse");
    assert_eq!(pages[1].title, " ");
}

#[test]
fn tab_key_is_wapp_id_colon_page_id() {
    assert_eq!(workspace_tab_key("acme.bill-audit", "quota-check"), "acme.bill-audit:quota-check");
}

// -- the tab payload ---------------------------------------------------------

#[test]
fn build_workspace_tab_carries_the_tab_fields_and_drops_the_rest() {
    let wapp = bill_audit();
    let tab = build_workspace_tab(&wapp, &wapp.pages[0]);
    // The T067 `WorkspaceTabVm`: the entry minus author/description (the
    // tab carries neither).
    assert_eq!(
        tab,
        WorkspaceTabVm {
            key: "acme.bill-audit:quota-check".into(),
            wapp_id: "acme.bill-audit".into(),
            page_id: "quota-check".into(),
            title: "Quota Check".into(),
            icon: Some("quota.svg".into()),
            version: "1.2.0".into(),
        }
    );
    // And it matches the derivation's own row for the same page.
    let entry = &derive_workspace_pages(std::slice::from_ref(&wapp))[0];
    assert_eq!(tab.key, entry.key);
    assert_eq!(tab.title, entry.title);
    assert_eq!(tab.icon, entry.icon);
    assert_eq!(tab.version, entry.version);
}

// -- the resolve-then-open ---------------------------------------------------

#[test]
fn open_without_a_page_id_opens_the_first_page() {
    let wapps = vec![bill_audit()];
    let (mut opened, open) = open_log();
    assert!(open_wapp_page(&wapps, "acme.bill-audit", None, &open, &mut opened));
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].page_id, "quota-check");
    assert_eq!(opened[0].key, "acme.bill-audit:quota-check");
}

#[test]
fn open_with_an_empty_page_id_opens_the_first_page() {
    // React: `pageId ? find(...) : pages[0]` — the empty string is falsy,
    // so it behaves exactly like no page id.
    let wapps = vec![bill_audit()];
    let (mut opened, open) = open_log();
    assert!(open_wapp_page(&wapps, "acme.bill-audit", Some(""), &open, &mut opened));
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].page_id, "quota-check");
}

#[test]
fn open_resolves_the_named_page() {
    let wapps = vec![bill_audit()];
    let (mut opened, open) = open_log();
    assert!(open_wapp_page(&wapps, "acme.bill-audit", Some("history"), &open, &mut opened));
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].page_id, "history");
    assert_eq!(opened[0].title, "History");
}

#[test]
fn open_refuses_and_opens_nothing_for_a_missing_wapp() {
    let wapps = vec![bill_audit()];
    let (mut opened, open) = open_log();
    assert!(!open_wapp_page(&wapps, "acme.gone", Some("quota-check"), &open, &mut opened));
    assert!(opened.is_empty());
}

#[test]
fn open_refuses_a_disabled_wapp() {
    let wapps = vec![wapp("corp.disabled", "Disabled", false, vec![page("p", "P")])];
    let (mut opened, open) = open_log();
    assert!(!open_wapp_page(&wapps, "corp.disabled", None, &open, &mut opened));
    assert!(!open_wapp_page(&wapps, "corp.disabled", Some("p"), &open, &mut opened));
    assert!(opened.is_empty());
}

#[test]
fn open_refuses_a_wapp_with_no_pages() {
    let wapps = vec![wapp("corp.themes", "Themes", true, Vec::new())];
    let (mut opened, open) = open_log();
    assert!(!open_wapp_page(&wapps, "corp.themes", None, &open, &mut opened));
    assert!(opened.is_empty());
}

#[test]
fn open_refuses_an_unknown_page_id() {
    let wapps = vec![bill_audit()];
    let (mut opened, open) = open_log();
    assert!(!open_wapp_page(&wapps, "acme.bill-audit", Some("nope"), &open, &mut opened));
    assert!(opened.is_empty());
}

#[test]
fn open_resolves_per_wapp_not_globally() {
    // Two enabled wapps can share a page id; the resolve only looks inside
    // the named wapp.
    let wapps = vec![
        bill_audit(),
        wapp("corp.data-sync", "Data Sync", true, vec![page("quota-check", "Sync Quota")]),
    ];
    let (mut opened, open) = open_log();
    assert!(open_wapp_page(&wapps, "corp.data-sync", Some("quota-check"), &open, &mut opened));
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].key, "corp.data-sync:quota-check");
    assert_eq!(opened[0].title, "Sync Quota");
}

// -- the deep-link payload ---------------------------------------------------

#[test]
fn open_page_payload_defaults_to_no_target() {
    // A raw event that is not an object with string `wappId`/`pageId`
    // fields maps to `None`s and fails the truthiness gates downstream
    // (the T200 deep-link body reads it).
    assert_eq!(
        OpenPageEventPayload::default(),
        OpenPageEventPayload { wapp_id: None, page_id: None }
    );
}
