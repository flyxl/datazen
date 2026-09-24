//! Key-tree **scan budget** model — pure, no I/O (PRD §3.2「扫描预算模型」, §4 I-2/I-3).
//!
//! One user action (one page of the key tree, one `count_matching`) gets **one
//! cumulative COUNT budget**: every `SCAN` round issued inside that action
//! charges its `COUNT` to [`ScanBudget`], the loop may never spend more than the
//! budget, and whatever it could not scan is reported as `truncated` instead of
//! being silently dropped. This is what stops a sparse `MATCH` from turning into
//! an unbounded keyspace walk.
//!
//! # This budget is *not* the value-search budget
//!
//! [`crate::ops::value_search`] has its own pair of constants
//! (`DEFAULT_MAX_KEYS` = 50 000, `HARD_MAX_KEYS` = **200 000**) that cap how many
//! *keys* a value search may look at. This module caps how much **cumulative
//! `SCAN` COUNT** one key-tree action may spend, and its hard ceiling is
//! **1 000 000** (PRD I-2's `10k/50k/200k/1M` ladder). Different name, different
//! unit, different value on purpose — do **not** "align" the two, and do not
//! reuse those constants here.
//!
//! `consumed` is therefore measured in **COUNT units, not in keys returned**:
//! Redis treats `COUNT` as a hint, so a round may return more or fewer keys than
//! it was charged for. That is exactly the quantity the server cost scales with,
//! which is why the budget is expressed in it.

/// Budget for one key-tree action when the caller does not ask for one and
/// `DBSIZE` gives nothing to scale by. Matches PRD I-2's default tier.
pub const DEFAULT_TREE_BUDGET: u64 = 50_000;

/// Hard ceiling for one key-tree action (PRD I-2's top tier). Larger requests
/// are clamped, never rejected — a stale UI preference must not be able to
/// trigger an unbounded scan.
pub const HARD_MAX_TREE_BUDGET: u64 = 1_000_000;

/// Scaling factor applied to `DBSIZE` when deriving a budget from the keyspace.
///
/// `SCAN` visits the keyspace in one pass, so a *complete* pass costs about
/// `dbsize` COUNT units. Two multipliers of headroom are folded in:
/// `COUNT` is a hint the server may under-deliver (more rounds for the same
/// keys), and a `MATCH` filter needs a full pass to fill a page. So
/// `dbsize × 2` is "one honest full pass, with slack", still far below the
/// 1M hard cap for typical databases.
pub const TREE_BUDGET_DBSIZE_FACTOR: u64 = 2;

/// `SCAN COUNT` used for a round when the caller's page target is smaller.
///
/// Rounds are the expensive part (one round trip each), so a round asks for at
/// least this many keys even when the page only wants 50 — the extra keys are
/// what fills folders' counts and makes the next page cheaper. The per-round
/// COUNT is always additionally clamped to the budget that is left, so this
/// number can never be spent past the limit.
pub const TREE_SCAN_MIN_ROUND_COUNT: u32 = 1_000;

/// Floor for a `SCAN COUNT`, and the value used when a budget leaves less than
/// a round's worth of credit: the last round of a nearly-spent budget still
/// makes progress instead of being skipped.
pub const MIN_TREE_SCAN_COUNT: u32 = 10;

/// Absolute cap on `SCAN` rounds per action.
///
/// Inherited from [`crate::ops::workbench::MAX_SCAN_ROUNDS`], for the same
/// reason: a proxy or a downgraded replica that keeps handing out a non-zero
/// cursor would otherwise pin the connection (the tree re-runs on every folder
/// expansion, and the driver holds the connection's write lock while it scans).
pub const MAX_TREE_SCAN_ROUNDS: u32 = 64;

/// Stop after this many consecutive rounds that yielded no new key — the
/// cursor is not making progress, so spending more budget would buy nothing.
pub const MAX_TREE_STALLED_ROUNDS: u32 = 16;

/// Glob metacharacters Redis' `MATCH` understands. A pattern without any of
/// them names exactly one key, which is what lets the tree short-circuit the
/// scan entirely (PRD §4 I-3). `[` included: it opens a character class.
const GLOB_CHARS: [char; 4] = ['*', '?', '[', '\\'];

/// Is this pattern an exact key name rather than a glob?
///
/// `true` only for a non-empty pattern with no glob character, and explicitly
/// **not** for `""` / `"*"`, which mean "everything" to the tree.
pub fn is_exact_key_pattern(pattern: &str) -> bool {
    !pattern.is_empty() && pattern != "*" && !pattern.chars().any(|c| GLOB_CHARS.contains(&c))
}

/// Budget for one action, scaled by `DBSIZE` when it is known.
///
/// `min(HARD_MAX_TREE_BUDGET, max(DEFAULT_TREE_BUDGET, dbsize ×
/// [TREE_BUDGET_DBSIZE_FACTOR]))`, and an explicitly requested budget is clamped
/// into `1..=HARD_MAX_TREE_BUDGET` **without** scaling — a caller that picked a
/// tier from the `10k/50k/200k/1M` ladder means it. `dbsize == 0` (empty db, or
/// `DBSIZE` unavailable and reported as zero) falls back to the default tier, so
/// a missing answer can never produce a zero budget.
///
/// **`Some(0)` means "no request"**, exactly like `None` (审查发现 R-1, closed in
/// favour of `## 契约冻结`: "`budget` 缺失或 `0` ⇒ 派生档"). A zero never reaches the
/// clamp, so this function cannot hand out a 1-round budget someone did not ask
/// for; the dispatch arm folds `0` into `None` for the same reason, and the two
/// layers now agree instead of holding opposite readings of the same input.
pub fn tree_scan_budget(requested: Option<u64>, dbsize: u64) -> u64 {
    match requested.filter(|raw| *raw > 0) {
        // A requested tier is clamped into 1..=HARD_MAX without scaling.
        Some(raw) => raw.clamp(1, HARD_MAX_TREE_BUDGET),
        // Absent or zero: derived — DEFAULT floor, DBSIZE × factor, HARD ceiling.
        // The constants satisfy DEFAULT < HARD, so this clamp cannot panic.
        None => dbsize
            .saturating_mul(TREE_BUDGET_DBSIZE_FACTOR)
            .clamp(DEFAULT_TREE_BUDGET, HARD_MAX_TREE_BUDGET),
    }
}

/// Cumulative COUNT ledger for one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanBudget {
    limit: u64,
    consumed: u64,
}

impl ScanBudget {
    /// Start a fresh ledger against `limit`.
    pub fn new(limit: u64) -> Self {
        Self {
            limit: limit.max(1),
            consumed: 0,
        }
    }

    /// The cap this action may not spend past.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// COUNT units charged so far — the `consumed` the reply reports.
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    /// COUNT units still available.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.consumed)
    }

    /// Is the ledger spent?
    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0
    }

    /// `COUNT` to send on the next round, or `None` when there is nothing left
    /// to spend.
    ///
    /// `page_target` is what the caller wants per page; a round asks for at
    /// least [`TREE_SCAN_MIN_ROUND_COUNT`] so small pages do not multiply the
    /// round trips, and never more than [`ScanBudget::remaining`], which is the
    /// invariant the whole model rests on: **`Σ COUNT ≤ budget`**. When the
    /// remainder is tiny the last round still runs at
    /// [`MIN_TREE_SCAN_COUNT`] (a `COUNT` the server may overshoot is the point
    /// of a hint) rather than being skipped, so `consumed` can end marginally
    /// above `limit` on the final round — see [`ScanBudget::truncated`] for how
    /// that is reported.
    pub fn next_count(&self, page_target: u32) -> Option<u32> {
        if self.is_exhausted() {
            return None;
        }
        let want = page_target.max(TREE_SCAN_MIN_ROUND_COUNT) as u64;
        Some(want.min(self.remaining()).max(MIN_TREE_SCAN_COUNT as u64) as u32)
    }

    /// Charge one round's `COUNT`.
    pub fn charge(&mut self, count: u32) {
        self.consumed = self.consumed.saturating_add(u64::from(count));
    }

    /// Did the ledger run out? A page that filled *before* the cap is not
    /// truncation — the caller resumes from the returned cursor.
    pub fn truncated(&self) -> bool {
        self.is_exhausted()
    }
}

/// Round-count / progress guards for one action's scan loop.
///
/// Kept separate from [`ScanBudget`] because these two exits are about the
/// *cursor misbehaving*, not about money: a stuck cursor must stop the loop even
/// while budget remains, and must be reportable as `truncated` for the same
/// reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanLoopGuard {
    rounds: u32,
    stalled_rounds: u32,
}

impl ScanLoopGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rounds spent so far.
    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    /// Record one completed round; `advanced` is whether it produced new keys.
    pub fn record_round(&mut self, advanced: bool) {
        self.rounds = self.rounds.saturating_add(1);
        self.stalled_rounds = if advanced {
            0
        } else {
            self.stalled_rounds.saturating_add(1)
        };
    }

    /// Must the loop stop even though the cursor has not wrapped?
    pub fn cap_hit(&self) -> bool {
        self.rounds >= MAX_TREE_SCAN_ROUNDS || self.stalled_rounds >= MAX_TREE_STALLED_ROUNDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- exact-key short circuit (I-3) ---------------------------------

    #[test]
    fn exact_pattern_is_a_key_name_not_a_glob() {
        assert!(is_exact_key_pattern("app:users:42"));
        assert!(is_exact_key_pattern("{tag}:member"));
        assert!(is_exact_key_pattern("has spaces and-dash"));
    }

    #[test]
    fn any_glob_character_disqualifies_the_pattern() {
        for pattern in ["app:*", "user:?", "[abc]:key", "cache\\:1", "*", "a*b", "?"] {
            assert!(
                !is_exact_key_pattern(pattern),
                "'{pattern}' carries a glob character and must not short-circuit"
            );
        }
    }

    #[test]
    fn empty_and_star_mean_everything_not_one_key() {
        assert!(!is_exact_key_pattern(""));
        assert!(!is_exact_key_pattern("*"));
    }

    // --- budget derivation ---------------------------------------------

    #[test]
    fn budget_scales_with_dbsize_between_default_and_hard_cap() {
        // Small db: the default floor wins, so a 12-key db is not scanned with a
        // 24-unit budget that could never fill a page.
        assert_eq!(tree_scan_budget(None, 12), DEFAULT_TREE_BUDGET);
        assert_eq!(tree_scan_budget(None, 0), DEFAULT_TREE_BUDGET);
        assert_eq!(tree_scan_budget(None, 1_000), DEFAULT_TREE_BUDGET);
        // Mid-size db: dbsize × 2.
        assert_eq!(tree_scan_budget(None, 40_000), 80_000);
        // Huge db: the hard cap, never more.
        assert_eq!(tree_scan_budget(None, 9_000_000), HARD_MAX_TREE_BUDGET);
        assert_eq!(tree_scan_budget(None, u64::MAX), HARD_MAX_TREE_BUDGET);
    }

    #[test]
    fn requested_budget_is_clamped_but_never_scaled() {
        assert_eq!(tree_scan_budget(Some(10_000), 9_000_000), 10_000);
        assert_eq!(tree_scan_budget(Some(200_000), 0), 200_000);
        assert_eq!(tree_scan_budget(Some(5_000_000), 0), HARD_MAX_TREE_BUDGET);
        assert_eq!(tree_scan_budget(Some(1), 9_000_000), 1);
    }

    #[test]
    fn a_zero_request_is_the_derived_tier_not_a_one_round_budget() {
        // 审查发现 R-1, closed in favour of `## 契约冻结`: "budget 缺失或 0 ⇒ 派生档".
        // The old reading (Some(0) == "did you mean 1") contradicted the freeze and
        // only stayed invisible because the dispatch arm filtered 0 into None.
        assert_eq!(tree_scan_budget(Some(0), 0), tree_scan_budget(None, 0));
        assert_eq!(tree_scan_budget(Some(0), 0), DEFAULT_TREE_BUDGET);
        assert_eq!(
            tree_scan_budget(Some(0), 40_000),
            tree_scan_budget(None, 40_000)
        );
        assert_eq!(tree_scan_budget(Some(0), 9_000_000), HARD_MAX_TREE_BUDGET);
    }

    #[test]
    fn tree_budget_hard_cap_is_the_key_tree_one_not_value_searchs() {
        // Guarding the deliberate divergence: value search caps at 200 000 keys,
        // the key tree at 1 000 000 COUNT. See the module docs.
        assert_eq!(HARD_MAX_TREE_BUDGET, 1_000_000);
        assert_ne!(
            HARD_MAX_TREE_BUDGET,
            crate::ops::value_search::HARD_MAX_KEYS
        );
        assert_eq!(
            DEFAULT_TREE_BUDGET,
            crate::ops::value_search::DEFAULT_MAX_KEYS
        );
    }

    // --- the ledger ------------------------------------------------------

    #[test]
    fn next_count_never_offers_more_than_the_remainder() {
        let budget = ScanBudget::new(50_000);
        // A small page still asks for the round floor — rounds, not keys, are
        // what costs a round trip.
        assert_eq!(budget.next_count(50), Some(TREE_SCAN_MIN_ROUND_COUNT));
        assert_eq!(budget.next_count(100_000), Some(50_000));

        let mut nearly_spent = ScanBudget::new(1_000);
        nearly_spent.charge(995);
        assert_eq!(nearly_spent.remaining(), 5);
        // The remainder is below the minimum round, so the last round runs at
        // the floor rather than being skipped: progress beats pedantry, and it
        // is *reported* (see `truncated`).
        assert_eq!(nearly_spent.next_count(100), Some(MIN_TREE_SCAN_COUNT));
    }

    #[test]
    fn sum_of_charged_counts_stays_within_the_budget_while_credit_remains() {
        // The invariant behind I-2: walk a whole page loop and check the ledger
        // after every charge. As long as a full minimum round fits in the
        // remainder, no round may spend past the cap.
        let mut budget = ScanBudget::new(10_000);
        let mut rounds = 0usize;
        while let Some(count) = budget.next_count(1_000) {
            assert!(
                u64::from(count) <= budget.remaining(),
                "round of {count} offered past the {budget:?} remainder"
            );
            budget.charge(count);
            rounds += 1;
        }
        assert_eq!(rounds, 10);
        assert_eq!(budget.consumed(), 10_000);
        assert!(budget.truncated());
    }

    #[test]
    fn only_the_final_round_of_a_partial_budget_may_overshoot_by_one_min_round() {
        let mut budget = ScanBudget::new(1_005);
        budget.charge(1_000);
        let last = budget.next_count(1_000).expect("credit remains");
        assert_eq!(last, MIN_TREE_SCAN_COUNT);
        budget.charge(last);
        assert_eq!(
            budget.consumed(),
            1_010,
            "overshoot must stay within one minimum round"
        );
        assert!(budget.next_count(1_000).is_none(), "ledger is spent");
    }

    #[test]
    fn exhausted_budget_offers_no_further_round() {
        let mut budget = ScanBudget::new(2_000);
        assert_eq!(budget.next_count(2_000), Some(2_000));
        budget.charge(2_000);
        assert_eq!(budget.next_count(2_000), None);
        assert!(budget.is_exhausted());
        assert!(budget.truncated());
    }

    #[test]
    fn zero_limit_still_acts_as_one_round_min() {
        let budget = ScanBudget::new(0);
        assert_eq!(budget.limit(), 1);
        assert_eq!(budget.next_count(500), Some(MIN_TREE_SCAN_COUNT));
    }

    // --- loop guards ------------------------------------------------------

    #[test]
    fn guard_resets_stalled_counter_when_the_cursor_advances() {
        let mut guard = ScanLoopGuard::new();
        guard.record_round(false);
        guard.record_round(false);
        assert_eq!(guard.rounds(), 2);
        guard.record_round(true);
        assert!(!guard.cap_hit());
        for _ in 0..MAX_TREE_STALLED_ROUNDS {
            guard.record_round(false);
        }
        assert!(guard.cap_hit(), "stalled rounds must stop the loop");
    }

    #[test]
    fn guard_stops_a_never_wrapping_cursor_at_the_round_cap() {
        let mut guard = ScanLoopGuard::new();
        while !guard.cap_hit() {
            guard.record_round(true);
        }
        assert_eq!(guard.rounds(), MAX_TREE_SCAN_ROUNDS);
    }
}
