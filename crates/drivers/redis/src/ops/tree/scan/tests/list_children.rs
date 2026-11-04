// `list_children`: the budget trio is appended to every row, and folders are
// never typed.

use super::*;

// ---------------------------------------------------------------------------
// list_children: budget trio appended, folders never typed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_children_appends_the_budget_trio_and_types_only_leaves() {
    let mut conn = TreeConn::new();
    conn.state().dbsize = 10;
    conn.seed_string("app:top", 6, "hello world");
    {
        let mut st = conn.state();
        st.types.insert("app:u:1".to_string(), "string".to_string());
        st.types.insert("app:u:2".to_string(), "string".to_string());
        st.scan_script.push_back((
            0,
            vec![
                "app:top".to_string(),
                "app:u:1".to_string(),
                "app:u:2".to_string(),
            ],
        ));
    }

    let page = list_children_page(
        &mut conn,
        "app:",
        0,
        100,
        None,
        false,
        None,
        false,
        None,
        Topology::Standalone,
        Instant::now(),
    )
    .await
    .expect("children page");

    {
        let st = conn.state();
        assert_eq!(
            joined(&st.singles)
                .iter()
                .filter(|line| line.starts_with("DBSIZE"))
                .count(),
            1,
            "list_children reads DBSIZE once, too"
        );
        assert_eq!(
            st.batches.len(),
            2,
            "meta + value batches for the whole level"
        );
        assert_eq!(
            joined(&st.batches[0]),
            ["TYPE app:top", "TTL app:top"],
            "a virtual folder's keys are never typed — only the leaf is"
        );
        assert_eq!(joined(&st.batches[1]), ["STRLEN app:top", "GET app:top"]);
    }

    // Append-only contract: entries + cursor stay, the budget trio joins.
    assert_eq!(page.entries.len(), 2);
    assert!(
        matches!(
            &page.entries[0],
            ChildEntry::Folder { prefix, count } if prefix == "app:u:" && *count == 2
        ),
        "folders sort first and carry their floor count"
    );
    match &page.entries[1] {
        ChildEntry::Key {
            key,
            key_type,
            ttl,
            logical_len,
            ..
        } => {
            assert_eq!(key, "app:top");
            assert_eq!(key_type, "string");
            assert_eq!(*ttl, -1);
            assert_eq!(*logical_len, 6);
        }
        other => panic!("expected a leaf key, got {other:?}"),
    }
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.dbsize, 10);
    assert_eq!(page.consumed, 1_000, "one round at the 1 000-count floor");
    assert!(!page.truncated);
}
