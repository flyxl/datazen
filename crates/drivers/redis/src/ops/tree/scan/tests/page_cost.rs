// Standalone pages: the cost shape. `DBSIZE` exactly once per command, two
// batches per page, the exact-key short circuit, and the budget arms that end a
// walk (`truncated` vs pagination).

use super::*;

// ---------------------------------------------------------------------------
// Standalone pages: the cost shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn page_reads_dbsize_once_and_answers_in_two_batches() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 1_000;
    conn.seed_string("app:a", 3, "hey");
    conn.seed_string("app:b", 5, "hello");
    conn.state().ttls.insert("app:b".to_string(), 5);
    conn.state()
        .scan_script
        .push_back((0, vec!["app:a".to_string(), "app:b".to_string()]));

    let page = scan_keys_page(
        &mut conn,
        "app:*",
        0,
        100,
        None,
        false,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("standalone page");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("DBSIZE"))
                .count(),
            1,
            "DBSIZE is read once per command, never per page or batch"
        );
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("SCAN"))
                .count(),
            1
        );
        assert_eq!(
            st.batches.len(),
            2,
            "one meta batch + one value batch for the whole page"
        );
        assert_eq!(
            joined(&st.batches[0]),
            ["TYPE app:a", "TTL app:a", "TYPE app:b", "TTL app:b"]
        );
        assert_eq!(
            joined(&st.batches[1]),
            ["STRLEN app:a", "GET app:a", "STRLEN app:b", "GET app:b"]
        );
    }

    assert_eq!(page.dbsize, 1_000);
    assert!(!page.exact, "a glob pattern is not an exact key");
    assert!(!page.truncated, "the cursor wrapped inside the budget");
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.entries.len(), 2);
    assert_eq!(
        page.entries[0].size, 3,
        "size comes from the length command"
    );
    assert_eq!(page.entries[0].preview, "hey");
    assert_eq!(page.entries[0].ttl, -1);
    assert_eq!(page.entries[1].ttl, 5);
    // The count hint (100) is floored by TREE_SCAN_MIN_ROUND_COUNT, so one
    // round spends exactly 1 000 of the default 50 000 budget.
    assert_eq!(page.consumed, 1_000);
}

#[tokio::test]
async fn budget_exhaustion_stops_the_scan_and_reports_truncated() {
    let mut conn = TreeConn::new();
    {
        let mut st = conn.state();
        st.dbsize = 90_000;
        st.scan_script
            .push_back((3, (1..=3).map(|i| format!("k:{i}")).collect()));
        st.scan_script
            .push_back((7, (4..=5).map(|i| format!("k:{i}")).collect()));
    }
    for i in 1..=5 {
        conn.seed_string(&format!("k:{i}"), 1, "x");
    }

    let page = scan_keys_page(
        &mut conn,
        "k:*",
        0,
        100,
        None,
        false,
        false,
        Some(1_500),
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("budgeted page");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("SCAN"))
                .count(),
            2,
            "1500 budget = one 1000-count round + one 500-count round, then stop"
        );
    }
    assert_eq!(page.consumed, 1_500, "the ledger never overspends its cap");
    assert!(
        page.truncated,
        "an open cursor when the budget ran out is data the caller did not see"
    );
    assert_eq!(page.next_cursor, 7, "the resume cursor must survive");
    assert_eq!(page.entries.len(), 5);
    assert_eq!(page.dbsize, 90_000);
    // An explicitly requested tier means what it says: no DBSIZE scaling.
    assert_eq!(page.consumed, 1_500);
}

#[tokio::test]
async fn a_full_page_with_an_open_cursor_is_pagination_not_truncation() {
    let mut conn = TreeConn::new();
    conn.seed_string("p:1", 1, "a");
    conn.seed_string("p:2", 1, "b");
    conn.state()
        .scan_script
        .push_back((5, vec!["p:1".to_string(), "p:2".to_string()]));

    let page = scan_keys_page(
        &mut conn,
        "p:*",
        0,
        2,
        None,
        false,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("pagination page");

    assert_eq!(page.entries.len(), 2, "page filled to its count");
    assert_eq!(page.next_cursor, 5, "caller resumes from here");
    assert!(
        !page.truncated,
        "reaching the page limit with the cursor open is pagination, not loss"
    );
}

// ---------------------------------------------------------------------------
// Exact-key short circuit (PRD §4 I-3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exact_key_bypasses_scan_and_reapplies_the_type_filter() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 42;
    conn.state()
        .types
        .insert("app:users:42".to_string(), "hash".to_string());
    conn.state().ttls.insert("app:users:42".to_string(), -1);
    conn.state().lens.insert("app:users:42".to_string(), 4);

    let page = scan_keys_page(
        &mut conn,
        "app:users:42",
        0,
        100,
        Some("hash"),
        false,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("exact page");

    {
        let st = conn.state();
        assert!(
            !joined(&st.singles)
                .iter()
                .any(|line| line.starts_with("SCAN")),
            "an exact key name must never touch SCAN"
        );
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("DBSIZE"))
                .count(),
            1
        );
        assert_eq!(st.batches.len(), 2, "meta + value batches, as for any page");
    }
    assert!(page.exact);
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].key_type, "hash");
    assert_eq!(page.entries[0].key, "app:users:42");
    assert_eq!(page.consumed, 0, "no SCAN round means no COUNT spent");

    // The exact path never sent `SCAN … TYPE`, so a *mismatching* filter the
    // server would have applied must be re-applied client-side — otherwise a
    // filtered tree would show a key the glob path would hide.
    let mut conn2 = TreeConn::new();
    conn2.state().dbsize = 42;
    conn2
        .state()
        .types
        .insert("app:users:42".to_string(), "hash".to_string());
    conn2.state().lens.insert("app:users:42".to_string(), 4);
    let page2 = scan_keys_page(
        &mut conn2,
        "app:users:42",
        0,
        100,
        Some("string"),
        false,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("filtered exact page");
    assert!(page2.exact);
    assert!(
        page2.entries.is_empty(),
        "keyType must exclude a non-matching exact key, exactly like SCAN TYPE"
    );
}

#[tokio::test]
async fn a_degraded_type_reply_is_kept_by_the_exact_filter_rather_than_hidden() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 42;
    conn.state().nil_types.insert("app:users:42".to_string());
    conn.state().lens.insert("app:users:42".to_string(), 4);

    let page = scan_keys_page(
        &mut conn,
        "app:users:42",
        0,
        100,
        Some("string"),
        false,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("degraded TYPE page");

    assert_eq!(
        page.entries.len(),
        1,
        "an unreadable TYPE is not a filter result: hiding the key would lie"
    );
    assert_eq!(page.entries[0].key_type, "none");
}
