//! `count_matching`'s budgeted loop and its `CountOutcome` reply (`n+` is a floor, not a count).

use super::budget::read_dbsize;
use super::budget::scan_budgeted;
use crate::connect::Topology;
use crate::ops::key_probe::key_exists;
#[allow(unused_imports)]
use crate::ops::tree::budget::{is_exact_key_pattern, tree_scan_budget, ScanBudget, ScanLoopGuard};
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
use redis::aio::ConnectionLike;

/// Payload of the `count_matching` command.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountOutcome {
    /// Keys matched so far — a *partial* count when [`Self::truncated`].
    pub count: u64,
    /// The budget ran out before the cursor wrapped: the UI must label this
    /// `n+` rather than present it as a census.
    pub truncated: bool,
    /// Cumulative `COUNT` spent.
    pub consumed: u64,
    /// `DBSIZE`, read exactly once for this call.
    pub dbsize: u64,
}

/// Count keys matching `pattern` under one action budget.
///
/// Two shapes normally skip the scan entirely: `*` answers from `DBSIZE` (as
/// before, and at strictly less cost — one command instead of a full walk), and
/// an exact key name answers `0` or `1` with a single `EXISTS` (PRD §4 I-3,
/// "计数显示 1/1 而非 n+"). The `*` shortcut has one exception (see
/// [`read_dbsize`]): a `dbsize` of `0` is ambiguous between "this database is
/// empty" and "`DBSIZE` was refused", so it is verified by a real scan pass
/// before being published as a census.
pub(crate) async fn count_budgeted<C>(
    conn: &mut C,
    pattern: &str,
    budget: Option<u64>,
    topology: Topology,
) -> Result<CountOutcome, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let dbsize = read_dbsize(conn).await;
    // Both spellings of "everything" answer from DBSIZE .
    let matches_all = pattern.is_empty() || pattern == "*";
    if matches_all && dbsize > 0 {
        return Ok(CountOutcome {
            count: dbsize,
            truncated: false,
            consumed: 0,
            dbsize,
        });
    }
    if is_exact_key_pattern(pattern) {
        return Ok(CountOutcome {
            count: u64::from(key_exists(conn, pattern, topology).await?),
            truncated: false,
            consumed: 0,
            dbsize,
        });
    }

    // A `matches_all` request reaching here means `dbsize == 0`: spend one real
    // scan round instead of short-circuiting, so an unread DBSIZE cannot be sold
    // to the UI as "0 keys" (redis-tree-backend-BUG-003). A genuinely empty
    // database pays one extra round and the cursor wraps immediately. The filter
    // is omitted — `SCAN … MATCH *` is the no-op `scan_keys_page` declines too.
    let match_pat = (!matches_all).then_some(pattern);
    let mut ledger = ScanBudget::new(tree_scan_budget(budget, dbsize));
    let page = scan_budgeted(
        conn,
        0,
        crate::ops::tree::budget::TREE_SCAN_MIN_ROUND_COUNT,
        usize::MAX,
        match_pat,
        None,
        &mut ledger,
        topology,
    )
    .await?;
    // With `stop_at = usize::MAX` a page is never "full", so `truncated` is
    // exactly "the cursor did not wrap": reading `exhausted` directly keeps the
    // `n+` signal tied to its definition rather than to the loop's arithmetic.
    let truncated = !page.exhausted;
    Ok(CountOutcome {
        count: page.keys.len() as u64,
        truncated,
        consumed: page.consumed,
        dbsize,
    })
}
