//! T253 independent audit — fresh vectors against the language-pack
//! store port, none shared with the coder tests: the wire
//! deserialization of `LanguagePackSummary` (the camelCase shape with
//! the optional pair, the rejects), the store's repeated-fetch
//! overwrite semantics, the remove-then-refetch journey from a failed
//! load, `byCode`'s first-match rule on duplicate codes, and the
//! listener's latch across a platform that assembles late after two
//! different failures.

use serde_json::json;

use crate::language_pack_store::{
    LanguagePackEffect, LanguagePackStore, LanguagePackSummary, LanguagePacksChangedListener,
    ListenerOutcome,
};

fn wire_pack(code: &str, message_count: u64) -> serde_json::Value {
    json!({
        "code": code,
        "label": format!("{code}-label"),
        "version": "2.3.4",
        "author": format!("{code}-author"),
        "description": null,
        "messageCount": message_count,
        "fileName": code,
    })
}

// ---------------------------------------------------------------------------
// The wire shape (T331's seam, carried — see the store's module docs)
// ---------------------------------------------------------------------------

#[test]
fn the_summary_deserializes_the_camelcase_wire_shape() {
    let packs: Vec<LanguagePackSummary> =
        serde_json::from_value(json!([wire_pack("zh-CN", 318), {
            "code": "ja",
            "label": "日本語",
            "version": "1.0.0",
            "author": null,
            "messageCount": 3,
            "fileName": "ja"
        }]))
        .unwrap();
    assert_eq!(packs.len(), 2);
    assert_eq!(packs[0].code, "zh-CN");
    assert_eq!(packs[0].message_count, 318);
    assert_eq!(packs[0].author.as_deref(), Some("zh-CN-author"));
    assert_eq!(packs[1].author, None, "the absent optional stays None");
    assert_eq!(packs[1].label, "日本語");
    assert_eq!(packs[1].file_name, "ja");
}

#[test]
fn a_missing_required_field_rejects_the_list() {
    // The typed deserialization refuses a lying wire (the missing
    // required field), instead of a silent default.
    let missing: Result<Vec<LanguagePackSummary>, _> =
        serde_json::from_value(json!([{ "code": "ja" }]));
    assert!(missing.is_err());
}

// ---------------------------------------------------------------------------
// The fetch / remove seams (fresh journeys)
// ---------------------------------------------------------------------------

#[test]
fn a_second_fetch_overwrites_the_whole_list() {
    let mut store = LanguagePackStore::new();
    store.complete_fetch(vec![LanguagePackSummary {
        code: "ja".to_owned(),
        label: "日本語".to_owned(),
        version: "1.0.0".to_owned(),
        author: None,
        description: Some("first".to_owned()),
        message_count: 3,
        file_name: "ja".to_owned(),
    }]);
    // The refresh after a pack lands elsewhere: the new list replaces
    // the old wholesale, not merges it.
    store.begin_fetch();
    store.complete_fetch(Vec::new());
    assert!(store.packs.is_empty());
    assert!(store.by_code("ja").is_none());
    assert!(store.loaded && store.error.is_none());
}

#[test]
fn a_remove_from_a_failed_load_refetches_into_a_clean_state() {
    // A failed load left its error; the removal succeeds elsewhere and
    // the refetch lands a clean list — the error from the old failure
    // must not survive the new success.
    let mut store = LanguagePackStore::new();
    store.fail_fetch("目录不可读");
    assert_eq!(store.error.as_deref(), Some("目录不可读"));

    let effect = store.begin_remove("ja");
    assert_eq!(effect, LanguagePackEffect::RemovePack { code: "ja".to_owned() });
    store.begin_fetch();
    store.complete_fetch(vec![
        LanguagePackSummary {
            code: "de".to_owned(),
            label: "Deutsch".to_owned(),
            version: "1.2.0".to_owned(),
            author: None,
            description: None,
            message_count: 12,
            file_name: "de".to_owned(),
        }
    ]);
    assert_eq!(store.error, None, "the refetch's success clears the stale error");
    assert_eq!(store.by_code("de").unwrap().message_count, 12);
}

#[test]
fn by_code_takes_the_first_match_among_duplicates() {
    // The wire never sends duplicates, but React's `find` takes the
    // first — the mirror keeps the same rule.
    let mut store = LanguagePackStore::new();
    let first = LanguagePackSummary {
        code: "ja".to_owned(),
        label: "日本語".to_owned(),
        version: "1.0.0".to_owned(),
        author: Some("first".to_owned()),
        description: None,
        message_count: 1,
        file_name: "ja".to_owned(),
    };
    let second = LanguagePackSummary {
        author: Some("second".to_owned()),
        message_count: 2,
        ..first.clone()
    };
    store.complete_fetch(vec![first, second]);
    assert_eq!(store.by_code("ja").unwrap().author.as_deref(), Some("first"));
}

// ---------------------------------------------------------------------------
// The listener (fresh vectors)
// ---------------------------------------------------------------------------

#[test]
fn a_platform_that_assembles_late_after_two_distinct_failures_lands_once() {
    let mut listener = LanguagePacksChangedListener::new();
    // Pre-bootstrap: the platform is missing.
    assert_eq!(listener.ensure(false, false, false), ListenerOutcome::PlatformMissing);
    // Assembled, but the synchronous call throws.
    assert_eq!(listener.ensure(true, false, true), ListenerOutcome::ListenThrew);
    // The next attempt's promise rejects.
    assert_eq!(listener.ensure(true, true, false), ListenerOutcome::SubscribeFailed);
    // The fourth lands and latches; the fifth is the once-no-op.
    assert_eq!(listener.ensure(true, false, false), ListenerOutcome::Listening);
    assert_eq!(listener.ensure(true, false, false), ListenerOutcome::Listening);
    assert!(listener.is_started());
    // The latch survives later platform noise (React never unsets on
    // the happy path).
    assert_eq!(listener.ensure(false, false, false), ListenerOutcome::Listening);
}
