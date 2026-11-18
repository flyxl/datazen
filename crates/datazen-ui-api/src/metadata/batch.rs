//! Metadata batch coalescing, isolated per identity/config/target.
//!
//! Ported from React `packages/frontend/src/lib/metadata/batch.ts` (tasklist
//! T031). The P4 contract is the one [`identity`](super::identity) derives
//! keys for: metadata *batches* (e.g. "load columns for these tables") must
//! never mix requests that belong to different connections, config revisions,
//! or targets — a batch keyed without the target can silently merge two
//! targets' results. [`MetadataBatcher`] groups queued keys by their full
//! [`metadata_batch_bucket`] bucket (identity + config + target *namespace*),
//! so each bucket gets its own in-flight batch, while identical keys inside
//! one bucket deduplicate by their spelling (the caller enqueues cache keys
//! and the batcher only dedups the strings — the key derivations stay
//! single-sourced in `identity.rs`).
//!
//! ## Timer semantics: `setTimeout` → deadline + host-driven flush
//!
//! React arms a real `setTimeout` per bucket and lets the browser event loop
//! fire it. A Rune frame loop has no such side channel — the repo precedent
//! (CopyableError's 1500 ms "copied" transient in
//! `crates/datazen-ui/src/ui/copyable.rs`) is that **the library computes and
//! holds the deadline, the host supplies `now` when judging, and the host
//! schedules the wake** (`Tx::wake_after`, woken against
//! `Frame::wake_deadline`). This port follows that discipline exactly:
//!
//! - each pending bucket holds an absolute `due_at`: the clock reading at
//!   which its trailing debounce expires (`enqueue` time + `debounce_ms`,
//!   re-anchored on every enqueue);
//! - [`enqueue`](MetadataBatcher::enqueue) reads the deadline anchor from the
//!   injected [`MetadataBatcherOptions::now`] clock;
//! - the host judges expiry each frame with
//!   [`flush_due`](MetadataBatcher::flush_due) and schedules its wake from
//!   [`next_due`](MetadataBatcher::next_due), e.g.
//!   `tx.wake_after(Duration::from_millis(next_due - now))`.
//!
//! React declares `now?: () => number` but its implementation never reads it
//! (the timer is a real `setTimeout`); the port finally puts that injection
//! to work as the anchor clock. One deviation from React-as-written, forced
//! by the immediate-mode model and documented here: with `debounce_ms > 0`
//! but **no injected clock**, `enqueue` arms no deadline — without a clock
//! there is no meaningful "later", so flushing is fully manual
//! (`flush_now`), the same posture as React's default `debounceMs: 0`. The
//! clock unit is milliseconds by convention (React's `debounceMs` and the
//! production consumer's `DEFAULT_BATCH_DEBOUNCE_MS = 40` are ms); hosts
//! whose frame clock reads seconds (`Tx::clock()` is an `f64` seconds value)
//! convert before enqueue.
//!
//! ## What the sync port drops
//!
//! React's [`BatchFlush`] may return `Promise<void>` and `flushNow` awaits
//! each bucket sequentially. This module is pure and synchronous (the api
//! crate compiles for wasm32 and the UI event loop is sync); a sync callback
//! preserves React's callback *order* exactly, and the async half is owned
//! by the production consumer's port — `stores/schemaColumnBatcher.ts`,
//! tasklist T405 — which bridges through the adapter's blocking poll.
//!
//! ## Ordering semantics carried over from JavaScript
//!
//! React's `pending` is a `Map`, so `flushNow()` (all) and `invalidate()`
//! walk buckets in **insertion order**, and `flushNow` snapshots the bucket
//! list *before* flushing — buckets enqueued mid-pass are left for the next
//! flush. `Vec<PendingBatch>` reproduces both properties (and keeps the
//! bucket lookup a linear scan, honest at these sizes). React's
//! `Set<string>` keys keep the **first** insertion position on re-add;
//! a membership-checked `Vec` reproduces that, so a flush payload lists keys
//! in first-seen order. React's `flushBucket` also removes the entry before
//! the flush callback runs, so a callback that re-enqueues its own bucket
//! starts a fresh batch — mirrored by removal-before-callback here (in the
//! sync port the callback cannot re-enter the batcher at all, which is a
//! strictly stronger guarantee).
//!
//! batch.ts's tail `export { metadataCacheKey }` has no module-level
//! counterpart here: the Rust re-export chain (`metadata::mod.rs` →
//! `lib.rs`, i.e. `crate::metadata::metadata_cache_key` /
//! `crate::metadata_cache_key`) already covers that contract, and a test
//! below pins both paths.

use super::identity::{MetadataIdentityInput, metadata_batch_bucket};

/// React `BatchFlush = (bucket: string, keys: string[]) => void | Promise<void>`.
///
/// The sync half: the callback owns the batch's side effect (the metadata
/// read). `keys` is the batch's deduplicated key list in first-seen order,
/// handed over owned, like React's fresh `[...batch.keys]` array. The
/// `Promise` half is deliberately out of scope — see the module docs.
pub type BatchFlush = Box<dyn FnMut(&str, Vec<String>)>;

/// React's `now?: () => number` — the injectable clock that anchors debounce
/// deadlines. Reads milliseconds (the unit `debounce_ms` is expressed in).
/// Hosts wire it to their frame clock — typically a shared `Rc<Cell<u64>>`
/// the frame loop refreshes from `tx.clock()` (a closure cannot capture the
/// per-frame `Tx` itself); tests wire it to a counter cell for deterministic
/// drives.
pub type MetadataClock = Box<dyn Fn() -> u64>;

/// React `MetadataBatcherOptions` (`debounceMs?`, `now?`).
#[derive(Default)]
pub struct MetadataBatcherOptions {
    /// Trailing coalescing window, in milliseconds of the clock unit. `0`
    /// (React's default) arms no deadline: nothing auto-flushes, and only
    /// [`flush_now`](MetadataBatcher::flush_now) / `invalidate` move buckets.
    pub debounce_ms: u64,
    /// Deadline anchor clock. Required to arm timers — see the module docs
    /// for the no-clock rule.
    pub now: Option<MetadataClock>,
}

/// One queued batch: its bucket key, its deduplicated key set, and the port
/// of React's `timer: setTimeout | null` — the absolute clock reading at
/// which the trailing debounce expires (`None` = no timer armed).
struct PendingBatch {
    /// The bucket this batch is isolated to (identity + config + target
    /// namespace); also the key the entry is filed under.
    bucket: String,
    /// Deduplicated enqueued cache keys, first-seen order (React's `Set`).
    keys: Vec<String>,
    /// `timer` port: deadline in clock milliseconds, `None` when unarmed.
    due_at: Option<u64>,
}

/// Collects per-bucket metadata requests and flushes them as deduped batches.
///
/// Construct with [`new`](Self::new) (React's `options = {}` default) or
/// [`with_options`](Self::with_options). Every method mirrors its React
/// counterpart one-for-one; the timer-drive additions are
/// [`flush_due`](Self::flush_due) and [`next_due`](Self::next_due).
pub struct MetadataBatcher {
    flush: BatchFlush,
    debounce_ms: u64,
    now: Option<MetadataClock>,
    /// React's `pending: Map<string, PendingBatch>` — a `Map` walks in
    /// insertion order, so the entry list does too.
    pending: Vec<PendingBatch>,
}

impl MetadataBatcher {
    /// `new MetadataBatcher(flush)` — React's default options
    /// (`debounceMs: 0`, no clock, fully manual flushing).
    pub fn new(flush: BatchFlush) -> Self {
        Self::with_options(flush, MetadataBatcherOptions::default())
    }

    /// `new MetadataBatcher(flush, options)`.
    pub fn with_options(flush: BatchFlush, options: MetadataBatcherOptions) -> Self {
        Self {
            flush,
            debounce_ms: options.debounce_ms,
            now: options.now,
            pending: Vec::new(),
        }
    }

    /// Queue one metadata key for its identity/config/target bucket.
    ///
    /// React's `enqueue(input, key)`: buckets through
    /// [`metadata_batch_bucket`], dedups `key` inside the bucket (first-seen
    /// position kept), and re-arms the trailing debounce — every enqueue
    /// moves the bucket's deadline to `clock() + debounce_ms`, so a burst
    /// keeps deferring until the bucket has been quiet for the window.
    pub fn enqueue(&mut self, input: &MetadataIdentityInput, key: impl Into<String>) {
        let bucket = metadata_batch_bucket(input);
        let key = key.into();
        // React clears the old timer, then arms a new one only when
        // `debounceMs > 0`. The no-clock rule (module docs) folds into the
        // same condition: no clock, no anchor, no deadline.
        let due_at = if self.debounce_ms > 0 {
            self.now
                .as_ref()
                .map(|clock| clock().saturating_add(self.debounce_ms))
        } else {
            None
        };
        if let Some(batch) = self.pending.iter_mut().find(|batch| batch.bucket == bucket) {
            if !batch.keys.contains(&key) {
                batch.keys.push(key);
            }
            batch.due_at = due_at;
        } else {
            self.pending.push(PendingBatch {
                bucket,
                keys: vec![key],
                due_at,
            });
        }
    }

    /// Flush one bucket immediately, or all when `bucket` is `None`.
    ///
    /// React's `flushNow(bucket?)`: with a bucket, an unknown name is a
    /// no-op; without one, every pending bucket flushes in insertion order.
    /// Returns the number of buckets flushed (additive observability; React
    /// resolves `Promise<void>`).
    pub fn flush_now(&mut self, bucket: Option<&str>) -> usize {
        match bucket {
            Some(bucket) => self.flush_bucket(bucket),
            // React snapshots `[...this.pending.keys()]` before flushing, so
            // buckets enqueued mid-pass wait for the next flush. The cloned
            // list is that snapshot.
            None => {
                let buckets: Vec<String> =
                    self.pending.iter().map(|batch| batch.bucket.clone()).collect();
                buckets.iter().map(|bucket| self.flush_bucket(bucket)).sum()
            }
        }
    }

    /// Drop pending requests for every bucket matching the invalidation rule.
    ///
    /// React's `invalidate(matches)`: matching entries are removed together
    /// with their armed deadline — an invalidated bucket never fires later,
    /// which is React's `clearTimeout` on the same line as the delete.
    pub fn invalidate(&mut self, mut matches: impl FnMut(&str) -> bool) {
        // React walks a snapshot of entries and deletes matches; `retain`
        // with the negated predicate is that walk (the predicate cannot
        // re-enter the batcher from inside a sync call).
        self.pending.retain(|batch| !matches(&batch.bucket));
    }

    /// Number of pending **buckets** (observability / tests) — React's `size`
    /// is the `Map`'s entry count, not the key count.
    pub fn size(&self) -> usize {
        self.pending.len()
    }

    /// Earliest armed deadline across all buckets, in clock milliseconds.
    ///
    /// Additive (React had no analogue — its event loop owned the timer):
    /// the host schedules its wake from this, e.g.
    /// `tx.wake_after(Duration::from_millis(next_due - now))`, the same
    /// discipline CopyableError's copied transient uses. `None` when no
    /// bucket has an armed deadline.
    pub fn next_due(&self) -> Option<u64> {
        self.pending.iter().filter_map(|batch| batch.due_at).min()
    }

    /// Flush every bucket whose deadline has passed `now`.
    ///
    /// The timer-drive half of the `setTimeout` port (module docs): the host
    /// calls this each frame with its clock reading; buckets with
    /// `due_at <= now` flush in insertion order. Returns how many flushed.
    /// Unarmed buckets (`debounce_ms == 0`, or no clock) are never due —
    /// only [`flush_now`](Self::flush_now) moves them.
    pub fn flush_due(&mut self, now: u64) -> usize {
        let due: Vec<String> = self
            .pending
            .iter()
            .filter(|batch| batch.due_at.is_some_and(|due| due <= now))
            .map(|batch| batch.bucket.clone())
            .collect();
        due.iter().map(|bucket| self.flush_bucket(bucket)).sum()
    }

    /// React's private `flushBucket(bucket)`: a missing bucket is a no-op;
    /// the entry (and its deadline) is removed *before* the flush callback
    /// runs, so a callback that re-enqueues this bucket starts a fresh batch
    /// instead of mutating the one being flushed.
    fn flush_bucket(&mut self, bucket: &str) -> usize {
        let Some(index) = self.pending.iter().position(|batch| batch.bucket == bucket) else {
            return 0;
        };
        let batch = self.pending.remove(index);
        (self.flush)(&batch.bucket, batch.keys);
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{ExecutionTarget, MetadataTarget, NamespaceTarget};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// Recorded flush callbacks: `(bucket, keys)` pairs in flush order.
    type Flushes = Rc<RefCell<Vec<(String, Vec<String>)>>>;

    /// A batcher whose flush callback appends to a shared record, plus the
    /// record handle. React's `new MetadataBatcher(flush)` default options.
    fn recorder() -> (MetadataBatcher, Flushes) {
        let flushes: Flushes = Rc::default();
        let sink = flushes.clone();
        let batcher = MetadataBatcher::new(Box::new(move |bucket, keys| {
            sink.borrow_mut().push((bucket.to_string(), keys));
        }));
        (batcher, flushes)
    }

    /// A debounced batcher with a shared clock cell wired as the `now`
    /// injection, plus the record and the cell handle: tests set the cell,
    /// then enqueue.
    fn debounced(debounce_ms: u64) -> (MetadataBatcher, Flushes, Rc<Cell<u64>>) {
        let flushes: Flushes = Rc::default();
        let sink = flushes.clone();
        let cell = Rc::new(Cell::new(0u64));
        let read = cell.clone();
        let batcher = MetadataBatcher::with_options(
            Box::new(move |bucket, keys| {
                sink.borrow_mut().push((bucket.to_string(), keys));
            }),
            MetadataBatcherOptions {
                debounce_ms,
                now: Some(Box::new(move || read.get())),
            },
        );
        (batcher, flushes, cell)
    }

    /// The React suite's `target()` shape: an execution target on namespace
    /// `db`/`public` with no object, at the given revision and session.
    fn input(revision: u64, db_session: &str) -> MetadataIdentityInput {
        MetadataIdentityInput {
            connection_id: "c1".to_string(),
            config_revision: revision,
            db_session_id: db_session.to_string(),
            target: MetadataTarget::Execution(ExecutionTarget {
                connection_id: "c1".to_string(),
                namespace: NamespaceTarget::new(
                    Some("db".to_string()),
                    None,
                    Some("public".to_string()),
                    Vec::new(),
                ),
                object: None,
            }),
        }
    }

    #[test]
    fn groups_keys_by_bucket_and_dedupes_like_the_react_suite() {
        // Port of the React test 'groups keys by identity/config/target
        // bucket and dedupes': the same identity twice (dedup), a second
        // target namespace, and a second db session must produce three
        // batches, never a merged one.
        let (mut batcher, flushes) = recorder();
        let a = input(1, "s1");
        let mut other_namespace = a.clone();
        other_namespace.target = MetadataTarget::Execution(ExecutionTarget {
            connection_id: "c1".to_string(),
            namespace: NamespaceTarget::new(
                Some("db2".to_string()),
                None,
                Some("public".to_string()),
                Vec::new(),
            ),
            object: None,
        });
        let other_session = MetadataIdentityInput {
            db_session_id: "s2".to_string(),
            ..a.clone()
        };
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&a, "k2");
        batcher.enqueue(&other_namespace, "k1");
        batcher.enqueue(&other_session, "k3");
        assert_eq!(batcher.size(), 3);
        assert_eq!(batcher.flush_now(None), 3);
        let flushed = flushes.borrow();
        assert_eq!(flushed.len(), 3);
        // The duplicate k1 collapsed into the first bucket only.
        let two_key: Vec<&(String, Vec<String>)> =
            flushed.iter().filter(|(_, keys)| keys.len() == 2).collect();
        assert_eq!(two_key.len(), 1);
        assert_eq!(two_key[0].1, vec!["k1".to_string(), "k2".to_string()]);
        assert!(flushed.iter().any(|(_, keys)| keys == &["k1".to_string()]));
        assert!(flushed.iter().any(|(_, keys)| keys == &["k3".to_string()]));
        // Bucket isolation, observed through the callback: the three buckets
        // are pairwise distinct and each equals its input's batch bucket.
        let buckets: Vec<String> = flushed.iter().map(|(bucket, _)| bucket.clone()).collect();
        assert_ne!(buckets[0], buckets[1]);
        assert_ne!(buckets[0], buckets[2]);
        assert_ne!(buckets[1], buckets[2]);
        assert!(buckets.contains(&metadata_batch_bucket(&a)));
        assert!(buckets.contains(&metadata_batch_bucket(&other_namespace)));
        assert!(buckets.contains(&metadata_batch_bucket(&other_session)));
    }

    #[test]
    fn duplicate_keys_collapse_keeping_first_seen_order() {
        // React's `Set` keeps the first insertion position on re-add: the
        // key sequence k1, k2, k1, k3, k2 flushes as [k1, k2, k3].
        let (mut batcher, flushes) = recorder();
        let a = input(1, "s1");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&a, "k2");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&a, "k3");
        batcher.enqueue(&a, "k2");
        batcher.flush_now(None);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(
                metadata_batch_bucket(&a),
                vec!["k1".to_string(), "k2".to_string(), "k3".to_string()]
            )]
        );
    }

    #[test]
    fn a_debounced_bucket_flushes_once_its_deadline_passes() {
        // Timer expiry, driven deterministically: enqueue at t=100 with a
        // 40ms window arms due=140; before it nothing flushes, at exactly it
        // the batch flushes once, and afterwards nothing more.
        let (mut batcher, flushes, cell) = debounced(40);
        cell.set(100);
        let a = input(1, "s1");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&a, "k2");
        assert_eq!(batcher.next_due(), Some(140));
        assert_eq!(batcher.flush_due(139), 0);
        assert_eq!(batcher.size(), 1);
        assert!(flushes.borrow().is_empty());
        assert_eq!(batcher.flush_due(140), 1);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(metadata_batch_bucket(&a), vec!["k1".to_string(), "k2".to_string()])]
        );
        assert_eq!(batcher.flush_due(1_000), 0);
        assert_eq!(batcher.size(), 0);
    }

    #[test]
    fn enqueue_resets_the_trailing_deadline() {
        // React clears and re-arms the timer on every enqueue: a second
        // enqueue at t=120 moves due 140 → 160, so t=140 does NOT fire.
        let (mut batcher, flushes, cell) = debounced(40);
        let a = input(1, "s1");
        cell.set(100);
        batcher.enqueue(&a, "k1");
        cell.set(120);
        batcher.enqueue(&a, "k2");
        assert_eq!(batcher.next_due(), Some(160));
        assert_eq!(batcher.flush_due(140), 0);
        assert!(flushes.borrow().is_empty());
        assert_eq!(batcher.flush_due(160), 1);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(metadata_batch_bucket(&a), vec!["k1".to_string(), "k2".to_string()])]
        );
    }

    #[test]
    fn flush_due_only_moves_buckets_whose_deadline_has_passed() {
        // Among armed buckets only the due ones flush, in creation order;
        // the later bucket stays pending with its deadline intact.
        let (mut batcher, flushes, cell) = debounced(40);
        let first = input(1, "s1");
        let second = input(2, "s1");
        let third = input(3, "s1");
        cell.set(100);
        batcher.enqueue(&first, "k1");
        cell.set(110);
        batcher.enqueue(&second, "k2");
        cell.set(500);
        batcher.enqueue(&third, "k3");
        assert_eq!(batcher.flush_due(200), 2);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[
                (metadata_batch_bucket(&first), vec!["k1".to_string()]),
                (metadata_batch_bucket(&second), vec!["k2".to_string()]),
            ]
        );
        assert_eq!(batcher.size(), 1);
        assert_eq!(batcher.next_due(), Some(540));
        assert_eq!(batcher.flush_due(540), 1);
    }

    #[test]
    fn flush_now_with_one_bucket_leaves_the_rest_pending() {
        // flushNow(bucket): only that bucket flushes; a repeat is a no-op,
        // and the other bucket still flushes afterwards.
        let (mut batcher, flushes) = recorder();
        let a = input(1, "s1");
        let b = input(2, "s1");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&b, "k2");
        let a_bucket = metadata_batch_bucket(&a);
        assert_eq!(batcher.flush_now(Some(&a_bucket)), 1);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(a_bucket.clone(), vec!["k1".to_string()])]
        );
        assert_eq!(batcher.size(), 1);
        assert_eq!(batcher.flush_now(Some(&a_bucket)), 0);
        assert_eq!(flushes.borrow().len(), 1);
        assert_eq!(batcher.flush_now(None), 1);
        assert_eq!(
            flushes.borrow()[1],
            (metadata_batch_bucket(&b), vec!["k2".to_string()])
        );
    }

    #[test]
    fn flush_now_without_bucket_flushes_in_creation_order() {
        // React's Map iterates insertion order, so the all-flush walks the
        // buckets in the order they were first created — even when an older
        // bucket was re-enqueued in between (Map.set keeps its position).
        let (mut batcher, flushes) = recorder();
        let a = input(1, "s1");
        let b = input(2, "s1");
        let c = input(3, "s1");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&b, "k2");
        batcher.enqueue(&a, "k1b");
        batcher.enqueue(&c, "k3");
        assert_eq!(batcher.size(), 3);
        assert_eq!(batcher.flush_now(None), 3);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[
                (metadata_batch_bucket(&a), vec!["k1".to_string(), "k1b".to_string()]),
                (metadata_batch_bucket(&b), vec!["k2".to_string()]),
                (metadata_batch_bucket(&c), vec!["k3".to_string()]),
            ]
        );
    }

    #[test]
    fn invalidate_drops_only_matching_buckets_and_their_timers() {
        // Port of the React test 'invalidate drops only matching buckets':
        // the revision-2 bucket (its JSON string contains ",2,") is dropped
        // with its armed timer, so it never fires — not even at now=10^9 —
        // and the surviving revision-1 batch flushes alone.
        let (mut batcher, flushes, cell) = debounced(40);
        cell.set(100);
        let rev1 = input(1, "s1");
        let rev2 = input(2, "s1");
        batcher.enqueue(&rev1, "k1");
        batcher.enqueue(&rev2, "k2");
        assert_eq!(batcher.size(), 2);
        batcher.invalidate(|bucket| bucket.contains(",2,"));
        assert_eq!(batcher.size(), 1);
        // The dropped bucket's timer went with the entry.
        assert_eq!(batcher.next_due(), Some(140));
        assert_eq!(batcher.flush_due(1_000_000_000), 1);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(metadata_batch_bucket(&rev1), vec!["k1".to_string()])]
        );
        assert_eq!(batcher.size(), 0);
    }

    #[test]
    fn an_empty_flush_now_is_a_no_op() {
        // flushBucket early-returns for a missing bucket: an empty batcher,
        // an unknown bucket, and an already-flushed bucket all stay silent.
        let (mut batcher, flushes) = recorder();
        assert_eq!(batcher.flush_now(None), 0);
        assert_eq!(batcher.flush_now(Some("no-such-bucket")), 0);
        assert_eq!(batcher.size(), 0);
        let a = input(1, "s1");
        batcher.enqueue(&a, "k1");
        assert_eq!(batcher.flush_now(None), 1);
        assert_eq!(batcher.flush_now(None), 0);
        assert_eq!(batcher.flush_now(Some(&metadata_batch_bucket(&a))), 0);
        assert_eq!(flushes.borrow().len(), 1);
    }

    #[test]
    fn size_counts_buckets_not_keys() {
        // React's `size` is the pending Map's entry count: three keys in one
        // bucket still size 1, and it drops back to 0 after a flush.
        let (mut batcher, _flushes) = recorder();
        let a = input(1, "s1");
        batcher.enqueue(&a, "k1");
        batcher.enqueue(&a, "k2");
        batcher.enqueue(&a, "k3");
        assert_eq!(batcher.size(), 1);
        batcher.flush_now(None);
        assert_eq!(batcher.size(), 0);
    }

    #[test]
    fn zero_debounce_never_arms_a_timer() {
        // React's `if (debounce > 0) setTimeout(...)`: the default options
        // arm nothing, so no clock reading ever auto-flushes — only flush_now
        // moves buckets, exactly how the React suite drives its tests.
        let (mut batcher, flushes) = recorder();
        let a = input(1, "s1");
        batcher.enqueue(&a, "k1");
        assert_eq!(batcher.next_due(), None);
        assert_eq!(batcher.flush_due(1_000_000), 0);
        assert_eq!(batcher.size(), 1);
        assert!(flushes.borrow().is_empty());
        assert_eq!(batcher.flush_now(None), 1);
    }

    #[test]
    fn without_an_injected_clock_a_positive_debounce_stays_manual() {
        // The documented deviation (module docs): with no clock to anchor
        // against, `debounce_ms > 0` arms nothing and flushing is fully
        // manual — pin it so the rule cannot silently change.
        let (mut batcher, flushes) = recorder();
        batcher.debounce_ms = 40;
        let a = input(1, "s1");
        batcher.enqueue(&a, "k1");
        assert_eq!(batcher.next_due(), None);
        assert_eq!(batcher.flush_due(u64::MAX / 2), 0);
        assert_eq!(batcher.size(), 1);
        assert!(flushes.borrow().is_empty());
        assert_eq!(batcher.flush_now(None), 1);
    }

    #[test]
    fn next_due_reports_the_earliest_armed_deadline() {
        // The host's wake-scheduling surface: the minimum over armed buckets,
        // falling back as buckets are invalidated or flushed.
        let (mut batcher, _flushes, cell) = debounced(40);
        assert_eq!(batcher.next_due(), None);
        let a = input(1, "s1");
        let b = input(2, "s1");
        cell.set(100);
        batcher.enqueue(&a, "k1");
        cell.set(200);
        batcher.enqueue(&b, "k2");
        assert_eq!(batcher.next_due(), Some(140));
        let a_bucket = metadata_batch_bucket(&a);
        batcher.invalidate(|bucket| bucket == a_bucket);
        assert_eq!(batcher.next_due(), Some(240));
        batcher.flush_now(None);
        assert_eq!(batcher.next_due(), None);
    }

    #[test]
    fn a_flushed_bucket_re_enqueues_as_a_fresh_batch_with_a_fresh_timer() {
        // React removes the entry before calling flush, so re-enqueueing the
        // same bucket starts a fresh batch with a fresh timer instead of
        // mutating the flushed one. The sync port's callback cannot re-enter
        // the batcher (unique `&mut self`), so the same observable sequence
        // is driven from outside: flush, re-enqueue, prove the new deadline.
        let (mut batcher, flushes, cell) = debounced(40);
        let a = input(1, "s1");
        cell.set(100);
        batcher.enqueue(&a, "k1");
        assert_eq!(batcher.flush_due(140), 1);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(metadata_batch_bucket(&a), vec!["k1".to_string()])]
        );
        assert_eq!(batcher.size(), 0);
        cell.set(300);
        batcher.enqueue(&a, "k2");
        assert_eq!(batcher.size(), 1);
        assert_eq!(batcher.next_due(), Some(340));
        assert_eq!(batcher.flush_due(339), 0);
        assert_eq!(batcher.flush_due(340), 1);
        assert_eq!(
            flushes.borrow()[1],
            (metadata_batch_bucket(&a), vec!["k2".to_string()])
        );
    }

    #[test]
    fn the_crate_root_reexports_metadata_cache_key_like_batch_ts_did() {
        // batch.ts's tail `export { metadataCacheKey }`: the Rust equivalent
        // is the mod.rs/lib.rs re-export chain — pin both import paths and
        // the batch-side surface consumers rely on.
        let a = input(1, "s1");
        let via_root = crate::metadata_cache_key(&a);
        let via_metadata_mod = crate::metadata::metadata_cache_key(&a);
        assert_eq!(via_root, via_metadata_mod);
        assert_eq!(via_root, r#"["c1",1,"s1","db",null,"public",[],null]"#);
        assert_eq!(
            crate::metadata::metadata_batch_bucket(&a),
            metadata_batch_bucket(&a)
        );
    }

    #[test]
    fn the_flush_callback_receives_bucket_then_keys_in_first_seen_order() {
        // The callback contract: bucket string first, then the batch's
        // deduped key list — the shapes React's BatchFlush receives, in the
        // order the keys were first seen.
        let (mut batcher, flushes) = recorder();
        let a = input(1, "s1");
        batcher.enqueue(&a, "k2");
        batcher.enqueue(&a, "k1");
        batcher.flush_now(None);
        assert_eq!(
            flushes.borrow().as_slice(),
            &[(metadata_batch_bucket(&a), vec!["k2".to_string(), "k1".to_string()])]
        );
    }
}
