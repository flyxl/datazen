//! T253: the language-pack store's coder tests — the fetch seam's two
//! endings (the populated list with `byCode`'s hit and miss, the failed
//! load that still counts as loaded), the remove seam (the effect
//! payload, the success path's refetch hand-off, the failure's error
//! without touching `loaded`), and the changed listener's once-latch
//! with its three retry paths.

use datazen_ui::language_pack_store::{
    LanguagePackEffect, LanguagePackStore, LanguagePackSummary, LanguagePacksChangedListener,
    ListenerOutcome, LANGUAGE_PACKS_CHANGED_EVENT,
};

fn pack(code: &str) -> LanguagePackSummary {
    LanguagePackSummary {
        code: code.to_owned(),
        label: if code == "ja" { "日本語".to_owned() } else { format!("{code} pack") },
        version: "1.0.0".to_owned(),
        author: Some("DataZen".to_owned()),
        description: None,
        message_count: 3,
        file_name: code.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// The fetch seam (React 24–31)
// ---------------------------------------------------------------------------

#[test]
fn a_successful_fetch_populates_the_list_and_the_lookup() {
    let mut store = LanguagePackStore::new();
    assert_eq!(store.begin_fetch(), LanguagePackEffect::ListPacks);
    store.complete_fetch(vec![pack("ja")]);

    assert_eq!(store.packs, vec![pack("ja")]);
    assert!(store.loaded);
    assert!(store.error.is_none());
    assert_eq!(store.by_code("ja"), Some(&pack("ja")));
    assert_eq!(store.by_code("de"), None, "byCode misses an unknown code");
}

#[test]
fn a_failed_fetch_records_the_error_but_stays_loaded() {
    // React 50-57: `set({ loaded: true, error })` — the card shows the
    // error, never a spinner again.
    let mut store = LanguagePackStore::new();
    store.fail_fetch("boom");
    assert!(store.loaded);
    assert_eq!(store.error.as_deref(), Some("boom"));
    assert!(store.packs.is_empty());
}

#[test]
fn a_refetch_after_a_failure_clears_the_error() {
    let mut store = LanguagePackStore::new();
    store.fail_fetch("boom");
    store.begin_fetch();
    store.complete_fetch(vec![pack("ja")]);
    assert!(store.error.is_none(), "the success clears the old error");
    assert_eq!(store.packs.len(), 1);
}

// ---------------------------------------------------------------------------
// The remove seam (React 33–41)
// ---------------------------------------------------------------------------

#[test]
fn a_remove_delegates_the_code_and_refetches_through_the_seam() {
    // React 59-66: the removal succeeds, the list is whatever the
    // refetch says — the remove itself writes nothing.
    let mut store = LanguagePackStore::new();
    store.complete_fetch(vec![pack("ja")]);

    assert_eq!(
        store.begin_remove("ja"),
        LanguagePackEffect::RemovePack { code: "ja".to_owned() }
    );
    // The host's success path: the refetch seam runs (begin_fetch →
    // complete_fetch), exactly React's `await get().fetch()`.
    store.begin_fetch();
    store.complete_fetch(vec![pack("ja"), pack("de")]);
    assert_eq!(store.packs.len(), 2, "the refetch owns the list");
}

#[test]
fn a_failed_remove_surfaces_the_error_without_touching_loaded() {
    // React 68-75: `set({ error })` only — `loaded` keeps whatever the
    // last fetch left, and the rethrow is the host's `Err`.
    let mut store = LanguagePackStore::new();
    store.complete_fetch(vec![pack("ja")]);
    assert!(store.loaded);

    store.begin_remove("ja");
    store.fail_remove("language pack not found: ja");
    assert_eq!(store.error.as_deref(), Some("language pack not found: ja"));
    assert!(store.loaded, "the remove failure never flips loaded");
    assert_eq!(store.packs.len(), 1, "the remove failure keeps the list");
}

// ---------------------------------------------------------------------------
// The changed listener (React 46–74)
// ---------------------------------------------------------------------------

#[test]
fn the_event_spelling_is_the_react_constant() {
    assert_eq!(LANGUAGE_PACKS_CHANGED_EVENT, "language-packs:changed");
}

#[test]
fn a_missing_platform_retries_later() {
    // React 57-61: not assembled yet — the flag resets so a later call
    // retries.
    let mut listener = LanguagePacksChangedListener::new();
    assert_eq!(
        listener.ensure(false, false, false),
        ListenerOutcome::PlatformMissing
    );
    assert!(!listener.is_started());
    // The retry lands once the platform assembles.
    assert_eq!(
        listener.ensure(true, false, false),
        ListenerOutcome::Listening
    );
    assert!(listener.is_started());
}

#[test]
fn the_once_rule_holds_after_the_first_successful_subscription() {
    let mut listener = LanguagePacksChangedListener::new();
    assert_eq!(listener.ensure(true, false, false), ListenerOutcome::Listening);
    // Every later call is a no-op that answers Listening.
    for _ in 0..3 {
        assert_eq!(listener.ensure(true, false, false), ListenerOutcome::Listening);
        assert_eq!(listener.ensure(false, true, true), ListenerOutcome::Listening);
    }
    assert!(listener.is_started());
}

#[test]
fn a_rejected_listen_or_a_thrown_call_both_reset_the_latch() {
    // React 65-70 (the promise's catch) and 71-73 (the sync throw):
    // either way the flag resets and a later call retries.
    let mut listener = LanguagePacksChangedListener::new();
    assert_eq!(
        listener.ensure(true, true, false),
        ListenerOutcome::SubscribeFailed
    );
    assert!(!listener.is_started());
    assert_eq!(
        listener.ensure(true, false, true),
        ListenerOutcome::ListenThrew
    );
    assert!(!listener.is_started());
    // The retry after either failure can still land.
    assert_eq!(listener.ensure(true, false, false), ListenerOutcome::Listening);
}
