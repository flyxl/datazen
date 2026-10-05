// BUG-003: a refused `DBSIZE` degrades the budget tier, it never fails a
// command.

use super::*;

// ---------------------------------------------------------------------------
// BUG-003: a refused DBSIZE degrades the budget tier, it never fails a command
// ---------------------------------------------------------------------------

/// `read_dbsize` is an input to a budget and a display number, not a hard
/// dependency: an ACL profile without `DBSIZE`, a proxy that hides it, or one
/// unreachable master under the cluster fan-out must leave the key tree working
/// with the default tier (redis-tree-backend-BUG-003).
#[tokio::test]
async fn read_dbsize_never_fails_and_reports_zero_when_refused() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 777;
    conn.state().dbsize_refused = true;

    assert_eq!(
        read_dbsize(&mut conn).await,
        0,
        "a refused DBSIZE reads as 0, exactly like the 8981d3078 baseline's unwrap_or(0)"
    );

    // An unusable-but-successful reply is the same degraded case.
    let mut garbled = TreeConn::new();
    garbled.state().dbsize_reply = Some(bulk("not-a-number"));
    assert_eq!(read_dbsize(&mut garbled).await, 0);

    // 0 is what makes `tree_scan_budget` fall back to the default tier, which is
    // the frozen rule "DBSIZE 不可得 ⇒ 默认档".
    assert_eq!(tree_scan_budget(None, 0), DEFAULT_TREE_BUDGET);
}

/// `scan_keys` with `DBSIZE` refused: the page still succeeds, carries
/// `dbsize: 0`, and is budgeted at the default tier.
#[tokio::test]
async fn scan_keys_succeeds_when_dbsize_is_refused() {
    let mut conn = TreeConn::new();
    conn.state().dbsize_refused = true;
    conn.seed_string("app:a", 1, "x");
    conn.state()
        .scan_script
        .push_back((0, vec!["app:a".to_string()]));

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
    .expect("a refused DBSIZE must not fail the page");

    assert_eq!(page.dbsize, 0, "the degraded value is published as 0");
    assert_eq!(page.entries.len(), 1, "the keyspace itself is healthy");
    assert_eq!(page.entries[0].key, "app:a");
    assert_eq!(
        page.consumed, TREE_SCAN_MIN_ROUND_COUNT as u64,
        "the default tier buys the same first round an empty-db default would"
    );
}

/// `list_children` with `DBSIZE` refused — and, per BUG-003's warning that the
/// two interact, the leaf rows stay aligned while `dbsize` is degraded.
#[tokio::test]
async fn list_children_succeeds_with_own_attributes_when_dbsize_is_refused() {
    let mut conn = TreeConn::new();
    conn.state().dbsize_refused = true;
    for (key, ttl, len) in [("tree:a", -1i64, 1i64), ("tree:b", 7, 2), ("tree:c", -1, 3)] {
        let mut st = conn.state();
        st.types.insert(key.to_string(), "string".to_string());
        st.ttls.insert(key.to_string(), ttl);
        st.lens.insert(key.to_string(), len);
        st.previews.insert(key.to_string(), bulk("v"));
    }
    conn.state().scan_script.push_back((
        0,
        vec![
            "tree:a".to_string(),
            "tree:b".to_string(),
            "tree:c".to_string(),
        ],
    ));

    let page = list_children_page(
        &mut conn,
        "tree:",
        0,
        100,
        None,
        true,
        None,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("a refused DBSIZE must not fail the tree level");

    assert_eq!(page.dbsize, 0);
    let leaves: Vec<(String, String, i64, u64)> = page
        .entries
        .iter()
        .filter_map(|e| match e {
            ChildEntry::Key {
                key,
                key_type,
                ttl,
                logical_len,
                ..
            } => Some((key.clone(), key_type.clone(), *ttl, *logical_len)),
            _ => None,
        })
        .collect();
    assert_eq!(
        leaves,
        vec![
            ("tree:a".to_string(), "string".to_string(), -1, 1),
            ("tree:c".to_string(), "string".to_string(), -1, 3),
        ],
        "the degraded dbsize path must not disturb per-key attribute binding"
    );
}

/// `count_matching` on `*` when `DBSIZE` is refused: the count is answered from
/// a real scan pass instead of being short-circuited to a fake "empty database"
/// census. An empty keyspace pays one extra round and still answers 0.
#[tokio::test]
async fn count_star_verifies_with_a_scan_when_dbsize_is_refused() {
    let mut conn = TreeConn::new();
    conn.state().dbsize_refused = true;
    // Keyspace with two keys, one round, cursor wraps.
    conn.seed_string("app:a", 1, "x");
    conn.seed_string("app:b", 2, "y");
    conn.state()
        .scan_script
        .push_back((0, vec!["app:a".to_string(), "app:b".to_string()]));

    let outcome = count_budgeted(&mut conn, "*", None, Topology::Standalone)
        .await
        .expect("counting must survive a refused DBSIZE");

    assert_eq!(outcome.dbsize, 0, "the unfilled display value stays 0");
    assert_eq!(
        outcome.count, 2,
        "the census comes from the scan, not from 0"
    );
    assert!(
        !outcome.truncated,
        "the cursor wrapped: this is a full count"
    );
    assert_eq!(outcome.consumed, TREE_SCAN_MIN_ROUND_COUNT as u64);
    {
        let st = conn.state();
        let scan_lines: Vec<String> = joined(&st.singles)
            .iter()
            .filter(|line| line.starts_with("SCAN"))
            .cloned()
            .collect();
        assert_eq!(
            scan_lines.len(),
            1,
            "exactly one verification round is spent"
        );
        assert!(
            !scan_lines[0].contains("MATCH"),
            "counting everything needs no filter argument: {}",
            scan_lines[0]
        );
    }

    // A genuinely empty database takes the same path and answers an honest 0.
    let mut empty = TreeConn::new();
    empty.state().dbsize_refused = true;
    let outcome = count_budgeted(&mut empty, "*", None, Topology::Standalone)
        .await
        .expect("count on an empty keyspace");
    assert_eq!(outcome.count, 0);
    assert!(!outcome.truncated);

    // The other spelling of "everything" (empty pattern) is verified the same way.
    let mut bare = TreeConn::new();
    bare.state().dbsize_refused = true;
    bare.seed_string("k", 1, "x");
    bare.state()
        .scan_script
        .push_back((0, vec!["k".to_string()]));
    let outcome = count_budgeted(&mut bare, "", None, Topology::Standalone)
        .await
        .expect("count with an empty pattern");
    assert_eq!(outcome.count, 1, "an empty pattern counts every key");
    assert_eq!(outcome.dbsize, 0);
}

/// A non-empty `DBSIZE` still short-circuits: the frozen fast path is intact.
#[tokio::test]
async fn count_star_still_short_circuits_when_dbsize_answers() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 42;

    let outcome = count_budgeted(&mut conn, "*", None, Topology::Standalone)
        .await
        .expect("count *");

    assert_eq!(outcome.count, 42);
    assert_eq!(
        outcome.consumed, 0,
        "no scan round is spent on the fast path"
    );
    {
        let st = conn.state();
        assert_eq!(joined(&st.singles), ["DBSIZE"]);
    }
}
