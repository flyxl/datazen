// The cluster arm of the sample field read: one addressed batch per key, and
// the per-command replay that keeps a rejected *field* from failing the whole
// batch (the `Value::extract_error_vec` fold a real `ClusterConnection` does).
use super::*;

/// Double for the *real* `ClusterConnection::route_pipeline` shape: one
/// addressed batch per key (one round trip), whose dispatch layer folds any
/// per-command rejection into a single batch error — exactly the
/// `Value::extract_error_vec` behaviour the module docs describe and the only
/// reason [`fetch_memory_sample_fields`] needs a replay fallback.
#[derive(Clone)]
struct ClusterBatchConn {
    inner: ScriptedConn,
}

impl ConnectionLike for ClusterBatchConn {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> RedisFuture<'a, RValue> {
        self.inner.req_packed_command(cmd)
    }
    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        self.inner.req_packed_commands(pipe, offset, count)
    }
    fn get_db(&self) -> i64 {
        self.inner.get_db()
    }
}

impl SlotRoutedConnection for ClusterBatchConn {
    fn pipeline_at_slot<'a>(
        &'a mut self,
        pipe: &'a redis::Pipeline,
        _slot: u16,
    ) -> SlotRoutedBatchFuture<'a> {
        (async move {
            let mut values = Vec::new();
            let mut journaled = Vec::new();
            for cmd in pipe.cmd_iter() {
                let args = args_of(cmd);
                let name = args.first().cloned().unwrap_or_default();
                values.push(self.inner.take_reply(&name));
                journaled.push(normalized(&args));
            }
            // One addressed batch, one round trip — what `route_pipeline` is
            // for, and why cluster costs N trips and not 3N.
            self.inner
                .journal
                .lock()
                .expect("journal lock")
                .batches
                .push(journaled);
            match values
                .iter()
                .position(|v| matches!(v, RValue::ServerError(_)))
            {
                Some(index) => Err(format!(
                    "cluster dispatch: batch folded rejection at command {index}"
                )),
                None => Ok(values),
            }
        })
        .boxed()
    }
}

#[tokio::test]
async fn cluster_memory_sample_field_read_is_one_addressed_batch_per_key() {
    // The round-trip ceiling the fix must not exceed: the original loop paid
    // one `MEMORY USAGE` per key (N). Here two keys cost TWO trips total —
    // one addressed batch each — never three trips per key.
    let keys: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let mut conn = ClusterBatchConn {
        inner: ScriptedConn::new(),
    };
    conn.inner.push_int("MEMORY", 50);
    conn.inner.push_int("MEMORY", 60);
    conn.inner.push_str("TYPE", "string");
    conn.inner.push_str("TYPE", "list");
    conn.inner.push_int("PTTL", -1);
    conn.inner.push_int("PTTL", 700);

    let fields = fetch_memory_sample_fields(&mut conn, &keys, Topology::Cluster)
        .await
        .expect("cluster field read");
    assert_eq!(fields[0].bytes, Some(50));
    assert_eq!(fields[0].key_type.as_deref(), Some("string"));
    assert_eq!(fields[0].ttl_ms, Some(-1), "no-expiry stays a value");
    assert_eq!(fields[1].key_type.as_deref(), Some("list"));
    assert_eq!(fields[1].ttl_ms, Some(700));

    let journal = conn.inner.journal();
    assert_eq!(journal.batches.len(), 2, "one batch per key");
    assert!(journal.singles.is_empty(), "no per-command replay needed");
    assert_eq!(
        journal.round_trips(),
        keys.len(),
        "cluster trips == keys, equal to the original MEMORY USAGE loop"
    );
    for batch in &journal.batches {
        assert_eq!(
            batch.len(),
            MEMORY_SAMPLE_FIELDS_PER_KEY,
            "every batch stays inside the key's own slot"
        );
    }
}

#[tokio::test]
async fn cluster_rejected_field_degrades_via_per_command_replay_not_batch_error() {
    // `route_pipeline` folds the rejected TYPE into one batch error. The fix
    // must NOT turn that into a failed `memory_sample` (acceptance: a single
    // denied field is an empty state, not an error), so only that key replays
    // per command while its bytes/ttl still survive.
    let keys: Vec<String> = ["denied", "fine"].iter().map(|s| s.to_string()).collect();
    let mut conn = ClusterBatchConn {
        inner: ScriptedConn::new(),
    };
    // "denied": batch attempt (MEMORY ok, TYPE rejected, PTTL ok) + replay.
    conn.inner.push_int("MEMORY", 70);
    conn.inner
        .push("TYPE", err_reply("NOPERM no permission to run 'TYPE'"));
    conn.inner.push_int("PTTL", -1);
    conn.inner.push_int("MEMORY", 70);
    conn.inner
        .push("TYPE", err_reply("NOPERM no permission to run 'TYPE'"));
    conn.inner.push_int("PTTL", -1);
    // "fine": one clean batch.
    conn.inner.push_int("MEMORY", 80);
    conn.inner.push_str("TYPE", "hash");
    conn.inner.push_int("PTTL", 4_000);

    let fields = fetch_memory_sample_fields(&mut conn, &keys, Topology::Cluster)
        .await
        .expect("a denied TYPE must not fail the sample");
    assert_eq!(fields.len(), 2);
    let denied = &fields[0];
    assert!(!denied.missing, "NOPERM is a rejection, not a vanished key");
    assert_eq!(denied.key_type, None, "the rejected field alone degrades");
    assert_eq!(
        denied.bytes,
        Some(70),
        "the other fields survive the replay"
    );
    assert_eq!(denied.ttl_ms, Some(-1));
    assert_eq!(fields[1].key_type.as_deref(), Some("hash"));

    let journal = conn.inner.journal();
    assert_eq!(
        journal.batches.len(),
        2,
        "denied batch attempt + clean batch"
    );
    assert_eq!(
        journal.singles.len(),
        MEMORY_SAMPLE_FIELDS_PER_KEY,
        "only the folded key replays, one command at a time"
    );
}

#[tokio::test]
async fn memory_sample_reads_type_and_ttl_in_the_same_single_round_trip() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 3);
    conn.push_scan(0, &["small", "biggest", "medium"]);
    // The single field pipeline, consumed in SCAN order (small, biggest, medium).
    conn.push_int("MEMORY", 10);
    conn.push_int("MEMORY", 999);
    conn.push_int("MEMORY", 500);
    conn.push_str("TYPE", "string");
    conn.push_str("TYPE", "hash");
    conn.push_str("TYPE", "list");
    conn.push_int("PTTL", -1);
    conn.push_int("PTTL", 8000);
    conn.push_int("PTTL", -1);

    let result = crate::ops::observe::memory_sample(&mut conn, 10, Topology::Standalone)
        .await
        .expect("memory_sample");

    // Sorted by bytes desc: biggest(999), medium(500), small(10).
    assert_eq!(result.samples[0].key, "biggest");
    assert_eq!(result.samples[0].bytes, 999);
    assert_eq!(result.samples[0].key_type.as_deref(), Some("hash"));
    assert_eq!(result.samples[0].ttl_ms, Some(8000));
    assert_eq!(result.samples[2].key, "small");
    assert_eq!(result.samples[2].key_type.as_deref(), Some("string"));
    assert_eq!(result.samples[2].ttl_ms, Some(-1));
    assert!(result.samples.iter().all(|s| !s.missing));

    let journal = conn.journal();
    assert_eq!(journal.count_single("DBSIZE"), 1);
    assert_eq!(journal.count_single("SCAN"), 1);
    assert_eq!(
        journal.batches.len(),
        1,
        "every sampled key's fields resolve in one pipeline"
    );
    let single_names = journal.single_names();
    assert!(
        !single_names
            .iter()
            .any(|n| n == "MEMORY" || n == "TYPE" || n == "PTTL"),
        "fields must not go out one command at a time on a single node: {single_names:?}"
    );
    assert!(!journal.flat().iter().any(|c| c[0] == "KEYS"));
}

#[tokio::test]
async fn memory_sample_reports_a_key_deleted_after_sampling_as_missing() {
    let mut conn = ScriptedConn::new();
    conn.push_int("DBSIZE", 2);
    conn.push_scan(0, &["live", "gone"]);
    conn.push_int("MEMORY", 40);
    conn.push("MEMORY", RValue::Nil); // gone: MEMORY USAGE answers nil
    conn.push_str("TYPE", "string");
    conn.push_str("TYPE", "none"); // gone: TYPE answers "none"
    conn.push_int("PTTL", -1);
    conn.push_int("PTTL", -2); // gone: PTTL answers -2

    let result = crate::ops::observe::memory_sample(&mut conn, 10, Topology::Standalone)
        .await
        .expect("a deleted key is a success case, not an error");

    let gone = result
        .samples
        .iter()
        .find(|s| s.key == "gone")
        .expect("the sampled key still appears in the result");
    assert!(
        gone.missing,
        "a vanished key is a distinguishable empty state"
    );
    assert_eq!(gone.bytes, 0);
    assert_eq!(gone.key_type, None);
    assert_eq!(gone.ttl_ms, Some(TTL_MISSING));

    let live = result.samples.iter().find(|s| s.key == "live").unwrap();
    assert!(!live.missing);
    assert_eq!(live.bytes, 40);
}
