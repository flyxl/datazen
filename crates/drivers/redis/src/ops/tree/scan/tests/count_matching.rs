// `count_matching`: { count, truncated, consumed, dbsize } — and why a partial
// answer is a floor the UI must label `n+`.

use super::*;

// ---------------------------------------------------------------------------
// count_matching: { count, truncated, consumed, dbsize }
// ---------------------------------------------------------------------------

#[tokio::test]
async fn count_star_answers_from_dbsize_without_a_scan() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 777;

    let outcome = count_budgeted(&mut conn, "*", None, Topology::Standalone)
        .await
        .expect("count *");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles),
            ["DBSIZE"],
            "counting everything is one command"
        );
        assert!(st.batches.is_empty());
        assert!(st.addressed.is_empty());
    }
    assert_eq!(outcome.count, 777);
    assert!(!outcome.truncated);
    assert_eq!(outcome.consumed, 0);
    assert_eq!(outcome.dbsize, 777);

    // The wire shape the UI contract freeze pins: exactly these four fields.
    let json = serde_json::to_value(&outcome).expect("CountOutcome serializes");
    assert_eq!(
        json,
        serde_json::json!({
            "count": 777,
            "truncated": false,
            "consumed": 0,
            "dbsize": 777
        })
    );
}

#[tokio::test]
async fn count_exact_is_one_exists_never_a_scan() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 42;
    conn.state()
        .types
        .insert("app:users:42".to_string(), "hash".to_string());

    let hit = count_budgeted(&mut conn, "app:users:42", None, Topology::Standalone)
        .await
        .expect("exact count");
    assert_eq!(hit.count, 1, "PRD §4 I-3: a known key answers 1, not n+");
    assert!(!hit.truncated);
    assert_eq!(hit.consumed, 0);
    {
        let st = conn.state();
        assert_eq!(joined(&st.singles), ["DBSIZE", "EXISTS app:users:42"]);
        assert!(st.batches.is_empty(), "no batch, no SCAN, no value read");
    }

    let mut miss_conn = TreeConn::new();
    miss_conn.state().dbsize = 42;
    let miss = count_budgeted(&mut miss_conn, "app:users:42", None, Topology::Standalone)
        .await
        .expect("exact count of a missing key");
    assert_eq!(miss.count, 0);
    assert!(!miss.truncated);
}

#[tokio::test]
async fn count_cluster_addresses_the_existence_probe_to_the_keys_slot() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 42;
    conn.state()
        .types
        .insert("{user1000}:profile".to_string(), "string".to_string());

    let outcome = count_budgeted(&mut conn, "{user1000}:profile", None, Topology::Cluster)
        .await
        .expect("cluster exact count");
    assert_eq!(outcome.count, 1);

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles),
            ["DBSIZE"],
            "DBSIZE stays unrouted on every topology (redis sums all masters)"
        );
        assert_eq!(st.addressed.len(), 1, "the probe is one addressed command");
        let (slot, cmds) = &st.addressed[0];
        assert_eq!(joined(cmds), ["EXISTS {user1000}:profile"]);
        assert_eq!(
            *slot,
            get_slot(b"{user1000}:profile"),
            "the probe must run on the shard owning the key"
        );
    }
}

#[tokio::test]
async fn count_partial_is_a_floor_the_ui_must_label_n_plus() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 10;
    conn.state()
        .scan_script
        .push_back((5, vec!["app:a".to_string()]));
    conn.seed_string("app:a", 1, "x");

    let outcome = count_budgeted(&mut conn, "app:*", Some(10), Topology::Standalone)
        .await
        .expect("budgeted count");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("SCAN"))
                .count(),
            1,
            "a 10-unit budget buys exactly one 10-count round"
        );
    }
    assert_eq!(outcome.count, 1, "the floor is what was actually seen");
    assert_eq!(outcome.consumed, 10);
    assert_eq!(outcome.dbsize, 10);
    let json = serde_json::to_value(&outcome).expect("CountOutcome serializes");
    assert_eq!(
        json["truncated"], true,
        "truncated=true is the `n+` signal the UI contract freezes"
    );
}
