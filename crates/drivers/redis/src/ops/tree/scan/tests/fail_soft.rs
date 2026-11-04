// [tester] Round-1 coverage of the fail-soft / degradation contract: a rejected
// addressed batch replays per command, a short batch degrades the tail, a
// transport failure aborts the command.

use super::*;

// ---------------------------------------------------------------------------
// [tester] Round-1 coverage of the fail-soft / degradation contract.
//
// The track's cluster claim is not only "one addressed batch per key" but also
// "a rejected *field* degrades alone, a transport failure does not" — the
// replay arm (`fetch_key_group` → `replay_per_command` → `fold_command_answer`)
// was entirely unexercised by the delivered suite, so these tests pin it.
// Declared in this file's existing `mod tests` scope via `super::*`.
// ---------------------------------------------------------------------------

/// How the addressed dispatch layer behaves, mirroring what a real
/// `ClusterConnection` does to a batch.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum BatchOutcome {
    /// One reply per command (the happy path), and the un-scripted default.
    #[default]
    Full,
    /// `try_pipeline_request` folds the batch into one error (BUG-001 shape).
    Rejected,
    /// A short reply vector — the `#[doc(hidden)]` layout drift of BUG-003.
    Short(usize),
}

#[derive(Clone, Default)]
struct RoutingConn {
    state: Arc<Mutex<TreeState>>,
    outcome: Arc<Mutex<BatchOutcome>>,
    /// Command names whose *addressed single* answer arrives as a server error.
    answered_errors: Arc<Mutex<HashSet<String>>>,
    /// Command names whose addressed single fails at transport level.
    transport_errors: Arc<Mutex<HashSet<String>>>,
    replays: Arc<Mutex<usize>>,
    replay_slots: Arc<Mutex<Vec<u16>>>,
}

impl RoutingConn {
    fn new() -> Self {
        Self::default()
    }

    fn set_outcome(&self, outcome: BatchOutcome) {
        *self.outcome.lock().expect("outcome lock") = outcome;
    }

    fn answer_error(&self, name: &str) {
        self.answered_errors
            .lock()
            .expect("lock")
            .insert(name.to_string());
    }

    fn fail_transport(&self, name: &str) {
        self.transport_errors
            .lock()
            .expect("lock")
            .insert(name.to_string());
    }

    fn replay_count(&self) -> usize {
        *self.replays.lock().expect("lock")
    }

    fn replayed_slots(&self) -> Vec<u16> {
        self.replay_slots.lock().expect("lock").clone()
    }

    fn batch_commands(&self, pipe: &Pipeline) -> Vec<Vec<String>> {
        pipe.cmd_iter().map(args_of).collect()
    }

    /// The addressed-single answer, shared by the two dispatch shapes.
    fn single(&self, args: &[String]) -> Result<RValue, redis::RedisError> {
        let name = args.first().map(String::as_str).unwrap_or_default();
        let errors = self.answered_errors.lock().expect("lock");
        if errors.contains(name) {
            return Err(redis::RedisError::from((
                ErrorKind::ResponseError,
                "ERR unknown subcommand or wrong number of args",
            )));
        }
        drop(errors);
        let fails = self.transport_errors.lock().expect("lock");
        if fails.contains(name) {
            return Err(redis::RedisError::from((
                ErrorKind::IoError,
                "connection reset",
            )));
        }
        drop(fails);
        let mut st = self.state.lock().expect("lock");
        Ok(st.reply(args))
    }
}

impl ConnectionLike for RoutingConn {
    fn get_db(&self) -> i64 {
        0
    }

    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        (async move {
            let mut st = self.state.lock().expect("lock");
            st.singles.push(args.clone());
            Ok(st.reply(&args))
        })
        .boxed()
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a Pipeline,
        _offset: usize,
        _count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        let batch = self.batch_commands(pipe);
        (async move {
            let mut st = self.state.lock().expect("lock");
            let values = batch.iter().map(|a| st.reply(a)).collect();
            st.batches.push(batch);
            Ok(values)
        })
        .boxed()
    }
}

impl SlotRoutedConnection for RoutingConn {
    fn command_at_slot<'a>(&'a mut self, cmd: &'a Cmd, slot: u16) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        (async move {
            *self.replays.lock().expect("lock") += 1;
            self.replay_slots.lock().expect("lock").push(slot);
            let mut st = self.state.lock().expect("lock");
            st.addressed.push((slot, vec![args.clone()]));
            drop(st);
            self.single(&args)
        })
        .boxed()
    }

    fn pipeline_at_slot<'a>(
        &'a mut self,
        pipe: &'a Pipeline,
        slot: u16,
    ) -> SlotRoutedBatchFuture<'a> {
        let batch = self.batch_commands(pipe);
        let len = batch.len();
        (async move {
            match *self.outcome.lock().expect("lock") {
                BatchOutcome::Rejected => {
                    Err(redis::RedisError::from((ErrorKind::CrossSlot, "CROSSSLOT")).to_string())
                }
                BatchOutcome::Short(n) => {
                    let mut st = self.state.lock().expect("lock");
                    Ok(batch.iter().take(n.min(len)).map(|a| st.reply(a)).collect())
                }
                BatchOutcome::Full => {
                    let mut st = self.state.lock().expect("lock");
                    let values = batch.iter().map(|a| st.reply(a)).collect();
                    st.addressed.push((slot, batch));
                    Ok(values)
                }
            }
        })
        .boxed()
    }
}

fn seeded_probe_conn(conn: &RoutingConn, key: &str) {
    let mut st = conn.state.lock().expect("lock");
    st.types.insert(key.to_string(), "hash".to_string());
    st.ptls.insert(key.to_string(), 1_500);
    st.mems.insert(key.to_string(), 64);
}

#[tokio::test]
async fn test_tester_a_rejected_addressed_batch_replays_that_key_command_by_command() {
    const KEY: &str = "app:{user1000}:session";
    let conn = RoutingConn::new();
    seeded_probe_conn(&conn, KEY);
    // The dispatch layer folds the whole batch into one error, exactly as a
    // cluster connection folds `OBJECT FREQ`'s rejection (redis-cmds-p0 BUG-001).
    conn.set_outcome(BatchOutcome::Rejected);

    let mut routed = conn.clone();
    let probe = key_probe(&mut routed, KEY, Topology::Cluster)
        .await
        .expect("a folded batch must degrade to singles, not fail the probe");

    assert!(probe.exists);
    assert_eq!(probe.key_type.as_deref(), Some("hash"));
    assert_eq!(probe.ttl_ms, 1_500);
    assert_eq!(probe.memory_bytes, Some(64));
    assert_eq!(
        conn.replay_count(),
        crate::ops::key_probe::KEY_PROBE_PIPELINE_LEN,
        "the replay costs one round trip per command of *this* key only"
    );
    // Every replay stays on the key's own slot: the fallback must not silently
    // un-address the batch (that is what caused the MOVED/slot-rebuild storm).
    let expected = get_slot(KEY.as_bytes());
    assert!(
        conn.replayed_slots().iter().all(|s| *s == expected),
        "replayed commands must keep the key's slot: {:?}",
        conn.replayed_slots()
    );
    // And the batch itself was never taken as an answer.
    assert!(conn.state.lock().expect("lock").batches.is_empty());
}

#[tokio::test]
async fn test_tester_a_short_addressed_batch_replays_instead_of_misreading_slots() {
    const KEY: &str = "app:{user1000}:session";
    let conn = RoutingConn::new();
    seeded_probe_conn(&conn, KEY);
    // Only 2 of 4 replies come back: reading them positionally would answer
    // `exists` from `EXISTS`, `type` from `TYPE`, and then *invent* ttl/memory.
    conn.set_outcome(BatchOutcome::Short(2));

    let mut routed = conn.clone();
    let probe = key_probe(&mut routed, KEY, Topology::Cluster)
        .await
        .expect("a short vector must trigger the replay, not a half-read probe");

    assert_eq!(
        probe.ttl_ms, 1_500,
        "ttl must come from PTTL, not from a shifted slot"
    );
    assert_eq!(probe.memory_bytes, Some(64));
    assert_eq!(
        conn.replay_count(),
        crate::ops::key_probe::KEY_PROBE_PIPELINE_LEN
    );
}

#[tokio::test]
async fn test_tester_an_answered_error_degrades_one_field_and_keeps_the_rest() {
    const KEY: &str = "app:{user1000}:session";
    let conn = RoutingConn::new();
    seeded_probe_conn(&conn, KEY);
    conn.set_outcome(BatchOutcome::Rejected);
    // MEMORY USAGE below Redis 4.0 (or under an ACL): a *server* answer, so only
    // that field degrades.
    conn.answer_error("MEMORY");

    let mut routed = conn.clone();
    let probe = key_probe(&mut routed, KEY, Topology::Cluster)
        .await
        .expect("one rejected field must not fail the probe");

    assert!(probe.exists);
    assert_eq!(probe.key_type.as_deref(), Some("hash"));
    assert_eq!(probe.ttl_ms, 1_500);
    assert_eq!(
        probe.memory_bytes, None,
        "the rejected field alone becomes null"
    );
}

#[tokio::test]
async fn test_tester_a_transport_failure_during_replay_aborts_the_command() {
    const KEY: &str = "app:{user1000}:session";
    let conn = RoutingConn::new();
    seeded_probe_conn(&conn, KEY);
    conn.set_outcome(BatchOutcome::Rejected);
    // An I/O failure means the command never ran: reporting "no value" for it
    // would dress a lost connection up as an empty keyspace.
    conn.fail_transport("TYPE");

    let mut routed = conn.clone();
    let err = key_probe(&mut routed, KEY, Topology::Cluster)
        .await
        .expect_err("a transport failure must surface, not degrade silently");
    assert!(
        err.contains("connection reset"),
        "the transport error must survive verbatim: {err}"
    );
}

#[test]
fn test_tester_the_tree_classifier_splits_a_lost_connection_from_a_rejected_command() {
    // Connection-level: aborts the page (never rendered as "no such key").
    for (kind, detail) in [
        (ErrorKind::IoError, "connection reset"),
        (ErrorKind::ClusterDown, "CLUSTERDOWN"),
        (ErrorKind::CrossSlot, "CROSSSLOT"),
        (
            ErrorKind::ClusterConnectionNotFound,
            "no connection to a valid node",
        ),
    ] {
        let error = redis::RedisError::from((kind, detail));
        assert!(
            is_connection_level_failure(&error),
            "{detail} is a transport/topology failure and must abort the batch"
        );
    }
    // Command-level: degrades one field only.
    for (kind, detail) in [
        (ErrorKind::ResponseError, "ERR unknown command 'PTTL'"),
        (ErrorKind::TypeError, "WRONGTYPE"),
        (ErrorKind::ExtensionError, "NOPERM"),
        (ErrorKind::ReadOnly, "READONLY"),
    ] {
        let error = redis::RedisError::from((kind, detail));
        assert!(
            !is_connection_level_failure(&error),
            "{detail} is an answered error and must degrade one field"
        );
    }
}

#[test]
fn test_tester_fold_command_answer_maps_a_server_error_to_a_missing_value() {
    let rejected = redis::RedisError::from((ErrorKind::ResponseError, "ERR nope"));
    assert!(matches!(
        fold_command_answer(Err(rejected)),
        Ok(RValue::Nil)
    ));
    let lost = redis::RedisError::from((ErrorKind::IoError, "connection reset"));
    assert!(fold_command_answer(Err(lost)).is_err());
    assert!(matches!(
        fold_command_answer(Ok(RValue::Int(3))),
        Ok(RValue::Int(3))
    ));
}
/// A standalone page whose batch answers with fewer values than commands: the
/// `#[doc(hidden)]` reply-layout drift BUG-003 warned about. The tail must
/// degrade, and the *shape* of the page must survive.
#[derive(Clone, Default)]
struct ShortBatchConn {
    state: Arc<Mutex<TreeState>>,
    /// Replies the pipeline hands back, however many commands were sent.
    replies: usize,
}

impl ConnectionLike for ShortBatchConn {
    fn get_db(&self) -> i64 {
        0
    }
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, RValue> {
        let args = args_of(cmd);
        (async move {
            let mut st = self.state.lock().expect("lock");
            st.singles.push(args.clone());
            Ok(st.reply(&args))
        })
        .boxed()
    }
    fn req_packed_commands<'a>(
        &'a mut self,
        pipe: &'a Pipeline,
        _offset: usize,
        _count: usize,
    ) -> RedisFuture<'a, Vec<RValue>> {
        let batch: Vec<Vec<String>> = pipe.cmd_iter().map(args_of).collect();
        let replies = self.replies;
        (async move {
            let mut st = self.state.lock().expect("lock");
            let values = batch.iter().take(replies).map(|a| st.reply(a)).collect();
            st.batches.push(batch);
            Ok(values)
        })
        .boxed()
    }
}

impl SlotRoutedConnection for ShortBatchConn {}

#[tokio::test]
async fn test_tester_a_short_standalone_batch_degrades_the_tail_without_shifting_keys() {
    // 3 keys × 2 meta commands = 6, but only the first key's 2 replies arrive.
    let mut conn = ShortBatchConn {
        state: Arc::new(Mutex::new(TreeState::default())),
        replies: 2,
    };
    let keys = vec!["sh:1".to_string(), "sh:2".to_string(), "sh:3".to_string()];
    let metas = fetch_key_meta(&mut conn, &keys, false, Topology::Standalone)
        .await
        .expect("a short batch must not fail the page");

    assert_eq!(
        metas.len(),
        3,
        "one group per key, whatever the reply count"
    );
    assert_eq!(
        metas[0].key_type, "none",
        "no key was seeded, so even the answered slot reads as 'none'"
    );
    // The point of `scatter`: the trailing keys must not borrow key #1's replies.
    assert_eq!(
        metas[1], metas[2],
        "both unanswered keys degrade identically, not shift by one reply"
    );
}
